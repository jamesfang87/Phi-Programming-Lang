use std::collections::HashMap;

use crate::ast::Symbol;
use crate::diagnostics::typeck::traits::index::{
    report_attempt_to_extend_with_non_trait, report_extend_generic, report_extend_primitive,
    report_extend_trait,
};
use crate::hir::{DefId, HirId, OwnerNode, Res, TyDef, Type};
use crate::typeck::Typeck;
use crate::typeck::traits::TraitRef;
use crate::typeck::ty::Ty;

#[derive(Default)]
pub struct ExtendIndex {
    /// Type -> All extend blocks for it.
    by_adt: HashMap<DefId, Vec<DefId>>,

    /// Extend block -> the trait that it implements.
    by_extend: HashMap<DefId, TraitRef>,
}

impl ExtendIndex {
    pub fn new() -> Self {
        ExtendIndex::default()
    }

    fn push(&mut self, head: DefId, block: DefId, trait_: Option<TraitRef>) {
        self.by_adt.entry(head).or_default().push(block);
        if let Some(trait_) = trait_ {
            self.by_extend.insert(block, trait_);
        }
    }

    /// What `block` implements, or `None` where it implements nothing.
    pub fn trait_of(&self, block: DefId) -> Option<&TraitRef> {
        self.by_extend.get(&block)
    }

    /// The blocks extending `head`, in declaration order.
    pub fn for_type(&self, head: DefId) -> &[DefId] {
        self.by_adt.get(&head).map_or(&[], Vec::as_slice)
    }

    #[cfg(test)]
    pub fn len(&self) -> usize {
        self.by_adt.values().map(Vec::len).sum()
    }

    #[cfg(test)]
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    pub fn extended_types(&self) -> Vec<DefId> {
        let mut heads: Vec<DefId> = self.by_adt.keys().copied().collect();
        heads.sort_unstable();
        heads
    }

    pub fn all(&self) -> Vec<DefId> {
        self.extended_types()
            .into_iter()
            .flat_map(|head| self.for_type(head).to_vec())
            .collect()
    }

    /// Groups all extend blocks for one type pairwise.
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
}

impl<'hir> Typeck<'hir> {
    pub fn build_extend_index(&mut self) {
        let hir = self.hir;
        let extends: Vec<DefId> = hir
            .def_ids()
            .filter(|&def| matches!(hir.def(def), OwnerNode::Extend(_)))
            .collect();

        for block in extends {
            let Some(head) = self.adt_of(block) else {
                continue;
            };
            let trait_ = self.trait_of(block, &hir.extend(block).trait_generics);
            self.extends.push(head, block, trait_);
        }
    }

    /// The type `block` extends with its arguments provided in the header.
    pub(crate) fn adt_of_with_args(&self, block: DefId) -> Ty {
        self.types
            .ty_of_def(block)
            .expect("collect_extend records every extend block's self type")
    }

    pub(crate) fn get_method_in_block(&self, block: DefId, method_name: Symbol) -> Option<DefId> {
        self.hir
            .extend(block)
            .methods
            .iter()
            .copied()
            .find(|&method| self.hir.function(method).name.text == method_name)
    }

    fn adt_of(&self, block: DefId) -> Option<DefId> {
        let node = self.hir.extend(block);
        match node.adt_path.res {
            Res::Type(Type::Def(TyDef::Struct(def) | TyDef::Enum(def))) => Some(def),
            Res::Type(Type::Def(TyDef::Trait(_))) => {
                report_extend_trait(node.span);
                None
            }
            Res::Type(Type::Prim(_)) => {
                report_extend_primitive(node.span);
                None
            }
            Res::Type(Type::Generic(_)) => {
                report_extend_generic(node.span);
                None
            }
            Res::Err => None,
            Res::SelfTy(_) | Res::Local(_) | Res::Function(_) | Res::Module(_) => unreachable!(
                "an extend block's own path cannot resolve to Self, a local, a function, or a \
                 module"
            ),
        }
    }

    fn trait_of(&self, block: DefId, trait_generics: &[HirId]) -> Option<TraitRef> {
        let node = self.hir.extend(block);

        let Some(Res::Type(Type::Def(tydef))) = node.trait_path.as_ref().map(|path| path.res)
        else {
            return None;
        };

        let def = tydef.def_id();
        if !matches!(tydef, TyDef::Trait(_)) {
            report_attempt_to_extend_with_non_trait(node.span);
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
    use crate::diagnostics::DiagCtx;
    use crate::hir::Hir;
    use crate::testing::{Stage, checker_through, lex_src, messages, resolve_src};
    use crate::typeck::Typeck;

    /// Runs collection and index construction over `src`, and hands back the checker so a test
    /// can look at the index it built.
    ///
    /// Body checking is deliberately not run: most expression kinds are still `todo!()`, so a
    /// fixture would have to be written around the checker rather than around what is being
    /// tested. Diagnostics from name resolution are cleared first, since a fixture is resolved
    /// without the core library and so reports every lang item as missing.
    fn indexed<'hir>(hir: &'hir Hir) -> Typeck<'hir> {
        let mut checker = checker_through(hir, Stage::Collect);
        DiagCtx::clear();
        checker.build_extend_index();
        checker
    }

    #[test]
    fn an_inherent_extend_is_indexed_against_the_type_it_extends() {
        let hir = resolve_src(
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

        let block = checker.extends.for_type(foo(&checker))[0];
        let trait_ref = checker
            .extends
            .trait_of(block)
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
        let block = checker.extends.for_type(wrap(&checker))[0];

        let generics = checker.declared_generics(block);
        assert_eq!(generics.len(), 1);
        assert_eq!(generics[0].owner, block);
    }

    /// `extend i32 with Add` is rejected, but not here: a primitive is a keyword token and the
    /// extended type is parsed as a path of identifiers, so the parser never builds the block at
    /// all. [`Typeck::adt_of`]'s primitive arm is what would catch it the day a path may name
    /// one, which is why the arm exists with no reachable path to it today.
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
            checker.extends.is_empty(),
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
        assert!(checker.extends.is_empty());
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
        assert!(checker.extends.is_empty());
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
        // The block itself is still perfectly valid as an inherent block, so it stays in the index.
        assert_eq!(checker.extends.len(), 1);
    }

    /// A type with no `extend` block at all answers the same way as one with an empty bucket,
    /// which is what keeps the query from needing an "unimplemented" case of its own.
    #[test]
    fn a_type_with_no_impls_has_an_empty_bucket() {
        let hir = resolve_src("struct Foo {}");
        let checker = indexed(&hir);

        assert!(checker.extends.for_type(foo(&checker)).is_empty());
        assert!(checker.extends.extended_types().is_empty());
    }

    fn foo(checker: &Typeck<'_>) -> crate::hir::DefId {
        crate::testing::named_def(checker.hir, "Foo")
    }

    fn wrap(checker: &Typeck<'_>) -> crate::hir::DefId {
        crate::testing::named_def(checker.hir, "Wrap")
    }
}
