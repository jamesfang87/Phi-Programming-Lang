//! Coherence checks for
//! - Duplicate implementation
//! - Duplicate method names

use crate::ast::Symbol;
use crate::ast::interner::Interner;
use crate::diagnostics::typeck::traits::coherence::{
    report_conflicting_extends, report_duplicate_method,
};
use crate::hir::DefId;
use crate::typeck::Typeck;
use crate::typeck::traits::overlap::overlaps;

impl<'hir> Typeck<'hir> {
    pub fn check_coherence(&mut self) {
        for (first, second) in self.extends.pairs_per_type() {
            self.check_pair(first, second);
        }
    }

    fn check_pair(&mut self, first: DefId, second: DefId) {
        let (a, b) = (self.extend_header(first), self.extend_header(second));
        if !overlaps(&mut self.tcx, &a, &b) {
            return;
        }

        // Check 1: no two extends of the same trait may overlap.
        if let (Some(trait_a), Some(trait_b)) = (&a.trait_, &b.trait_)
            && trait_a.def == trait_b.def
        {
            report_conflicting_extends(self.hir, self.display_cx(), &a, &b);
        }

        // Check 2: no two overlapping blocks may offer the same method name
        for name in self.shared_method_names(first, second) {
            report_duplicate_method(self.hir, self.display_cx(), name, &a, &b);
        }
    }

    /// The method names both blocks make available in sorted order.
    fn shared_method_names(&self, a: DefId, b: DefId) -> Vec<Symbol> {
        let (mut names, other) = (self.declared_methods(a), self.declared_methods(b));
        names.retain(|name| other.contains(name));
        names.sort_by_key(|&name| Interner::resolve(name));
        names
    }

    /// The declared methods which are extended to a type by an extend block.
    /// For extend-with blocks, this also includes any defaulted methods.
    fn declared_methods(&self, extend: DefId) -> Vec<Symbol> {
        let names = |methods: &[DefId]| {
            methods
                .iter()
                .map(|&function| self.hir.function(function).name.text)
                .collect()
        };

        match self.extends.trait_of(extend) {
            Some(trait_ref) => names(&self.hir.trait_(trait_ref.def).functions),
            None => names(&self.hir.extend(extend).methods),
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::diagnostics::DiagCtx;
    use crate::hir::Hir;
    use crate::testing::{Stage, checker_through, messages, resolve_src};

    /// Runs everything up to and including coherence over `src`, and hands back what it reported.
    ///
    /// Diagnostics from name resolution are cleared first: a fixture is resolved without the core
    /// library, so every one of them reports the whole set of missing lang items.
    fn coherence(hir: &Hir) -> Vec<String> {
        let mut checker = checker_through(hir, Stage::Index);
        DiagCtx::clear();
        checker.check_coherence();
        messages()
    }

    // -----------------------------------------------------------------
    // Check 1: duplicate implementation
    // -----------------------------------------------------------------

    #[test]
    fn implementing_one_trait_twice_for_one_type_is_reported() {
        let hir = resolve_src(
            "trait Show { fun show(&self); }
             struct Foo {}
             extend Foo with Show { fun show(&self) {} }
             extend Foo with Show { fun show(&self) {} }",
        );

        assert_eq!(
            coherence(&hir),
            [
                "conflicting implementations of trait `Show` for type `Foo`",
                // The same pair collides on `show` as well, which is check 2 speaking.
                "the method `show` is defined more than once for type `Foo`",
            ]
        );
    }

    /// The conflict is about two blocks, so the diagnostic names two places: the later block,
    /// which is what has to change, and the earlier one it collides with.
    #[test]
    fn a_conflict_points_at_both_blocks() {
        let hir = resolve_src(
            "trait Marker {}
             struct Foo {}
             extend Foo with Marker {}
             extend Foo with Marker {}",
        );

        let mut checker = checker_through(&hir, Stage::Index);
        DiagCtx::clear();
        checker.check_coherence();

        let diagnostics = DiagCtx::diagnostics();
        let [conflict] = diagnostics.as_slice() else {
            panic!("expected exactly one diagnostic, got {diagnostics:?}");
        };
        let [first] = conflict.secondary.as_slice() else {
            panic!("expected exactly one secondary label");
        };
        assert_eq!(first.message, "`Foo` is already implemented here");

        // The secondary points at the earlier block and the primary at the later one, which is
        // the whole distinction the two labels are drawing.
        let primary = conflict.span.expect("a conflict names a place");
        assert!(first.span.get_begin() < primary.get_begin());
    }

    /// A marker trait with no methods is exactly the case check 2 cannot see, which is why the
    /// two checks are separate.
    #[test]
    fn implementing_a_method_less_trait_twice_is_still_reported() {
        let hir = resolve_src(
            "trait Marker {}
             struct Foo {}
             extend Foo with Marker {}
             extend Foo with Marker {}",
        );

        assert_eq!(
            coherence(&hir),
            ["conflicting implementations of trait `Marker` for type `Foo`"]
        );
    }

    /// A fully generic block overlaps a concrete one, so the two conflict even though neither is
    /// literally a duplicate of the other.
    #[test]
    fn a_generic_impl_conflicts_with_a_concrete_one() {
        let hir = resolve_src(
            "trait Marker {}
             struct Wrap<T> { inner: T }
             extend<T> Wrap<T> with Marker {}
             extend Wrap<i32> with Marker {}",
        );

        assert_eq!(
            coherence(&hir),
            ["conflicting implementations of trait `Marker` for type `Wrap<i32>`"]
        );
    }

    #[test]
    fn impls_for_disjoint_arguments_do_not_conflict() {
        let hir = resolve_src(
            "trait Marker {}
             struct Wrap<T> { inner: T }
             extend Wrap<i32> with Marker {}
             extend Wrap<bool> with Marker {}",
        );

        assert!(coherence(&hir).is_empty());
    }

    #[test]
    fn impls_of_different_traits_for_one_type_do_not_conflict() {
        let hir = resolve_src(
            "trait A {}
             trait B {}
             struct Foo {}
             extend Foo with A {}
             extend Foo with B {}",
        );

        assert!(coherence(&hir).is_empty());
    }

    #[test]
    fn one_trait_implemented_for_two_types_does_not_conflict() {
        let hir = resolve_src(
            "trait Marker {}
             struct Foo {}
             struct Bar {}
             extend Foo with Marker {}
             extend Bar with Marker {}",
        );

        assert!(coherence(&hir).is_empty());
    }

    /// Bounds are not consulted, so these conflict even though no type could pick the wrong one.
    /// Proving otherwise takes negative reasoning, and the help text says as much.
    #[test]
    fn a_conditional_impl_still_conflicts_with_a_concrete_one() {
        let hir = resolve_src(
            "trait Marker {}
             struct Wrap<T> { inner: T }
             extend<T: Marker> Wrap<T> with Marker {}
             extend Wrap<i32> with Marker {}",
        );

        assert_eq!(coherence(&hir).len(), 1);
    }

    // -----------------------------------------------------------------
    // Check 2: duplicate method name
    // -----------------------------------------------------------------

    #[test]
    fn two_traits_declaring_one_method_name_conflict_for_a_type_implementing_both() {
        let hir = resolve_src(
            "trait A { fun size(&self); }
             trait B { fun size(&self); }
             struct Foo {}
             extend Foo with A { fun size(&self) {} }
             extend Foo with B { fun size(&self) {} }",
        );

        assert_eq!(
            coherence(&hir),
            ["the method `size` is defined more than once for type `Foo`"]
        );
    }

    /// The trait's own list is what counts, not the block's. A block that overrides nothing still
    /// makes every defaulted method available on the type, and so still collides.
    #[test]
    fn an_impl_supplying_only_defaults_still_collides() {
        let hir = resolve_src(
            "trait A { fun size(&self) {} }
             struct Foo {}
             extend Foo with A {}
             extend Foo { fun size(&self) {} }",
        );

        assert_eq!(
            coherence(&hir),
            ["the method `size` is defined more than once for type `Foo`"]
        );
    }

    #[test]
    fn an_inherent_method_conflicts_with_a_trait_method_of_the_same_name() {
        let hir = resolve_src(
            "trait A { fun size(&self); }
             struct Foo {}
             extend Foo with A { fun size(&self) {} }
             extend Foo { fun size(&self) {} }",
        );

        assert_eq!(
            coherence(&hir),
            ["the method `size` is defined more than once for type `Foo`"]
        );
    }

    #[test]
    fn two_inherent_blocks_with_different_method_names_do_not_conflict() {
        let hir = resolve_src(
            "struct Foo {}
             extend Foo { fun a(&self) {} }
             extend Foo { fun b(&self) {} }",
        );

        assert!(coherence(&hir).is_empty());
    }

    /// Blocks that cannot both apply to one type are never compared for method names, however
    /// many names they share.
    #[test]
    fn impls_for_disjoint_types_may_share_method_names() {
        let hir = resolve_src(
            "struct Wrap<T> { inner: T }
             extend Wrap<i32> { fun size(&self) {} }
             extend Wrap<bool> { fun size(&self) {} }",
        );

        assert!(coherence(&hir).is_empty());
    }

    #[test]
    fn every_shared_method_name_is_reported() {
        let hir = resolve_src(
            "struct Foo {}
             extend Foo { fun a(&self) {} fun b(&self) {} }
             extend Foo { fun a(&self) {} fun b(&self) {} }",
        );

        assert_eq!(
            coherence(&hir),
            [
                "the method `a` is defined more than once for type `Foo`",
                "the method `b` is defined more than once for type `Foo`",
            ]
        );
    }

    #[test]
    fn a_program_with_no_extend_blocks_reports_nothing() {
        let hir = resolve_src("struct Foo {}");
        assert!(coherence(&hir).is_empty());
    }
}
