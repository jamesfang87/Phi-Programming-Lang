//! Coherence checks for
//! - Duplicate implementation
//! - Duplicate method names

use crate::ast::Symbol;
use crate::diagnostics::typeck::traits::coherence::{
    report_conflicting_extends, report_duplicate_method,
};
use crate::hir::DefId;
use crate::typeck::Typeck;
use crate::typeck::traits::collect::ExtendHeader;

impl<'hir> Typeck<'hir> {
    pub fn check_coherence(&mut self) {
        for (first, second) in self.extends.pairs_per_type() {
            self.check_overlapping_extends(first, second);
        }
    }

    // Checks for two distinct extend blocks (1) implementing the same trait and (2)
    // declaring the same methods
    fn check_overlapping_extends(&mut self, first: DefId, second: DefId) {
        let (a, b) = (self.extend_header(first), self.extend_header(second));
        if !self.overlaps(&a, &b) {
            return;
        }

        self.check_conflicting_extends(&a, &b);
        self.check_duplicate_methods(first, second, &a, &b);
    }

    /// Reports two distinct blocks that implement the same trait.
    fn check_conflicting_extends(&mut self, a: &ExtendHeader, b: &ExtendHeader) {
        if let (Some(trait_a), Some(trait_b)) = (&a.trait_, &b.trait_)
            && trait_a.def == trait_b.def
        {
            report_conflicting_extends(self.hir, self.display_cx(), a, b);
        }
    }

    /// Reports that there are no duplicate methods.
    fn check_duplicate_methods(
        &mut self,
        first: DefId,
        second: DefId,
        a: &ExtendHeader,
        b: &ExtendHeader,
    ) {
        for name in self.shared_method_names(first, second) {
            report_duplicate_method(self.hir, self.display_cx(), name, a, b);
        }
    }

    /// Returns a Vec of the methods present in both a and b's declared methods
    fn shared_method_names(&self, a: DefId, b: DefId) -> Vec<Symbol> {
        let (mut names, other) = (self.declared_methods(a), self.declared_methods(b));
        names.retain(|name| other.contains(name));
        names.sort_by_key(|&name| self.session.resolve(name));
        names
    }

    /// Returns a Vec of the methods that this extend block declares.
    /// This also includes default implementations from Traits
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
    use crate::hir::Hir;
    use crate::testing::{TypeckStage, checker_through, lower_to_hir};

    fn coherence(hir: &Hir) -> Vec<String> {
        let mut checker = checker_through(hir, TypeckStage::Index);
        crate::testing::clear_diagnostics();
        checker.check_coherence();
        crate::testing::messages()
    }

    #[test]
    fn implementing_one_trait_twice_for_one_type_is_reported() {
        let hir = lower_to_hir(
            "trait Show { fun show(&self); }
             struct Foo {}
             extend Foo with Show { fun show(&self) {} }
             extend Foo with Show { fun show(&self) {} }",
        );

        assert_eq!(
            coherence(&hir),
            [
                "conflicting implementations of trait `Show` for type `Foo`",
                "the method `show` is defined more than once for type `Foo`",
            ]
        );
    }

    #[test]
    fn a_conflict_points_at_both_blocks() {
        let hir = lower_to_hir(
            "trait Marker {}
             struct Foo {}
             extend Foo with Marker {}
             extend Foo with Marker {}",
        );

        let mut checker = checker_through(&hir, TypeckStage::Index);
        crate::testing::clear_diagnostics();
        checker.check_coherence();

        let diagnostics = crate::testing::diagnostics();
        let [conflict] = diagnostics.as_slice() else {
            panic!("expected exactly one diagnostic, got {diagnostics:?}");
        };
        let [first] = conflict.secondary.as_slice() else {
            panic!("expected exactly one secondary label");
        };
        assert_eq!(first.message, "`Foo` is already implemented here");

        let primary = conflict.span.expect("a conflict names a place");
        assert!(first.span.get_begin() < primary.get_begin());
    }

    #[test]
    fn implementing_a_method_less_trait_twice_is_still_reported() {
        let hir = lower_to_hir(
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

    #[test]
    fn a_generic_impl_conflicts_with_a_concrete_one() {
        let hir = lower_to_hir(
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
        let hir = lower_to_hir(
            "trait Marker {}
             struct Wrap<T> { inner: T }
             extend Wrap<i32> with Marker {}
             extend Wrap<bool> with Marker {}",
        );

        assert!(coherence(&hir).is_empty());
    }

    #[test]
    fn impls_of_different_traits_for_one_type_do_not_conflict() {
        let hir = lower_to_hir(
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
        let hir = lower_to_hir(
            "trait Marker {}
             struct Foo {}
             struct Bar {}
             extend Foo with Marker {}
             extend Bar with Marker {}",
        );

        assert!(coherence(&hir).is_empty());
    }

    #[test]
    fn a_conditional_impl_still_conflicts_with_a_concrete_one() {
        let hir = lower_to_hir(
            "trait Marker {}
             struct Wrap<T> { inner: T }
             extend<T: Marker> Wrap<T> with Marker {}
             extend Wrap<i32> with Marker {}",
        );

        assert_eq!(coherence(&hir).len(), 1);
    }

    #[test]
    fn two_traits_declaring_one_method_name_conflict_for_a_type_implementing_both() {
        let hir = lower_to_hir(
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

    #[test]
    fn an_impl_supplying_only_defaults_still_collides() {
        let hir = lower_to_hir(
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
        let hir = lower_to_hir(
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
        let hir = lower_to_hir(
            "struct Foo {}
             extend Foo { fun a(&self) {} }
             extend Foo { fun b(&self) {} }",
        );

        assert!(coherence(&hir).is_empty());
    }

    #[test]
    fn impls_for_disjoint_types_may_share_method_names() {
        let hir = lower_to_hir(
            "struct Wrap<T> { inner: T }
             extend Wrap<i32> { fun size(&self) {} }
             extend Wrap<bool> { fun size(&self) {} }",
        );

        assert!(coherence(&hir).is_empty());
    }

    #[test]
    fn every_shared_method_name_is_reported() {
        let hir = lower_to_hir(
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
        let hir = lower_to_hir("struct Foo {}");
        assert!(coherence(&hir).is_empty());
    }
}
