use crate::diagnostics::typeck::lower_ty::{
    report_arg_count, report_dyn_not_a_trait, report_self_cycle, report_self_outside_item,
    report_trait_as_ty, report_unexpected_generic_args,
};
use crate::driver::source::SrcSpan;
use crate::hir::{DefId, HirId, OwnerNode, Res, TyDef, TyKind as HirTyKind, Type};
use crate::typeck::Typeck;
use crate::typeck::ty::Ty;

impl<'hir> Typeck<'hir> {
    pub fn lower_ty(&mut self, id: HirId) -> Ty {
        let ty = self.hir.ty(id);
        let span = ty.span;

        let lowered = match &ty.kind {
            HirTyKind::Path { path, args } => {
                let (res, args) = (path.res, args.clone());
                self.lower_base(id, res, &args, span)
            }
            HirTyKind::Ref { base, mutability } => {
                let (base, mutability) = (*base, *mutability);
                let base = self.lower_ty(base);
                self.tcx.mk_ref(base, mutability)
            }
            HirTyKind::Any(base) => {
                let base = self.lower_ty(*base);
                self.tcx.mk_any(base)
            }
            HirTyKind::Tuple(elems) => {
                let elems = elems.clone();
                let elems = self.lower_tys(&elems);
                self.tcx.mk_tuple(elems)
            }
            HirTyKind::Array { elem, len } => {
                let (elem, len) = (*elem, *len);
                let elem = self.lower_ty(elem);
                self.tcx.mk_array(elem, len)
            }
            HirTyKind::Function { params, ret } => {
                let (params, ret) = (params.clone(), *ret);
                let params = self.lower_tys(&params);
                let ret = ret.map(|ret| self.lower_ty(ret));
                self.tcx.mk_fun(params, ret)
            }
            HirTyKind::Dyn { path, args } => {
                let (res, args) = (path.res, args.clone());
                self.lower_dyn(id, res, &args, span)
            }
            HirTyKind::Error => self.tcx.error(),
        };

        self.types.record(id, lowered);
        lowered
    }

    pub fn lower_tys(&mut self, ids: &[HirId]) -> Vec<Ty> {
        ids.iter().map(|&id| self.lower_ty(id)).collect()
    }

    fn lower_base(&mut self, id: HirId, res: Res, args: &[HirId], span: SrcSpan) -> Ty {
        match res {
            Res::Type(Type::Prim(prim)) => {
                Self::expect_no_args(args, span, "a primitive type");
                self.tcx.mk_prim(prim)
            }
            Res::Type(Type::Generic(param)) => {
                Self::expect_no_args(args, span, "a generic type parameter");
                self.tcx.mk_generic(param)
            }
            Res::Type(Type::Def(TyDef::Struct(def_id) | TyDef::Enum(def_id))) => {
                let declared = match self.hir.def(def_id) {
                    OwnerNode::Struct(struct_) => struct_.generics.len(),
                    OwnerNode::Enum(enum_) => enum_.generics.len(),
                    _ => unreachable!("a TyDef::Struct/Enum always names a Struct/Enum owner"),
                };
                self.lower_def(def_id, declared, args, span, id.owner)
            }
            // A trait names every type that implements it, not one type of its own: it is a
            // type only spelled `dyn Trait` ([`Typeck::lower_dyn`]), or usable as a bound on a
            // generic parameter ([`Typeck::collect_bounds`]) -- neither of which is an ordinary
            // type position, so a bare trait reaching one here is always a mistake.
            Res::Type(Type::Def(TyDef::Trait(_))) => {
                report_trait_as_ty(span);
                self.tcx.error()
            }
            Res::SelfTy(_) => {
                Self::expect_no_args(args, span, "`Self`");
                self.self_ty(id.owner, span)
            }
            Res::Err => self.tcx.error(),
            Res::Local(_) | Res::Function(_) | Res::Module(_) => {
                unreachable!(
                    "name resolution never resolves a type-position path to a local, a \
                     function, or a module"
                )
            }
        }
    }

    fn lower_def(
        &mut self,
        def_id: DefId,
        declared: usize,
        args: &[HirId],
        span: SrcSpan,
        owner: DefId,
    ) -> Ty {
        if args.len() != declared {
            report_arg_count(span, declared, args.len());
            return self.tcx.error();
        }

        let args = self.lower_tys(args);
        self.register_bound_obligations(def_id, &args, span, owner);
        self.tcx.mk_adt(def_id, args)
    }

    fn lower_dyn(&mut self, id: HirId, res: Res, args: &[HirId], span: SrcSpan) -> Ty {
        match res {
            Res::Type(Type::Def(TyDef::Trait(def_id))) => {
                if !self.check_arg_count(def_id, args.len(), span) {
                    return self.tcx.error();
                }
                let args = self.lower_tys(args);
                self.register_bound_obligations(def_id, &args, span, id.owner);
                self.tcx.mk_dyn(def_id, args)
            }
            Res::Err => self.tcx.error(),
            _ => {
                report_dyn_not_a_trait(span);
                self.tcx.error()
            }
        }
    }

    pub fn self_ty(&mut self, owner_id: DefId, span: SrcSpan) -> Ty {
        let mut introducer = owner_id;
        loop {
            if matches!(
                self.hir.def(introducer),
                OwnerNode::Struct(_)
                    | OwnerNode::Enum(_)
                    | OwnerNode::Trait(_)
                    | OwnerNode::Extend(_)
            ) {
                break;
            }
            match self.hir.parent(introducer) {
                Some(parent) => introducer = parent,
                None => {
                    report_self_outside_item(span);
                    return self.tcx.error();
                }
            }
        }

        if let Some(&cached) = self.self_tys.get(&introducer) {
            return cached;
        }

        if !self.computing_self_tys.insert(introducer) {
            report_self_cycle(span);
            return self.tcx.error();
        }

        let self_ty = match self.hir.def(introducer) {
            OwnerNode::Struct(struct_) => {
                let params = struct_.generics.clone();
                self.adt_of_own_params(introducer, &params)
            }
            OwnerNode::Enum(enum_) => {
                let params = enum_.generics.clone();
                self.adt_of_own_params(introducer, &params)
            }
            OwnerNode::Trait(_) => self.tcx.mk_self_param(introducer),
            OwnerNode::Extend(extend) => {
                let adt_res = extend.adt_path.res;
                let args = extend.adt_generics.clone();
                let args = self.lower_tys(&args);
                match adt_res {
                    Res::Type(Type::Def(tydef)) => self.tcx.mk_adt(tydef.def_id(), args),
                    _ => self.tcx.error(),
                }
            }
            _ => unreachable!("only a struct, enum, trait, or extend block introduces a `Self`"),
        };

        self.computing_self_tys.remove(&introducer);
        self.self_tys.insert(introducer, self_ty);
        self_ty
    }

    fn adt_of_own_params(&mut self, def_id: DefId, params: &[HirId]) -> Ty {
        let args = params.iter().map(|&id| self.tcx.mk_generic(id)).collect();
        self.tcx.mk_adt(def_id, args)
    }

    fn expect_no_args(args: &[HirId], span: SrcSpan, kind: &str) {
        if !args.is_empty() {
            report_unexpected_generic_args(kind, span);
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::ast::Mutability;
    use crate::ast::interner::Interner;
    use crate::diagnostics::DiagCtx;
    use crate::hir::{DefId, Hir, HirId, OwnerNode};
    use crate::nameres::PrimTy;
    use crate::testing::resolve_src;
    use crate::typeck::results::TypeResolutions;
    use crate::typeck::ty::{Ty, TyKind};
    use crate::typeck::tyctx::TyCtx;

    /// Everything a lowered program's types are looked up through. The four travel together
    /// because a `Ty` is an index into `tcx`, and a `TypeResolutions` entry is keyed by a `HirId`
    /// that only means something against `hir`.
    struct Checked {
        hir: Hir,
        tcx: TyCtx,
        types: TypeResolutions,
    }

    /// Runs `src` through the whole pipeline up to and including `collect`.
    ///
    /// Diagnostics are cleared after name resolution, so what a test sees afterwards is only
    /// what type collection itself reported. Name resolution always reports every lang item as
    /// missing here: the core library is not registered for these tests, since compiling it
    /// alongside a two-line fixture would swamp what each test is about.
    fn check(src: &str) -> Checked {
        let hir = resolve_src(src);
        DiagCtx::clear();

        let checked = crate::typeck::check(&hir);
        Checked {
            hir,
            tcx: checked.tcx,
            types: checked.types,
        }
    }

    /// The messages `collect` reported, in order.
    fn diagnostics() -> Vec<String> {
        DiagCtx::diagnostics()
            .into_iter()
            .map(|diagnostic| diagnostic.message)
            .collect()
    }

    impl Checked {
        /// The `DefId` of the top-level definition named `name`.
        fn def(&self, name: &str) -> DefId {
            let root = self.hir.root();
            root.items
                .iter()
                .copied()
                .find(|&id| {
                    let named = match self.hir.def(id) {
                        OwnerNode::Function(f) => f.name,
                        OwnerNode::Struct(s) => s.name,
                        OwnerNode::Enum(e) => e.name,
                        OwnerNode::Trait(t) => t.name,
                        _ => return false,
                    };
                    Interner::resolve(named.text) == name
                })
                .unwrap_or_else(|| panic!("no definition named {name:?}"))
        }

        /// The `DefId` of the program's sole `extend` block.
        fn extend(&self) -> DefId {
            crate::testing::first_extend(&self.hir)
        }

        /// The type recorded for a definition as a whole.
        fn def_ty(&self, def: DefId) -> Ty {
            self.types
                .ty_of_def(def)
                .expect("this definition's type was never recorded")
        }

        fn ty(&self, hir_id: HirId) -> Ty {
            self.types
                .ty(hir_id)
                .unwrap_or_else(|| panic!("no type recorded for {hir_id:?}"))
        }

        fn kind(&self, ty: Ty) -> &TyKind {
            self.tcx.kind(ty)
        }

        /// The signature of the function `name` declares, as `(params, ret)`.
        fn sig(&self, def_id: DefId) -> (&[Ty], Option<Ty>) {
            let TyKind::Fun { params, ret } = self.kind(self.def_ty(def_id)) else {
                panic!("a function's type is always a Fun type");
            };
            (params, *ret)
        }

        /// The `Ty` of the `i`th generic parameter `def_id` declares.
        fn generic(&self, def_id: DefId, i: usize) -> Ty {
            let generics = match self.hir.def(def_id) {
                OwnerNode::Struct(s) => &s.generics,
                OwnerNode::Enum(e) => &e.generics,
                OwnerNode::Trait(t) => &t.generics,
                OwnerNode::Function(f) => &f.generics,
                OwnerNode::Extend(e) => &e.extend_generics,
                _ => panic!("this definition declares no generics"),
            };
            self.ty(generics[i])
        }
    }

    #[test]
    fn a_primitive_annotation_lowers_to_its_primitive_type() {
        let checked = check("fun f(x: i32, y: bool) -> i64 { return 0; }");
        let f = checked.def("f");
        let (params, ret) = checked.sig(f);

        assert_eq!(checked.kind(params[0]), &TyKind::Primitive(PrimTy::I32));
        assert_eq!(checked.kind(params[1]), &TyKind::Primitive(PrimTy::Bool));
        assert_eq!(
            checked.kind(ret.expect("f declares a return type")),
            &TyKind::Primitive(PrimTy::I64)
        );
        assert!(diagnostics().is_empty());
    }

    #[test]
    fn a_function_without_a_return_type_has_none() {
        let checked = check("fun f() {}");
        let (params, ret) = checked.sig(checked.def("f"));
        assert!(params.is_empty());
        assert_eq!(ret, None);
    }

    #[test]
    fn a_generic_parameter_lowers_to_the_node_that_declares_it() {
        let checked = check("fun identity<T>(x: T) -> T {}");
        let identity = checked.def("identity");
        let (params, ret) = checked.sig(identity);

        assert_eq!(params[0], checked.generic(identity, 0));
        assert_eq!(ret, Some(params[0]));
    }

    #[test]
    fn a_struct_is_itself_applied_to_its_own_parameters() {
        let checked = check("struct Map<K, V> { key: K, value: V }");
        let map = checked.def("Map");
        let (k, v) = (checked.generic(map, 0), checked.generic(map, 1));

        assert_eq!(
            checked.kind(checked.def_ty(map)),
            &TyKind::Adt {
                def: map,
                args: vec![k, v],
            }
        );
        assert!(matches!(checked.kind(k), TyKind::Generic(_)));
        assert_ne!(k, v);
    }

    #[test]
    fn a_field_annotation_naming_a_parameter_lowers_to_that_parameter() {
        let checked = check("struct Wrap<T> { inner: T }");
        let wrap = checked.def("Wrap");
        let struct_ = checked.hir.struct_(wrap);
        let field = checked.ty(struct_.fields[0]);

        assert_eq!(field, checked.generic(wrap, 0));
    }

    #[test]
    fn generic_arguments_are_lowered_positionally() {
        let checked = check(
            "struct Wrap<T> { inner: T }
             fun f(x: Wrap<i32>) {}",
        );
        let wrap = checked.def("Wrap");
        let (params, _) = checked.sig(checked.def("f"));

        let TyKind::Adt { def, args } = checked.kind(params[0]) else {
            panic!("Wrap<i32> lowers to an Adt");
        };
        assert_eq!(*def, wrap);
        assert_eq!(checked.kind(args[0]), &TyKind::Primitive(PrimTy::I32));
    }

    #[test]
    fn nested_generic_arguments_are_lowered() {
        let checked = check(
            "struct Wrap<T> { inner: T }
             fun f(x: Wrap<Wrap<bool>>) {}",
        );
        let (params, _) = checked.sig(checked.def("f"));

        let TyKind::Adt { args, .. } = checked.kind(params[0]) else {
            panic!("Wrap<..> lowers to an Adt");
        };
        let TyKind::Adt { args: inner, .. } = checked.kind(args[0]) else {
            panic!("the argument is itself an Adt");
        };
        assert_eq!(checked.kind(inner[0]), &TyKind::Primitive(PrimTy::Bool));
    }

    #[test]
    fn the_wrong_number_of_generic_arguments_is_reported() {
        let checked = check(
            "struct Wrap<T> { inner: T }
             fun f(x: Wrap<i32, bool>) {}",
        );
        let (params, _) = checked.sig(checked.def("f"));

        assert_eq!(checked.kind(params[0]), &TyKind::Error);
        assert_eq!(
            diagnostics(),
            ["this type takes 1 generic argument but 2 were supplied"]
        );
    }

    #[test]
    fn generic_arguments_on_a_type_parameter_are_reported() {
        let checked = check("fun f<T>(x: T<i32>) {}");
        let f = checked.def("f");
        let (params, _) = checked.sig(f);

        // The arguments are dropped rather than the whole annotation: `T` is still the type the
        // parameter has, so a body mentioning `x` can be checked against something.
        assert_eq!(params[0], checked.generic(f, 0));
        assert_eq!(
            diagnostics(),
            ["a generic type parameter takes no generic arguments"]
        );
    }

    #[test]
    fn compound_annotations_lower_structurally() {
        let checked = check("fun f(a: &i32, b: (i32, bool), c: [bool; 4], d: any i32) {}");
        let (params, _) = checked.sig(checked.def("f"));

        let TyKind::Ref { base, mutability } = checked.kind(params[0]) else {
            panic!("&i32 lowers to a Ref");
        };
        assert_eq!(*mutability, Mutability::Immutable);
        assert_eq!(checked.kind(*base), &TyKind::Primitive(PrimTy::I32));

        let TyKind::Tuple(elems) = checked.kind(params[1]) else {
            panic!("(i32, bool) lowers to a Tuple");
        };
        assert_eq!(checked.kind(elems[0]), &TyKind::Primitive(PrimTy::I32));
        assert_eq!(checked.kind(elems[1]), &TyKind::Primitive(PrimTy::Bool));

        let TyKind::Array { elem, len } = checked.kind(params[2]) else {
            panic!("[bool; 4] lowers to an Array");
        };
        assert_eq!(checked.kind(*elem), &TyKind::Primitive(PrimTy::Bool));
        assert!(
            len.is_some(),
            "the length expression is addressed, not dropped"
        );

        let TyKind::Any(base) = checked.kind(params[3]) else {
            panic!("any i32 lowers to an Any");
        };
        assert_eq!(checked.kind(*base), &TyKind::Primitive(PrimTy::I32));
    }

    /// A trait names every type that implements it, not one type of its own, so writing it bare
    /// in a type position -- rather than `dyn Show`, or as a bound on a generic parameter -- is
    /// a mistake, not a shorthand.
    #[test]
    fn a_bare_trait_used_as_a_type_is_rejected() {
        let checked = check(
            "trait Show { fun show(&self); }
             fun f(x: Show) {}",
        );
        let (params, _) = checked.sig(checked.def("f"));

        assert_eq!(checked.kind(params[0]), &TyKind::Error);
        assert_eq!(
            diagnostics(),
            ["a trait cannot be used as a type on its own"]
        );
    }

    /// The same rejection wherever a trait is named in an ordinary type position, not only a
    /// parameter -- and it is the only diagnostic even though `Index` is applied to arguments
    /// here too: naming the trait bare is already the whole mistake, so there is nothing to gain
    /// from also complaining about its argument count.
    #[test]
    fn a_bare_trait_is_rejected_in_a_field_even_when_applied_to_arguments() {
        let checked = check(
            "trait Index<K, V> { fun index(&self, key: K) -> &V; }
             struct Wrap { inner: Index<i32, bool> }",
        );
        let wrap = checked.def("Wrap");
        let struct_ = checked.hir.struct_(wrap);
        let field = checked.ty(struct_.fields[0]);

        assert_eq!(checked.kind(field), &TyKind::Error);
        assert_eq!(
            diagnostics(),
            ["a trait cannot be used as a type on its own"]
        );
    }

    #[test]
    fn dyn_lowers_to_the_trait_it_names() {
        let checked = check(
            "trait Show { fun show(&self); }
             fun f(x: &dyn Show) {}",
        );
        let show = checked.def("Show");
        let (params, _) = checked.sig(checked.def("f"));

        let TyKind::Ref { base, .. } = checked.kind(params[0]) else {
            panic!("&dyn Show lowers to a Ref");
        };
        assert_eq!(
            checked.kind(*base),
            &TyKind::Dyn {
                trait_: show,
                args: Vec::new(),
            }
        );
    }

    /// The case `dyn` could not express until it carried an argument list of its own: a trait
    /// that declares parameters, applied to them.
    #[test]
    fn dyn_carries_the_traits_generic_arguments() {
        let checked = check(
            "trait Index<K, V> { fun index(&self, key: K) -> &V; }
             fun f(x: &dyn Index<i32, bool>) {}",
        );
        let index = checked.def("Index");
        let (params, _) = checked.sig(checked.def("f"));

        let TyKind::Ref { base, .. } = checked.kind(params[0]) else {
            panic!("&dyn Index<i32, bool> lowers to a Ref");
        };
        let TyKind::Dyn { trait_, args } = checked.kind(*base) else {
            panic!("dyn Index<i32, bool> lowers to a Dyn");
        };
        assert_eq!(*trait_, index);
        assert_eq!(
            args.iter()
                .map(|&arg| checked.kind(arg))
                .collect::<Vec<_>>(),
            [
                &TyKind::Primitive(PrimTy::I32),
                &TyKind::Primitive(PrimTy::Bool)
            ]
        );
    }

    /// A `dyn` applied to the wrong number of arguments is an error for the same reason a struct
    /// is -- and, unlike before, one the user can now fix by writing the arguments.
    #[test]
    fn dyn_checks_the_traits_argument_count() {
        let checked = check(
            "trait Index<K, V> { fun index(&self, key: K) -> &V; }
             fun f(x: &dyn Index<i32>) {}",
        );
        let (params, _) = checked.sig(checked.def("f"));

        let TyKind::Ref { base, .. } = checked.kind(params[0]) else {
            panic!("&dyn Index<i32> lowers to a Ref");
        };
        assert_eq!(checked.kind(*base), &TyKind::Error);
        assert_eq!(
            DiagCtx::diagnostics()
                .into_iter()
                .map(|diagnostic| diagnostic.message)
                .collect::<Vec<_>>(),
            ["`Index` takes 2 generic arguments but 1 was supplied"]
        );
    }

    #[test]
    fn self_inside_a_struct_is_the_struct_applied_to_its_parameters() {
        let checked = check("struct Wrap<T> { inner: Self }");
        let wrap = checked.def("Wrap");
        let struct_ = checked.hir.struct_(wrap);
        let field = checked.ty(struct_.fields[0]);

        assert_eq!(field, checked.def_ty(wrap));
        assert_eq!(
            checked.kind(field),
            &TyKind::Adt {
                def: wrap,
                args: vec![checked.generic(wrap, 0)],
            }
        );
    }

    #[test]
    fn self_inside_a_trait_stays_the_traits_own_parameter() {
        let checked = check("trait Show { fun clone(&self) -> Self; }");
        let show = checked.def("Show");

        assert_eq!(checked.kind(checked.def_ty(show)), &TyKind::SelfTy(show));
    }

    #[test]
    fn self_inside_an_extend_is_the_type_being_extended() {
        let checked = check(
            "struct Wrap<T> { inner: T }
             extend<T> Wrap<T> { fun get(&self) -> Self {} }",
        );
        let wrap = checked.def("Wrap");
        let extend = checked.extend();

        assert_eq!(
            checked.kind(checked.def_ty(extend)),
            &TyKind::Adt {
                def: wrap,
                args: vec![checked.generic(extend, 0)],
            }
        );
    }

    #[test]
    fn an_extends_parameters_are_distinct_from_the_structs_own() {
        let checked = check(
            "struct Wrap<T> { inner: T }
             extend<T> Wrap<T> { fun get(&self) -> Self {} }",
        );
        assert_ne!(
            checked.generic(checked.def("Wrap"), 0),
            checked.generic(checked.extend(), 0)
        );
    }

    #[test]
    fn a_method_takes_self_as_its_first_parameter() {
        let checked = check(
            "struct Wrap<T> { inner: T }
             extend<T> Wrap<T> { fun get(&self, other: i32) -> Self {} }",
        );
        let extend = checked.extend();
        let block = checked.hir.extend(extend);
        let (params, ret) = checked.sig(block.methods[0]);

        assert_eq!(params.len(), 2, "`self` counts as a parameter");
        let TyKind::Ref { base, mutability } = checked.kind(params[0]) else {
            panic!("&self lowers to a Ref");
        };
        assert_eq!(*mutability, Mutability::Immutable);
        assert_eq!(*base, checked.def_ty(extend));
        assert_eq!(checked.kind(params[1]), &TyKind::Primitive(PrimTy::I32));
        assert_eq!(ret, Some(checked.def_ty(extend)));
    }

    #[test]
    fn structurally_equal_annotations_share_one_handle() {
        let checked = check("fun f(a: (i32, bool), b: (i32, bool)) {}");
        let (params, _) = checked.sig(checked.def("f"));
        assert_eq!(params[0], params[1]);
    }

    // -----------------------------------------------------------------
    // Deeper composition
    // -----------------------------------------------------------------

    /// A reference to a reference. Written with a space (`& &i32`) rather than `&&i32`, since
    /// the lexer tokenizes `&&` as one `DoubleAmp` token (the logical-and operator) rather than
    /// two `&`s.
    #[test]
    fn a_reference_to_a_reference_lowers_to_nested_refs() {
        let checked = check("fun f(x: & &i32) {}");
        let (params, _) = checked.sig(checked.def("f"));

        let TyKind::Ref { base: outer, .. } = checked.kind(params[0]) else {
            panic!("& &i32 lowers to a Ref");
        };
        let TyKind::Ref { base: inner, .. } = checked.kind(*outer) else {
            panic!("the outer Ref's base is itself a Ref");
        };
        assert_eq!(checked.kind(*inner), &TyKind::Primitive(PrimTy::I32));
    }

    /// `&any T` composes; the other order does not exist to test -- the parser's `any_target`
    /// only accepts a primitive, a path, a tuple, an array, or `Self`, so `any` can never wrap a
    /// reference (`any &T` is a parse error, not a typeck question).
    #[test]
    fn a_reference_may_wrap_any() {
        let checked = check("fun f(x: &any i32) {}");
        let (params, _) = checked.sig(checked.def("f"));

        let TyKind::Ref { base, .. } = checked.kind(params[0]) else {
            panic!("&any i32 lowers to a Ref wrapping Any");
        };
        assert!(matches!(checked.kind(*base), TyKind::Any(_)));
    }

    #[test]
    fn a_function_type_is_usable_as_a_parameter_annotation() {
        let checked = check("fun f(callback: fun(i32) -> bool) {}");
        let (params, _) = checked.sig(checked.def("f"));

        let TyKind::Fun { params: inner, ret } = checked.kind(params[0]) else {
            panic!("fun(i32) -> bool lowers to a Fun type");
        };
        assert_eq!(checked.kind(inner[0]), &TyKind::Primitive(PrimTy::I32));
        assert_eq!(
            ret.map(|r| checked.kind(r).clone()),
            Some(TyKind::Primitive(PrimTy::Bool))
        );
    }

    #[test]
    fn a_tuple_of_function_types_lowers_elementwise() {
        let checked = check("fun f(x: (fun() -> i32, fun() -> bool)) {}");
        let (params, _) = checked.sig(checked.def("f"));

        let TyKind::Tuple(elems) = checked.kind(params[0]) else {
            panic!("expected a tuple type");
        };
        assert!(matches!(checked.kind(elems[0]), TyKind::Fun { .. }));
        assert!(matches!(checked.kind(elems[1]), TyKind::Fun { .. }));
    }

    #[test]
    fn an_array_of_tuples_and_a_tuple_of_arrays_both_lower() {
        let checked = check("fun f(a: [(i32, bool); 3], b: ([i32; 2], [bool; 2])) {}");
        let (params, _) = checked.sig(checked.def("f"));

        let TyKind::Array { elem, .. } = checked.kind(params[0]) else {
            panic!("expected an array type");
        };
        assert!(matches!(checked.kind(*elem), TyKind::Tuple(_)));

        let TyKind::Tuple(elems) = checked.kind(params[1]) else {
            panic!("expected a tuple type");
        };
        assert!(matches!(checked.kind(elems[0]), TyKind::Array { .. }));
        assert!(matches!(checked.kind(elems[1]), TyKind::Array { .. }));
    }

    #[test]
    fn generic_arguments_nest_three_levels_deep() {
        let checked = check(
            "struct Wrap<T> { inner: T }
             fun f(x: Wrap<Wrap<Wrap<i32>>>) {}",
        );
        let (params, _) = checked.sig(checked.def("f"));

        let TyKind::Adt { args: l1, .. } = checked.kind(params[0]) else {
            panic!("expected an Adt");
        };
        let TyKind::Adt { args: l2, .. } = checked.kind(l1[0]) else {
            panic!("expected a nested Adt");
        };
        let TyKind::Adt { args: l3, .. } = checked.kind(l2[0]) else {
            panic!("expected a doubly nested Adt");
        };
        assert_eq!(checked.kind(l3[0]), &TyKind::Primitive(PrimTy::I32));
    }

    /// A struct's own field may itself be a function type over the struct's generic parameter --
    /// `T` inside `fun(T) -> T` reaches the same node `Wrap`'s own generic does.
    #[test]
    fn a_generic_field_may_be_a_function_type_over_the_structs_own_parameter() {
        let checked = check("struct Container<T> { f: fun(T) -> T }");
        let container = checked.def("Container");
        let struct_ = checked.hir.struct_(container);
        let field = checked.ty(struct_.fields[0]);

        let TyKind::Fun { params, ret } = checked.kind(field) else {
            panic!("fun(T) -> T lowers to a Fun type");
        };
        let t = checked.generic(container, 0);
        assert_eq!(params[0], t);
        assert_eq!(*ret, Some(t));
    }

    /// `Self` used inside a tuple field. Typeck does not size-check types (there is no codegen
    /// yet to make an infinitely-sized type observable), so this lowers exactly like any other
    /// composite containing an `Adt` -- nothing here rejects a struct that could never actually
    /// be constructed.
    #[test]
    fn self_may_appear_nested_inside_a_tuple_field() {
        let checked = check("struct Wrap<T> { pair: (Self, i32) }");
        let wrap = checked.def("Wrap");
        let struct_ = checked.hir.struct_(wrap);
        let field = checked.ty(struct_.fields[0]);

        let TyKind::Tuple(elems) = checked.kind(field) else {
            panic!("(Self, i32) lowers to a Tuple");
        };
        assert_eq!(elems[0], checked.def_ty(wrap));
    }

    /// A trait declaring more than one method, each mentioning both the trait's own generic
    /// parameter and `Self`.
    #[test]
    fn a_traits_own_generic_and_self_both_appear_across_its_methods() {
        let checked = check(
            "trait Container<T> {
                 fun get(&self) -> T;
                 fun replace(&mut self, v: T) -> Self;
             }",
        );
        let container = checked.def("Container");
        let trait_ = checked.hir.trait_(container);

        let get_sig = checked.sig(trait_.functions[0]);
        assert_eq!(get_sig.1, Some(checked.generic(container, 0)));

        let (replace_params, replace_ret) = checked.sig(trait_.functions[1]);
        assert_eq!(replace_params[1], checked.generic(container, 0));
        assert_eq!(replace_ret, Some(checked.def_ty(container)));
    }
}
