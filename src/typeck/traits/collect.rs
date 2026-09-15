//! Collecting the program's trait knowledge: reading every `extend` header and building the
//! index the rest of type checking looks traits and methods up in.

use std::collections::HashMap;

use crate::ast::{Mutability, SelfMode, Symbol};
use crate::diagnostics::typeck::traits::index::{
    report_attempt_to_extend_with_non_trait, report_extend_any, report_extend_bare_self,
    report_extend_dyn, report_extend_generic, report_extend_trait, report_extend_unsized,
};
use crate::hir::{DefId, Hir, HirId, OwnerNode, Res, TyDef, TyKind as HirTyKind, Type};
use crate::nameres::PrimTy;
use crate::typeck::Typeck;
use crate::typeck::traits::TraitRef;
use crate::typeck::ty::{Ty, TyKind};

/// The kind of type an `extend` block is keyed on, without its arguments: every `Wrap<i32>`
/// and `Wrap<bool>` share the head `Adt(Wrap)`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(crate) enum TypeHead {
    Adt(DefId),
    Prim(PrimTy),
    Tuple(usize),
    Array,
    Ref(Mutability),
    Fun(usize),
    Iso,
}

/// A stable order for iterating type heads, so checks that walk them report deterministically.
fn sort_key(head: &TypeHead) -> (u8, usize) {
    match *head {
        TypeHead::Adt(def) => (0, def.index()),
        TypeHead::Prim(prim) => (1, prim as usize),
        TypeHead::Tuple(arity) => (2, arity),
        TypeHead::Array => (3, 0),
        TypeHead::Ref(Mutability::Immutable) => (4, 0),
        TypeHead::Ref(Mutability::Mutable) => (4, 1),
        TypeHead::Fun(arity) => (5, arity),
        TypeHead::Iso => (6, 0),
    }
}

/// Every `extend` block, grouped by the type it extends.
#[derive(Default)]
pub struct ExtendIndex {
    /// Type -> all the blocks that extend it, in declaration order.
    by_type: HashMap<TypeHead, Vec<DefId>>,

    /// Extend block -> the trait it implements, when it implements one.
    by_block: HashMap<DefId, TraitRef>,
}

impl ExtendIndex {
    pub fn new() -> Self {
        ExtendIndex::default()
    }

    fn insert(&mut self, head: TypeHead, block: DefId, trait_: Option<TraitRef>) {
        self.by_type.entry(head).or_default().push(block);
        if let Some(trait_) = trait_ {
            self.by_block.insert(block, trait_);
        }
    }

    /// Returns an Option representing which trait `block` (an extend block) implements
    /// If it does not implement a trait, return `None`
    pub fn trait_of(&self, block: DefId) -> Option<&TraitRef> {
        self.by_block.get(&block)
    }

    /// Returns the DefIds of the extend blocks blocks extending `head`
    pub fn for_type(&self, head: TypeHead) -> &[DefId] {
        self.by_type.get(&head).map_or(&[], Vec::as_slice)
    }

    /// Every type head that has at least one block, in a stable order.
    pub fn extended_types(&self) -> Vec<TypeHead> {
        let mut heads: Vec<TypeHead> = self.by_type.keys().copied().collect();
        heads.sort_unstable_by_key(sort_key);
        heads
    }

    /// Every block, in the order its type head sorts.
    pub fn all(&self) -> Vec<DefId> {
        self.extended_types()
            .into_iter()
            .flat_map(|head| self.for_type(head).to_vec())
            .collect()
    }

    /// Every pair of blocks that extend the same type head. Only these pairs can overlap.
    pub fn pairs_per_type(&self) -> Vec<(DefId, DefId)> {
        let mut pairs = Vec::new();
        for head in self.extended_types() {
            let bucket = self.for_type(head);
            for (index, &first) in bucket.iter().enumerate() {
                pairs.extend(bucket[index + 1..].iter().map(|&second| (first, second)));
            }
        }
        pairs
    }

    #[cfg(test)]
    pub fn len(&self) -> usize {
        self.by_type.values().map(Vec::len).sum()
    }

    #[cfg(test)]
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

/// A single `extend` block's header: what it extends, what it implements, and the parameters
/// left open while matching it.
pub struct ExtendHeader {
    /// The block itself, which is what a diagnostic about it points at.
    pub def: DefId,

    /// The type the header matches, applied to whatever arguments it wrote.
    pub self_ty: Ty,

    /// The parameters the block declares, which are exactly the ones left open while matching.
    pub generics: Vec<HirId>,

    /// What the block implements, if it implements anything.
    pub trait_: Option<TraitRef>,
}

impl<'hir> Typeck<'hir> {
    // -----------------------------------------------------------------
    // The collect phase
    // -----------------------------------------------------------------

    /// Builds the trait knowledge the rest of type checking reads: for every `extend` block,
    /// which type it extends and which trait it implements.
    pub fn collect_traits(&mut self) {
        let blocks: Vec<DefId> = self.extend_blocks().collect();
        for block in blocks {
            self.index_extend_block(block);
        }
    }

    /// Records one `extend` block in the index, dropping it when the type it names cannot be
    /// extended.
    fn index_extend_block(&mut self, block: DefId) {
        let Some(head) = self.extend_head(block) else {
            return;
        };
        let trait_ = self.implemented_trait(block);
        self.extends.insert(head, block, trait_);
    }

    /// Every `extend` block in the program.
    fn extend_blocks(&self) -> impl Iterator<Item = DefId> + '_ {
        let hir = self.hir;
        hir.def_ids()
            .filter(|&def| matches!(hir.def(def), OwnerNode::Extend(_)))
    }

    // -----------------------------------------------------------------
    // Reading a header
    // -----------------------------------------------------------------

    /// Reads `block`'s header out of the places its parts live.
    pub(crate) fn extend_header(&self, block: DefId) -> ExtendHeader {
        ExtendHeader {
            def: block,
            self_ty: self.extended_type(block),
            generics: self.declared_generics(block).to_vec(),
            trait_: self.extends.trait_of(block).cloned(),
        }
    }

    /// The type `block` extends, with its arguments provided in the header.
    pub(crate) fn extended_type(&self, block: DefId) -> Ty {
        self.types
            .ty_of_def(block)
            .expect("collect_extend records every extend block's self type")
    }

    /// The type head `ty` is keyed on, or `None` for a type no `extend` block can name.
    pub(crate) fn type_head(&self, ty: Ty) -> Option<TypeHead> {
        match *self.tcx.kind(ty) {
            TyKind::Adt { def, .. } => Some(TypeHead::Adt(def)),
            TyKind::Primitive(prim) => Some(TypeHead::Prim(prim)),
            TyKind::Tuple(ref elems) => Some(TypeHead::Tuple(elems.len())),
            TyKind::Array { .. } => Some(TypeHead::Array),
            TyKind::Ref { mutability, .. } => Some(TypeHead::Ref(mutability)),
            TyKind::Fun { ref params, .. } => Some(TypeHead::Fun(params.len())),
            TyKind::Iso(_) => Some(TypeHead::Iso),
            _ => None,
        }
    }

    /// The generic parameters `def` declares.
    pub(crate) fn declared_generics(&self, def: DefId) -> &'hir [HirId] {
        let hir: &'hir Hir = self.hir;
        match hir.def(def) {
            OwnerNode::Function(f) => &f.generics,
            OwnerNode::Struct(s) => &s.generics,
            OwnerNode::Enum(e) => &e.generics,
            OwnerNode::Trait(t) => &t.generics,
            OwnerNode::Extend(e) => &e.extend_generics,
            OwnerNode::Module(_) | OwnerNode::Closure(_) => &[],
        }
    }

    // -----------------------------------------------------------------
    // Reading a trait and its members
    // -----------------------------------------------------------------

    /// The declaration of `name` inside `trait_def`, if it declares one.
    pub(crate) fn trait_method(&self, trait_def: DefId, name: Symbol) -> Option<DefId> {
        self.hir
            .trait_(trait_def)
            .functions
            .iter()
            .copied()
            .find(|&function| self.hir.function(function).name.text == name)
    }

    /// Maps a trait's declared generics to the arguments it was applied to.
    pub(crate) fn trait_subst(&self, trait_def: DefId, args: &[Ty]) -> HashMap<HirId, Ty> {
        self.hir
            .trait_(trait_def)
            .generics
            .iter()
            .copied()
            .zip(args.iter().copied())
            .collect()
    }

    /// The receiver mode `method` was declared with, or `None` for a free function.
    pub(crate) fn receiver_mode(&self, method: DefId) -> Option<SelfMode> {
        let function = self.hir.function(method);
        Some(self.hir.self_param(function.self_param?).mode)
    }

    /// The method named `name` that `block` provides, if any.
    pub(crate) fn get_method_in_block(&self, block: DefId, method_name: Symbol) -> Option<DefId> {
        self.hir
            .extend(block)
            .methods
            .iter()
            .copied()
            .find(|&method| self.hir.function(method).name.text == method_name)
    }

    // -----------------------------------------------------------------
    // Reading a header's parts
    // -----------------------------------------------------------------

    /// The index head `block`'s extended type maps to, reporting and rejecting the forms an
    /// `extend` block is not allowed to name.
    fn extend_head(&self, block: DefId) -> Option<TypeHead> {
        let node = self.hir.extend(block);
        match &self.hir.ty(node.self_ty).kind {
            HirTyKind::Path { path, .. } => match path.res {
                Res::Type(Type::Def(TyDef::Struct(def) | TyDef::Enum(def))) => {
                    Some(TypeHead::Adt(def))
                }
                Res::Type(Type::Prim(prim)) => Some(TypeHead::Prim(prim)),
                Res::Type(Type::Def(TyDef::Trait(_))) => {
                    report_extend_trait(self.session, node.span);
                    None
                }
                Res::Type(Type::Generic(_)) => {
                    report_extend_generic(self.session, node.span);
                    None
                }
                Res::Err => None,
                Res::SelfTy(_) | Res::Local(_) | Res::Function(_) => unreachable!(
                    "an extend block's own path cannot resolve to Self, a local, a function, or \
                     a module"
                ),
            },
            HirTyKind::Tuple(elems) => Some(TypeHead::Tuple(elems.len())),
            HirTyKind::Array { len: None, .. } => {
                report_extend_unsized(self.session, node.span);
                None
            }
            HirTyKind::Array { len: Some(_), .. } => Some(TypeHead::Array),
            HirTyKind::Ref { mutability, .. } => Some(TypeHead::Ref(*mutability)),
            HirTyKind::Function { params, .. } => Some(TypeHead::Fun(params.len())),
            HirTyKind::Iso(_) => Some(TypeHead::Iso),
            HirTyKind::Any(_) => {
                report_extend_any(self.session, node.span);
                None
            }
            HirTyKind::Dyn { .. } => {
                report_extend_dyn(self.session, node.span);
                None
            }
            HirTyKind::SelfTy(_) => {
                report_extend_bare_self(self.session, node.span);
                None
            }
            HirTyKind::Error => None,
        }
    }

    /// The trait `block` implements through its `with` clause, reporting a `with` that does not
    /// name a trait.
    fn implemented_trait(&self, block: DefId) -> Option<TraitRef> {
        let node = self.hir.extend(block);

        let Some(Res::Type(Type::Def(tydef))) = node.trait_path.as_ref().map(|path| path.res)
        else {
            return None;
        };

        let def = tydef.def_id();
        if !matches!(tydef, TyDef::Trait(_)) {
            report_attempt_to_extend_with_non_trait(self.session, node.span);
            return None;
        }

        let args = node
            .trait_generics
            .iter()
            .map(|&id| {
                self.types
                    .ty(id.into())
                    .expect("collect_extend lowers every trait argument an extend block writes")
            })
            .collect();

        Some(TraitRef { def, args })
    }
}

#[cfg(test)]
mod tests {
    use super::TypeHead;
    use crate::hir::Hir;
    use crate::testing::{TypeckStage, checker_through, lower_to_hir};
    use crate::typeck::Typeck;

    fn indexed<'hir>(hir: &'hir Hir) -> Typeck<'hir> {
        let mut checker = checker_through(hir, TypeckStage::Collect);
        crate::testing::clear_diagnostics();
        checker.collect_traits();
        checker
    }

    #[test]
    fn an_inherent_extend_is_indexed_against_the_type_it_extends() {
        let hir = lower_to_hir(
            "struct Foo {}
             extend Foo { fun get(&self) {} }",
        );
        let checker = indexed(&hir);

        assert_eq!(checker.extends.len(), 1);
        let block = checker.extends.for_type(foo(&checker))[0];
        assert!(
            checker.extends.trait_of(block).is_none(),
            "an inherent extend has no trait"
        );
        assert_eq!(hir.extend(block).methods.len(), 1);
        assert!(
            crate::testing::messages().is_empty(),
            "{:?}",
            crate::testing::messages()
        );
    }

    #[test]
    fn a_primitive_extend_is_indexed_against_the_primitive() {
        let hir = lower_to_hir("extend i32 { fun get(&self) -> i32 { return *self; } }");
        let checker = indexed(&hir);

        assert_eq!(checker.extends.len(), 1);
        let head = TypeHead::Prim(crate::nameres::PrimTy::I32);
        let block = checker.extends.for_type(head)[0];
        assert!(checker.extends.trait_of(block).is_none());
        assert!(
            crate::testing::messages().is_empty(),
            "{:?}",
            crate::testing::messages()
        );
    }

    #[test]
    fn a_trait_extend_records_the_trait_it_implements() {
        let hir = lower_to_hir(
            "trait Show { fun show(&self); }
             struct Foo {}
             extend Foo with Show { fun show(&self) {} }",
        );
        let checker = indexed(&hir);

        let block = checker.extends.for_type(foo(&checker))[0];
        let trait_ref = checker
            .extends
            .trait_of(block)
            .expect("`extend Foo with Show` implements a trait");
        assert!(trait_ref.args.is_empty());
        assert!(
            crate::testing::messages().is_empty(),
            "{:?}",
            crate::testing::messages()
        );
    }

    /// The block's own `<T>` group is what matching may bind; the struct's own `T` is a different
    /// parameter entirely and must not leak in.
    #[test]
    fn an_impls_generics_are_the_blocks_own_parameters() {
        let hir = lower_to_hir(
            "struct Wrap<T> { inner: T }
             extend<T> Wrap<T> { fun get(&self) {} }",
        );
        let checker = indexed(&hir);
        let block = checker.extends.for_type(wrap(&checker))[0];

        let generics = checker.declared_generics(block);
        assert_eq!(generics.len(), 1);
        assert_eq!(generics[0].owner, block);
    }

    #[test]
    fn a_tuple_extend_is_indexed_against_its_arity() {
        let hir = lower_to_hir("extend (i32, i32) { fun get(&self) {} }");
        let checker = indexed(&hir);

        assert_eq!(checker.extends.len(), 1);
        let head = TypeHead::Tuple(2);
        let block = checker.extends.for_type(head)[0];
        assert!(checker.extends.trait_of(block).is_none());
        assert!(
            crate::testing::messages().is_empty(),
            "{:?}",
            crate::testing::messages()
        );
    }

    #[test]
    fn extending_an_unsized_array_is_rejected() {
        let hir = lower_to_hir(
            "trait Show { fun show(&self); }
             extend [i32] with Show { fun show(&self) {} }",
        );
        let checker = indexed(&hir);

        assert!(checker.extends.is_empty());
        assert!(
            crate::testing::messages()
                .iter()
                .any(|m| m.contains("unsized array cannot be extended")),
            "{:?}",
            crate::testing::messages()
        );
    }

    /// The reachable non-nominal case: a path that names a type parameter rather than a type.
    #[test]
    fn extending_a_type_parameter_is_reported_and_dropped() {
        let hir = lower_to_hir(
            "trait Show { fun show(&self); }
             extend<T> T with Show { fun show(&self) {} }",
        );
        let checker = indexed(&hir);

        assert_eq!(
            crate::testing::messages(),
            ["a generic type parameter cannot be extended"]
        );
        assert!(
            checker.extends.is_empty(),
            "a rejected extend must not reach the index"
        );
    }

    #[test]
    fn extending_any_is_reported_and_dropped() {
        let hir = lower_to_hir(
            "trait Show { fun show(&self); }
             extend any i32 with Show { fun show(&self) {} }",
        );
        let checker = indexed(&hir);

        assert_eq!(crate::testing::messages(), ["`any` cannot be extended"]);
        assert!(
            checker.extends.is_empty(),
            "a rejected extend must not reach the index"
        );
    }

    #[test]
    fn extending_a_dyn_trait_is_reported_and_dropped() {
        let hir = lower_to_hir(
            "trait Show { fun show(&self); }
             extend dyn Show with Show { fun show(&self) {} }",
        );
        let checker = indexed(&hir);

        assert_eq!(
            crate::testing::messages(),
            ["`dyn Trait` cannot be extended"]
        );
        assert!(
            checker.extends.is_empty(),
            "a rejected extend must not reach the index"
        );
    }

    #[test]
    fn extending_a_trait_is_reported_and_dropped() {
        let hir = lower_to_hir(
            "trait Show { fun show(&self); }
             extend Show { fun show(&self) {} }",
        );
        let checker = indexed(&hir);

        assert_eq!(crate::testing::messages(), ["a trait cannot be extended"]);
        assert!(checker.extends.is_empty());
    }

    #[test]
    fn extending_an_unresolved_path_reports_nothing_further() {
        let hir = lower_to_hir("extend Nope { fun get(&self) {} }");
        let checker = indexed(&hir);

        assert!(
            crate::testing::messages().is_empty(),
            "name resolution already reported the missing name: {:?}",
            crate::testing::messages()
        );
        assert!(checker.extends.is_empty());
    }

    #[test]
    fn implementing_something_that_is_not_a_trait_is_reported() {
        let hir = lower_to_hir(
            "struct Foo {}
             struct Bar {}
             extend Foo with Bar {}",
        );
        let checker = indexed(&hir);

        assert_eq!(crate::testing::messages(), ["`with` must name a trait"]);
        // The block itself is still perfectly valid as an inherent block, so it stays in the index.
        assert_eq!(checker.extends.len(), 1);
    }

    /// A type with no `extend` block at all answers the same way as one with an empty bucket,
    /// which is what keeps the query from needing an "unimplemented" case of its own.
    #[test]
    fn a_type_with_no_impls_has_an_empty_bucket() {
        let hir = lower_to_hir("struct Foo {}");
        let checker = indexed(&hir);

        assert!(checker.extends.for_type(foo(&checker)).is_empty());
        assert!(checker.extends.extended_types().is_empty());
    }

    fn foo(checker: &Typeck<'_>) -> TypeHead {
        TypeHead::Adt(crate::testing::named_def(checker.hir, "Foo"))
    }

    fn wrap(checker: &Typeck<'_>) -> TypeHead {
        TypeHead::Adt(crate::testing::named_def(checker.hir, "Wrap"))
    }
}
