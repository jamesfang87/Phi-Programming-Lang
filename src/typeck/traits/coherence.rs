//! Coherence: the whole-program check that no two `extend` blocks can both answer one question.
//!
//! Without it [`implements`](crate::typeck::Typeck::implements) is not a function. Two impls that
//! both match a goal would make the answer depend on which one the index happened to see first,
//! and a call to `x.show()` would resolve to whichever body was written earlier in the file.
//!
//! Two checks, deliberately kept separate:
//!
//! - **Duplicate implementation**, grouped by `(self type's head, trait)`. This is what makes the
//!   query a function.
//! - **Duplicate method name**, grouped by the self type's head alone, so that inherent blocks
//!   and impls of *different* traits are compared against each other too. This is what a call
//!   site actually trips over.
//!
//! The second nearly implies the first -- but not for a marker trait with no methods, and keeping
//! them apart is what lets the two diagnostics say different, accurate things.
//!
//! Both checks are pairwise within a bucket, so worst case quadratic in the impls for one type.
//! Bucketing on the self type's head keeps the constant small; if it ever stops being small, the
//! fix is a discriminant prefilter on the first argument, not a different design.
//!
//! ## Overlap is decided on shape alone
//!
//! `extend<T: Show> Box<T> with Show` and `extend Box<i32> with Show` conflict here even if
//! `i32: Show` never holds, because ruling that out means proving a negative. See
//! [`overlaps`](crate::typeck::traits::overlap::overlaps); the diagnostic says so rather than
//! leaving it a mystery.

use crate::ast::Symbol;
use crate::ast::interner::Interner;
use crate::diagnostics::typeck::traits::coherence::{
    report_conflicting_extends, report_duplicate_method,
};
use crate::typeck::Typeck;
use crate::typeck::traits::index::{ExtendHeader, ImplId};
use crate::typeck::traits::overlap::overlaps;

impl<'hir> Typeck<'hir> {
    /// Runs both coherence checks over the whole index.
    ///
    /// Every conflict is reported and both impls stay in the index. Dropping one would change
    /// which of two equally-valid programs compiles depending on declaration order; the
    /// duplicate-match assertion in [`Typeck::select`](crate::typeck::Typeck) is what keeps the
    /// surviving pair from producing an arbitrary answer instead.
    pub fn check_coherence(&mut self) {
        for head in self.impls.extended_types() {
            let bucket = self.impls.for_self(head).to_vec();
            self.check_duplicate_impls(&bucket);
            self.check_duplicate_methods(&bucket);
        }
    }

    /// Check 1: within one type's bucket, no two impls of the same trait may overlap.
    ///
    /// This one test covers every case without classifying anything first. A fully generic impl
    /// overlaps everything in its bucket; two partly concrete impls conflict exactly when their
    /// argument lists unify; a bucket with one impl has no pair to check at all.
    fn check_duplicate_impls(&self, bucket: &[ImplId]) {
        for (i, &first) in bucket.iter().enumerate() {
            for &second in &bucket[i + 1..] {
                let (a, b) = (self.impls.header(first), self.impls.header(second));

                let (Some(trait_a), Some(trait_b)) = (&a.trait_ref, &b.trait_ref) else {
                    // At least one is an inherent block. Two inherent blocks on one type are
                    // perfectly legal -- they just add methods -- and a name collision between
                    // them is check 2's business.
                    continue;
                };
                if trait_a.def != trait_b.def || !overlaps(&self.tcx, a, b) {
                    continue;
                }

                report_conflicting_extends(self.hir, self.cx(), a, b);
            }
        }
    }

    /// Check 2: within one type's bucket, no two overlapping impls may offer the same method
    /// name.
    ///
    /// Grouped by the self type alone, so `extend Foo with A` and `extend Foo with B` are
    /// compared even though they implement different traits. Two traits that both declare `size`
    /// are caught here, at the declarations where it can be explained, instead of surfacing later
    /// as an ambiguity at some unrelated call site.
    fn check_duplicate_methods(&self, bucket: &[ImplId]) {
        for (i, &first) in bucket.iter().enumerate() {
            for &second in &bucket[i + 1..] {
                let (a, b) = (self.impls.header(first), self.impls.header(second));
                if !overlaps(&self.tcx, a, b) {
                    continue;
                }

                for name in self.shared_method_names(a, b) {
                    report_duplicate_method(self.cx(), name, a, b);
                }
            }
        }
    }

    /// The method names both impls make available, in a stable order.
    ///
    /// Sorted by name rather than left in hash order, because two impls colliding on several
    /// methods must report them the same way on every run.
    fn shared_method_names(&self, a: &ExtendHeader, b: &ExtendHeader) -> Vec<Symbol> {
        let (mut names, other) = (self.effective_methods(a), self.effective_methods(b));
        names.retain(|name| other.contains(name));
        names.sort_by_key(|&name| Interner::resolve(name));
        names
    }

    /// Every method name this impl gives the type it extends.
    ///
    /// For a trait impl that is the *trait's* full method list, not the block's own: a trait
    /// method with a default body is available on the type whether or not the block overrode it,
    /// so an impl that supplies nothing at all still collides with everything the trait declares.
    fn effective_methods(&self, header: &ExtendHeader) -> Vec<Symbol> {
        let Some(trait_ref) = &header.trait_ref else {
            return header.methods.keys().copied().collect();
        };

        self.hir
            .trait_(trait_ref.def)
            .functions
            .iter()
            .map(|&function| self.hir.function(function).name.text)
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use crate::diagnostics::DiagCtx;
    use crate::hir::Hir;
    use crate::testing::resolve_src;
    use crate::typeck::Typeck;

    /// Runs everything up to and including coherence over `src`, and hands back what it reported.
    ///
    /// Diagnostics from name resolution are cleared first: a fixture is resolved without the core
    /// library, so every one of them reports the whole set of missing lang items.
    fn coherence(hir: &Hir) -> Vec<String> {
        let mut checker = Typeck::new(hir);
        checker.collect_module(hir.root_id());
        checker.build_impl_index();
        DiagCtx::clear();
        checker.check_coherence();

        DiagCtx::diagnostics()
            .into_iter()
            .map(|diagnostic| diagnostic.message)
            .collect()
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

        let mut checker = Typeck::new(&hir);
        checker.collect_module(hir.root_id());
        checker.build_impl_index();
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

    /// A fully generic impl overlaps a concrete one, so the two conflict even though neither is
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

    /// The trait's own list is what counts, not the block's. An impl that overrides nothing still
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

    /// Impls that cannot both apply to one type are never compared for method names, however
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
