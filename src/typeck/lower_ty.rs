//! Turns a type *annotation* -- a [`hir::Ty`](crate::hir::Ty), which is what the user wrote --
//! into a [`Ty`], which is what the checker reasons about.
//!
//! The conversion is mostly mechanical, because name resolution has already done the hard part:
//! every path in a `hir::Ty` was resolved to a [`TypeRes`] and recorded under that node's
//! [`HirId`], so this pass reads the answer instead of doing its own lookups. What is left is to
//! replace each nested `HirId` with the `Ty` it lowers to, and to check the things that only
//! become checkable once a path has a definition behind it: that a type is applied to as many
//! generic arguments as it declares parameters, and that what a path names can be used as a type
//! at all.
//!
//! Every annotation lowered here is recorded in [`TypeResolutions`](crate::typeck::results::TypeResolutions)
//! under its own `HirId`, so later passes can ask what a written type meant without repeating
//! the walk.

use crate::diag::{DiagCtx, Diagnostic};
use crate::hir::{DefId, HirId, Node, OwnerNode, TyKind as HirTyKind};
use crate::lexer::src_span::SrcSpan;
use crate::nameres::results::{SelfTyRes, TypeRes};
use crate::typeck::Typeck;
use crate::typeck::ty::Ty;

impl<'hir> Typeck<'hir> {
    /// Lowers the annotation `id` names, recording the result under that same [`HirId`] before
    /// returning it.
    pub fn lower_ty(&mut self, id: HirId) -> Ty {
        let Node::Ty(ty) = self.hir.node(id) else {
            unreachable!("expected a ty id to name a ty");
        };
        let span = ty.span;

        let lowered = match &ty.kind {
            HirTyKind::Path { args, .. } => {
                // Collected first so the `&self` borrow of the node ends before lowering the
                // arguments, which needs `&mut self`.
                let args: Vec<HirId> = args.clone();
                self.lower_base(id, &args, span)
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
                // The length expression is addressed, not evaluated -- see `TyKind::Array`.
                self.tcx.mk_array(elem, len)
            }
            HirTyKind::Function { params, ret } => {
                let (params, ret) = (params.clone(), *ret);
                let params = self.lower_tys(&params);
                let ret = ret.map(|ret| self.lower_ty(ret));
                self.tcx.mk_fun(params, ret)
            }
            HirTyKind::SelfType => self.self_ty(id.owner, span),
            HirTyKind::Dyn { args, .. } => {
                let args: Vec<HirId> = args.clone();
                self.lower_dyn(id, &args, span)
            }
            HirTyKind::Error => self.tcx.error(),
        };

        self.types.record(id, lowered);
        lowered
    }

    /// Lowers a list of annotations sitting in the same arena, such as a tuple's elements or a
    /// type's generic arguments.
    pub fn lower_tys(&mut self, ids: &[HirId]) -> Vec<Ty> {
        ids.iter().map(|&id| self.lower_ty(id)).collect()
    }

    /// Lowers a named type: `i32`, `T`, `Map<K, V>`, and so on. `id` addresses the annotation
    /// itself, which is the key name resolution recorded its answer under.
    fn lower_base(&mut self, id: HirId, args: &[HirId], span: SrcSpan) -> Ty {
        // A missing entry means name resolution gave up on this annotation before recording an
        // answer -- an `extend` block's own `<...>` list holding something that isn't a bare
        // parameter name is the one way that happens. It has already been reported, so this
        // stays quiet.
        let Some(res) = self.nameres.ty(id) else {
            return self.tcx.error();
        };

        match res {
            TypeRes::PrimTy(prim) => {
                Self::expect_no_args(args, span, "a primitive type");
                self.tcx.mk_prim(prim)
            }
            TypeRes::Generic(param) => {
                Self::expect_no_args(args, span, "a generic type parameter");
                self.tcx.mk_generic(param)
            }
            TypeRes::Def(def_id) => {
                let declared = match self.hir.def(def_id) {
                    OwnerNode::Struct(struct_) => struct_.generics.len(),
                    OwnerNode::Enum(enum_) => enum_.generics.len(),
                    // A trait names a set of types rather than one type, so it can only be used
                    // in type position through `dyn`.
                    OwnerNode::Trait(_) => {
                        Self::report_trait_as_ty(span);
                        return self.tcx.error();
                    }
                    _ => {
                        // Name resolution only ever puts structs, enums, and traits in the type
                        // namespace, so nothing else can turn up here.
                        Self::report_not_a_ty(span);
                        return self.tcx.error();
                    }
                };

                if args.len() != declared {
                    Self::report_arg_count(span, declared, args.len());
                    return self.tcx.error();
                }

                let args = self.lower_tys(args);
                // Writing the type is what instantiates it, so this is where its declared bounds
                // become something to prove. Deferred rather than proved: the arguments may still
                // be inference variables, and the index may not exist yet. See
                // [`bounds`](crate::typeck::traits::bounds).
                self.register_bound_obligations(def_id, &args, span, id.owner);
                self.tcx.mk_adt(def_id, args)
            }
            // Already reported by name resolution; staying quiet here keeps one mistake from
            // producing a second diagnostic.
            TypeRes::Err => self.tcx.error(),
        }
    }

    /// Lowers `dyn Trait`, or `dyn Trait<K, V>` for a trait that declares parameters.
    ///
    /// A trait applied to the wrong number of arguments is an error for the same reason a struct
    /// is: a half-applied trait is not a type, and letting one reach the solver would mean
    /// matching a goal against an argument list nobody wrote.
    fn lower_dyn(&mut self, id: HirId, args: &[HirId], span: SrcSpan) -> Ty {
        let Some(res) = self.nameres.ty(id) else {
            return self.tcx.error();
        };

        match res {
            TypeRes::Def(def_id) if matches!(self.hir.def(def_id), OwnerNode::Trait(_)) => {
                if !self.check_arg_count(def_id, args.len(), span) {
                    return self.tcx.error();
                }
                let args = self.lower_tys(args);
                // A `dyn Trait` instantiates the trait's parameters like any other application of
                // it, so whatever they are bounded by has to hold of these arguments.
                self.register_bound_obligations(def_id, &args, span, id.owner);
                self.tcx.mk_dyn(def_id, args)
            }
            TypeRes::Err => self.tcx.error(),
            _ => {
                Self::report_dyn_not_a_trait(span);
                self.tcx.error()
            }
        }
    }

    /// What `Self` means inside `owner_id`.
    ///
    /// Name resolution already worked out *which* definition `Self` refers to; what is left is to
    /// give that definition its generic arguments, and those depend on where the `Self` was
    /// written:
    ///
    /// - inside a `struct` or `enum`, `Self` is that type applied to its own parameters, so
    ///   `Self` inside `struct Map<K, V>` is `Map<K, V>`;
    /// - inside an `extend` block, it is whatever the block targets, so `Self` inside
    ///   `extend<K, V> Map<K, V>` is again `Map<K, V>` -- but by way of the block's arguments,
    ///   which need not be bare parameters (`extend Map<i32, bool>` gives `Map<i32, bool>`);
    /// - inside a trait, there is no concrete type yet, so it stays the trait's own
    ///   [`SelfTy`](crate::typeck::ty::TyKind::SelfTy) until an `extend` substitutes it.
    ///
    /// Results are cached per definition, since `Self` is typically written many times in one
    /// body and the `extend` case has to lower a list of arguments each time it is computed.
    pub fn self_ty(&mut self, owner_id: DefId, span: SrcSpan) -> Ty {
        // `Self` is introduced by an enclosing definition, not necessarily by `owner_id` itself:
        // a method names the `Self` of the `extend` block or trait it is declared in.
        let mut introducer = owner_id;
        let res = loop {
            if let Some(res) = self.nameres.self_ty(introducer) {
                break res;
            }
            match self.hir.parent(introducer) {
                Some(parent) => introducer = parent,
                None => {
                    Self::report_self_outside_item(span);
                    return self.tcx.error();
                }
            }
        };

        if let Some(&cached) = self.self_tys.get(&introducer) {
            return cached;
        }

        // `extend Foo<Self>` would ask for the very type being computed. Nothing sensible can be
        // built from that, so it is cut off rather than recursing forever.
        if !self.computing_self_tys.insert(introducer) {
            Self::report_self_cycle(span);
            return self.tcx.error();
        }

        let SelfTyRes::Ty { adt, .. } = res else {
            // `resolve_extend` records `SelfTyRes::Err` when the extended path did not resolve.
            self.computing_self_tys.remove(&introducer);
            return self.tcx.error();
        };

        let self_ty = match self.hir.def(introducer) {
            OwnerNode::Struct(struct_) => {
                let params = struct_.generics.clone();
                self.adt_of_own_params(introducer, &params)
            }
            OwnerNode::Enum(enum_) => {
                let params = enum_.generics.clone();
                self.adt_of_own_params(introducer, &params)
            }
            OwnerNode::Trait(_) => self.tcx.mk_self_param(adt),
            OwnerNode::Extend(extend) => {
                let args = extend.adt_generics.clone();
                let args = self.lower_tys(&args);
                self.tcx.mk_adt(adt, args)
            }
            _ => unreachable!("only a struct, enum, trait, or extend block introduces a `Self`"),
        };

        self.computing_self_tys.remove(&introducer);
        self.self_tys.insert(introducer, self_ty);
        self_ty
    }

    /// Builds `def_id` applied to the type parameters it declares itself, which is what `Self`
    /// means inside a `struct` or `enum` body.
    fn adt_of_own_params(&mut self, def_id: DefId, params: &[HirId]) -> Ty {
        let args = params.iter().map(|&id| self.tcx.mk_generic(id)).collect();
        self.tcx.mk_adt(def_id, args)
    }

    /// Reports generic arguments applied to something that declares none, such as `i32<bool>`.
    fn expect_no_args(args: &[HirId], span: SrcSpan, kind: &str) {
        if !args.is_empty() {
            DiagCtx::emit(
                Diagnostic::error(format!("{kind} takes no generic arguments"), span)
                    .with_label("unexpected generic arguments"),
            );
        }
    }

    fn report_arg_count(span: SrcSpan, declared: usize, found: usize) {
        let plural = if declared == 1 { "" } else { "s" };
        DiagCtx::emit(
            Diagnostic::error(
                format!(
                    "this type takes {declared} generic argument{plural} but {found} \
                     {} supplied",
                    if found == 1 { "was" } else { "were" }
                ),
                span,
            )
            .with_label(format!("expected {declared} argument{plural}")),
        );
    }

    fn report_trait_as_ty(span: SrcSpan) {
        DiagCtx::emit(
            Diagnostic::error("a trait cannot be used as a type on its own", span)
                .with_label("not a type")
                .with_help(
                    "a trait names every type that implements it, not one type; write \
                     `dyn Trait` for a value whose type is only known at run time, or take a \
                     generic parameter bounded by the trait",
                ),
        );
    }

    fn report_not_a_ty(span: SrcSpan) {
        DiagCtx::emit(
            Diagnostic::error("this path does not name a type", span).with_label("not a type"),
        );
    }

    fn report_dyn_not_a_trait(span: SrcSpan) {
        DiagCtx::emit(
            Diagnostic::error("`dyn` must be applied to a trait", span)
                .with_label("not a trait")
                .with_help("only a trait describes a set of types that a `dyn` value can hold"),
        );
    }

    fn report_self_outside_item(span: SrcSpan) {
        DiagCtx::emit(
            Diagnostic::error("`Self` is not available here", span)
                .with_label("no enclosing type")
                .with_help(
                    "`Self` names the type being defined, so it only means something inside a \
                     `struct`, `enum`, `trait`, or `extend` body",
                ),
        );
    }

    fn report_self_cycle(span: SrcSpan) {
        DiagCtx::emit(
            Diagnostic::error("`Self` is defined in terms of itself", span)
                .with_label("cycle here")
                .with_help(
                    "the type this `Self` stands for cannot be worked out without already \
                     knowing it",
                ),
        );
    }
}

#[cfg(test)]
mod tests {
    use crate::ast::Mutability;
    use crate::ast::interner::Interner;
    use crate::diag::DiagCtx;
    use crate::hir::{DefId, Hir, HirId, OwnerNode};
    use crate::nameres::results::PrimTy;
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
    /// alongside a two-line fixture would swamp what each test is actually about.
    fn check(src: &str) -> Checked {
        let (hir, nameres) = resolve_src(src);
        DiagCtx::clear();

        let checked = crate::typeck::check(&hir, &nameres);
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
            let root = self.hir.root();
            root.items
                .iter()
                .copied()
                .find(|&id| matches!(self.hir.def(id), OwnerNode::Extend(_)))
                .expect("no extend block")
        }

        /// The type recorded for a definition as a whole.
        fn def_ty(&self, def: DefId) -> Ty {
            self.types
                .ty_of_def(def)
                .expect("this definition's own type was never recorded")
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
                panic!("a function's own type is always a Fun type");
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
        let checked = check("fun f(x: i32, y: bool) -> i64 {}");
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
        let OwnerNode::Struct(struct_) = checked.hir.def(wrap) else {
            unreachable!();
        };
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
    fn a_trait_used_as_a_type_is_reported() {
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
            args.iter().map(|&arg| checked.kind(arg)).collect::<Vec<_>>(),
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
        let OwnerNode::Struct(struct_) = checked.hir.def(wrap) else {
            unreachable!();
        };
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
        let OwnerNode::Extend(block) = checked.hir.def(extend) else {
            unreachable!();
        };
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
}
