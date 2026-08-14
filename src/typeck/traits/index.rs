//! Every `extend` block in the program, collected into one table the solver can look up in.
//!
//! An [`ImplHeader`] is an `extend` block with everything the solver needs and nothing it does
//! not: the type being extended, the trait being implemented, the block's own type parameters,
//! and the methods it defines. The bodies stay behind in the HIR.
//!
//! The header is an *open* term. `extend<T> Box<T> with Show` records a `self_ty` of `Box<T>`
//! whose `T` is a [`TyKind::Generic`] naming the block's own parameter, so the header describes a
//! family of types rather than one type. Closing it is what
//! [`match_ty`](crate::typeck::traits::solve::match_ty) does at query time; nothing is
//! instantiated here.
//!
//! [`ImplIndex::by_self`] keys on the *head* of the self type -- the struct or enum's [`DefId`],
//! with its arguments dropped. That key is the grid the whole design rests on: rows are ADTs, and
//! the "fully generic / partly concrete / absent" distinction nobody has to classify is simply
//! what an argument list looks like when matching tries it. An empty bucket is the absent case,
//! and everything else falls out of one code path.

use std::collections::HashMap;

use crate::ast::Symbol;
use crate::diagnostics::typeck::traits::index::{
    report_extend_generic, report_extend_primitive, report_extend_trait, report_impl_of_non_trait,
};
use crate::driver::source::SrcSpan;
use crate::hir::{DefId, HirId, OwnerNode, Res, TyDef, Type};
use crate::typeck::Typeck;
use crate::typeck::traits::TraitRef;
use crate::typeck::ty::Ty;

/// An index into [`ImplIndex::impls`], following the [`DefId`]/[`LocalId`] newtype pattern the
/// rest of the compiler addresses things by.
///
/// An `extend` block already has a `DefId`, so this is not the only way to name one. It exists
/// because `select` (in [`solve`](crate::typeck::traits::solve)) picks one impl out of a bucket
/// and then has to read its header back -- to substitute into its own obligations, and to reach
/// the `extend` block's `def` for a [`ParamEnv`](crate::typeck::traits::solve::ParamEnv) -- and
/// resolving a `DefId` back to a header would mean a second map for a lookup that is already an
/// array index.
///
/// [`LocalId`]: crate::hir::DefId
#[derive(Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Debug)]
pub struct ImplId(u32);

impl ImplId {
    fn from_usize(index: usize) -> Self {
        ImplId(index as u32)
    }

    pub fn index(self) -> usize {
        self.0 as usize
    }
}

/// One `extend` block, reduced to what answering a trait query takes.
///
/// Deliberately absent: any list of the obligations this impl imposes. Phi has no `where`
/// syntax, so an impl's obligations are exactly the bounds written on its `extend<T: Show>`
/// generics -- which is precisely
/// [`ParamEnv::of(self.def)`](crate::typeck::traits::solve::ParamEnv). Storing them a second time
/// here would be a copy to keep in sync with nothing gained.
#[derive(Debug)]
pub struct ImplHeader {
    /// The `extend` block itself.
    pub def: DefId,

    /// The parameters the block declares in its own `<..>` group -- *this impl's variables*, the
    /// ones matching is allowed to bind. A [`TyKind::Generic`] inside [`ImplHeader::self_ty`]
    /// that is not in here belongs to some enclosing definition and is a rigid constant as far as
    /// matching is concerned.
    pub generics: Vec<HirId>,

    /// The type being extended, open over [`ImplHeader::generics`]: `Box<T>`, `Map<i32, V>`,
    /// `Foo`. Always a [`TyKind::Adt`] whose head is a struct or an enum -- see
    /// [`Typeck::build_impl_index`].
    pub self_ty: Ty,

    /// The trait being implemented, or `None` for an inherent `extend Foo { .. }`.
    pub trait_ref: Option<TraitRef>,

    /// The methods this block defines, by name. Note this is the block's *own* list, which for a
    /// trait impl is not the same as the set of methods the type ends up with -- a trait's
    /// defaulted methods are available without appearing here. Coherence is careful about the
    /// difference; see [`coherence`](crate::typeck::traits::coherence).
    pub methods: HashMap<Symbol, DefId>,

    /// The whole block, which is what a conflict between two impls is reported against.
    pub span: SrcSpan,
}

/// Every `extend` block in the program, addressable by [`ImplId`] and bucketed by the type it
/// extends.
#[derive(Default)]
pub struct ImplIndex {
    impls: Vec<ImplHeader>,

    /// Which impls extend which type, keyed on the head of the self type. See the
    /// [module docs](self) on why the head alone is the right key.
    by_self: HashMap<DefId, Vec<ImplId>>,
}

impl ImplIndex {
    pub fn new() -> Self {
        ImplIndex::default()
    }

    /// Adds `header` to the index, returning the handle it was given.
    fn push(&mut self, head: DefId, header: ImplHeader) -> ImplId {
        let id = ImplId::from_usize(self.impls.len());
        self.impls.push(header);
        self.by_self.entry(head).or_default().push(id);
        id
    }

    pub fn header(&self, id: ImplId) -> &ImplHeader {
        &self.impls[id.index()]
    }

    /// Every impl whose self type is headed by `def`, in the order they were collected. An
    /// unimplemented type answers with an empty slice rather than nothing, since "no impls" is
    /// not a different question from "no bucket".
    pub fn for_self(&self, def: DefId) -> &[ImplId] {
        self.by_self.get(&def).map_or(&[], Vec::as_slice)
    }

    /// Every type that has at least one impl, in `DefId` order.
    ///
    /// Sorted rather than handed out in hash order, because coherence walks these buckets to
    /// report conflicts and diagnostics that come out in a different order on every run are
    /// untestable.
    pub fn extended_types(&self) -> Vec<DefId> {
        let mut heads: Vec<DefId> = self.by_self.keys().copied().collect();
        heads.sort_unstable();
        heads
    }

    pub fn len(&self) -> usize {
        self.impls.len()
    }

    pub fn is_empty(&self) -> bool {
        self.impls.is_empty()
    }
}

impl<'hir> Typeck<'hir> {
    /// Collects every `extend` block in the program into [`Typeck::impls`].
    ///
    /// Runs after collection, because a header is only a header once its types are lowered, and
    /// before any body is checked, because checking a body may ask the solver a question. See the
    /// pipeline order in [`check`](crate::typeck::check).
    ///
    /// Walking [`Hir::def_ids`](crate::hir::Hir::def_ids) rather than recursing through the
    /// module tree is deliberate: every `extend` block anywhere in the program has an arena, and
    /// arenas are numbered in lowering order, so this visits each block exactly once in an order
    /// that does not depend on a hash map.
    pub fn build_impl_index(&mut self) {
        let hir = self.hir;
        let extends: Vec<DefId> = hir
            .def_ids()
            .filter(|&def| matches!(hir.def(def), OwnerNode::Extend(_)))
            .collect();

        for extend in extends {
            if let Some((head, header)) = self.impl_header(extend) {
                self.impls.push(head, header);
            }
        }
    }

    /// Builds one `extend` block's header, or reports why it cannot be implemented and returns
    /// `None` -- which drops the block from the index, so every later pass sees a world where
    /// every impl has a nominal self type.
    fn impl_header(&mut self, extend: DefId) -> Option<(DefId, ImplHeader)> {
        let hir = self.hir;
        let OwnerNode::Extend(block) = hir.def(extend) else {
            unreachable!("root of an Extend owner is always OwnerNode::Extend");
        };
        let (generics, trait_generics, methods, span) = (
            block.extend_generics.clone(),
            block.trait_generics.clone(),
            block.methods.clone(),
            block.span,
        );

        let head = self.impl_self_head(extend, span)?;
        let self_ty = self
            .types
            .ty_of_def(extend)
            .expect("collect_extend records every extend block's self type");

        let trait_ref = self.impl_trait_ref(extend, &trait_generics, span);

        let methods = methods
            .into_iter()
            .map(|method| {
                let OwnerNode::Function(function) = hir.def(method) else {
                    unreachable!("an extend block's methods are always functions");
                };
                (function.name.text, method)
            })
            .collect();

        Some((
            head,
            ImplHeader {
                def: extend,
                generics,
                self_ty,
                trait_ref,
                methods,
                span,
            },
        ))
    }

    /// The struct or enum an `extend` block extends, or `None` after reporting that what it
    /// names cannot be implemented.
    ///
    /// What may be extended is exactly what may implement a trait: a struct or an enum. That
    /// keeps the index keyed on a single `DefId` and keeps `Self` nominal. A reference, a tuple,
    /// an array or a function type has no name to key on; a primitive has one but no definition
    /// behind it; and a trait names a *set* of types rather than a type, so it is not something
    /// that implements traits.
    ///
    /// Most of those are not reachable today, because the extended type is parsed as a path of
    /// identifiers: there is no syntax for `extend &T`, `extend (A, B)` or `extend dyn Show` to
    /// reject, and a primitive is a keyword rather than an identifier, so `extend i32` is a parse
    /// error before it ever gets here. The arms below are still written out, because what a path
    /// may resolve to is [`Res`]'s business rather than this function's, and a widened path
    /// grammar should meet a diagnostic here rather than an `unreachable!`.
    fn impl_self_head(&mut self, extend: DefId, span: SrcSpan) -> Option<DefId> {
        let OwnerNode::Extend(block) = self.hir.def(extend) else {
            unreachable!("root of an Extend owner is always OwnerNode::Extend");
        };

        // What the block's own `adt_path` named, kept undiscarded by name resolution precisely
        // so that the wording below can tell these cases apart; see `resolver::visit_extend`.
        match block.adt_path.res {
            Res::Type(Type::Def(TyDef::Struct(def) | TyDef::Enum(def))) => Some(def),
            Res::Type(Type::Def(TyDef::Trait(_))) => {
                report_extend_trait(span);
                None
            }
            Res::Type(Type::Prim(_)) => {
                report_extend_primitive(span);
                None
            }
            Res::Type(Type::Generic(_)) => {
                report_extend_generic(span);
                None
            }
            // Already reported by name resolution; staying quiet here keeps one mistake from
            // producing a second diagnostic.
            Res::Err => None,
            // `extend`'s own path is parsed like any other, never as the `Self` keyword or a
            // value/module name -- see `LoweringCtx::as_self_ty` and `SymbolTable::lookup_type_path`.
            Res::SelfTy(_) | Res::Local(_) | Res::Function(_) | Res::Module(_) => unreachable!(
                "an extend block's own path cannot resolve to Self, a local, a function, or a \
                 module"
            ),
        }
    }

    /// The trait an `extend` block implements, applied to the arguments the block wrote for it,
    /// or `None` for an inherent block.
    fn impl_trait_ref(
        &mut self,
        extend: DefId,
        trait_generics: &[HirId],
        span: SrcSpan,
    ) -> Option<TraitRef> {
        let OwnerNode::Extend(block) = self.hir.def(extend) else {
            unreachable!("root of an Extend owner is always OwnerNode::Extend");
        };

        let Some(Res::Type(Type::Def(tydef))) = block.trait_path.as_ref().map(|path| path.res)
        else {
            // Either an inherent `extend Foo { .. }` (no `trait_path` at all), or a `with`
            // clause whose path resolved to something other than a struct/enum/trait -- a
            // primitive, a generic parameter, or nothing at all because it failed to resolve,
            // which name resolution has already reported. All three leave the block with
            // inherent methods and no trait, which is a consistent thing for later passes to
            // see.
            return None;
        };

        let def = tydef.def_id();
        if !matches!(tydef, TyDef::Trait(_)) {
            // `extend Foo with Bar` where `Bar` is a struct. Reported here rather than left to
            // fail confusingly later: a `TraitRef` naming a non-trait would break every consumer's
            // assumption that `def` has a method list.
            report_impl_of_non_trait(span);
            return None;
        }

        let args = trait_generics
            .iter()
            .map(|&id| {
                self.types
                    .ty(id)
                    .expect("collect_extend lowers every trait argument an extend block writes")
            })
            .collect();

        Some(TraitRef { def, args })
    }
}

#[cfg(test)]
mod tests {
    use crate::diag::DiagCtx;
    use crate::hir::Hir;
    use crate::testing::{lex_src, resolve_src};
    use crate::typeck::Typeck;

    /// Runs collection and index construction over `src`, and hands back the checker so a test
    /// can look at the index it built.
    ///
    /// Body checking is deliberately not run: most expression kinds are still `todo!()`, so a
    /// fixture would have to be written around the checker rather than around what is being
    /// tested. Diagnostics from name resolution are cleared first, since a fixture is resolved
    /// without the core library and so reports every lang item as missing.
    fn indexed<'hir>(hir: &'hir Hir) -> Typeck<'hir> {
        let mut checker = Typeck::new(hir);
        checker.collect_module(hir.root_id());
        DiagCtx::clear();
        checker.build_impl_index();
        checker
    }

    fn messages() -> Vec<String> {
        DiagCtx::diagnostics()
            .into_iter()
            .map(|diagnostic| diagnostic.message)
            .collect()
    }

    #[test]
    fn an_inherent_extend_is_indexed_against_the_type_it_extends() {
        let hir = resolve_src(
            "struct Foo {}
             extend Foo { fun get(&self) {} }",
        );
        let checker = indexed(&hir);

        assert_eq!(checker.impls.len(), 1);
        let header = checker
            .impls
            .header(checker.impls.for_self(foo(&checker))[0]);
        assert!(
            header.trait_ref.is_none(),
            "an inherent extend has no trait"
        );
        assert_eq!(header.methods.len(), 1);
        assert!(messages().is_empty(), "{:?}", messages());
    }

    #[test]
    fn a_trait_extend_records_the_trait_it_implements() {
        let hir = resolve_src(
            "trait Show { fun show(&self); }
             struct Foo {}
             extend Foo with Show { fun show(&self) {} }",
        );
        let checker = indexed(&hir);

        let header = checker
            .impls
            .header(checker.impls.for_self(foo(&checker))[0]);
        let trait_ref = header
            .trait_ref
            .as_ref()
            .expect("`extend Foo with Show` implements a trait");
        assert!(trait_ref.args.is_empty());
        assert!(messages().is_empty(), "{:?}", messages());
    }

    /// The block's own `<T>` group is what matching may bind; the struct's own `T` is a different
    /// parameter entirely and must not leak in.
    #[test]
    fn an_impls_generics_are_the_blocks_own_parameters() {
        let hir = resolve_src(
            "struct Wrap<T> { inner: T }
             extend<T> Wrap<T> { fun get(&self) {} }",
        );
        let checker = indexed(&hir);
        let header = checker
            .impls
            .header(checker.impls.for_self(wrap(&checker))[0]);

        assert_eq!(header.generics.len(), 1);
        assert_eq!(header.generics[0].owner, header.def);
    }

    /// `extend i32 with Add` is rejected, but not here: a primitive is a keyword token and the
    /// extended type is parsed as a path of identifiers, so the parser never builds the block at
    /// all. [`Typeck::impl_self_head`]'s primitive arm is what catches it the day a path may name
    /// one -- which is why the arm exists with no reachable path to it.
    #[test]
    fn extending_a_primitive_is_rejected_before_type_checking() {
        let (tokens, offset) = lex_src(
            "trait Show { fun show(&self); }
             extend i32 with Show { fun show(&self) {} }",
        );
        crate::parser::Parser::new().parse(&tokens, offset);

        assert!(
            !DiagCtx::diagnostics().is_empty(),
            "a primitive in `extend` position is a parse error"
        );
        DiagCtx::clear();
    }

    /// The reachable non-nominal case: a path that names a type parameter rather than a type.
    #[test]
    fn extending_a_type_parameter_is_reported_and_dropped() {
        let hir = resolve_src(
            "trait Show { fun show(&self); }
             extend<T> T with Show { fun show(&self) {} }",
        );
        let checker = indexed(&hir);

        assert_eq!(messages(), ["a generic type parameter cannot be extended"]);
        assert!(
            checker.impls.is_empty(),
            "a rejected extend must not reach the index"
        );
    }

    #[test]
    fn extending_a_trait_is_reported_and_dropped() {
        let hir = resolve_src(
            "trait Show { fun show(&self); }
             extend Show { fun show(&self) {} }",
        );
        let checker = indexed(&hir);

        assert_eq!(messages(), ["a trait cannot be extended"]);
        assert!(checker.impls.is_empty());
    }

    #[test]
    fn extending_an_unresolved_path_reports_nothing_further() {
        let hir = resolve_src("extend Nope { fun get(&self) {} }");
        let checker = indexed(&hir);

        assert!(
            messages().is_empty(),
            "name resolution already reported the missing name: {:?}",
            messages()
        );
        assert!(checker.impls.is_empty());
    }

    #[test]
    fn implementing_something_that_is_not_a_trait_is_reported() {
        let hir = resolve_src(
            "struct Foo {}
             struct Bar {}
             extend Foo with Bar {}",
        );
        let checker = indexed(&hir);

        assert_eq!(messages(), ["`with` must name a trait"]);
        // The block itself is still a perfectly good inherent impl, so it stays in the index.
        assert_eq!(checker.impls.len(), 1);
    }

    /// A type with no `extend` block at all answers the same way as one with an empty bucket,
    /// which is what keeps the query from needing an "unimplemented" case of its own.
    #[test]
    fn a_type_with_no_impls_has_an_empty_bucket() {
        let hir = resolve_src("struct Foo {}");
        let checker = indexed(&hir);

        assert!(checker.impls.for_self(foo(&checker)).is_empty());
        assert!(checker.impls.extended_types().is_empty());
    }

    fn foo(checker: &Typeck<'_>) -> crate::hir::DefId {
        named(checker, "Foo")
    }

    fn wrap(checker: &Typeck<'_>) -> crate::hir::DefId {
        named(checker, "Wrap")
    }

    /// The `DefId` of the top-level struct named `name`.
    fn named(checker: &Typeck<'_>, name: &str) -> crate::hir::DefId {
        use crate::ast::interner::Interner;
        use crate::hir::OwnerNode;

        checker
            .hir
            .root()
            .items
            .iter()
            .copied()
            .find(|&id| match checker.hir.def(id) {
                OwnerNode::Struct(s) => Interner::resolve(s.name.text) == name,
                _ => false,
            })
            .unwrap_or_else(|| panic!("no struct named {name:?}"))
    }
}
