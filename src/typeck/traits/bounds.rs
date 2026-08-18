use std::collections::HashMap;
use std::mem;

use crate::diagnostics::typeck::traits::bounds::{
    report_annotations_needed, report_unsatisfied_bound,
};
use crate::driver::source::SrcSpan;
use crate::hir::{DefId, HirId};
use crate::typeck::traits::solve::{Query, Solution};
use crate::typeck::ty::Ty;
use crate::typeck::Typeck;

/// A trait bound which must hold
#[derive(Clone, Debug)]
pub struct Obligation {
    /// The bound to prove, for example `Bare: Show`.
    pub query: Query,
    /// Where the instantiation that raised this obligation was written.
    pub cause: SrcSpan,
    /// Where the bound itself was declared, e.g. on `Sorted`'s own `<T: Show>`.
    pub declared_at: SrcSpan,
}

impl<'hir> Typeck<'hir> {
    // -----------------------------------------------------------------
    // Registration
    // -----------------------------------------------------------------

    pub fn register_bound_obligations(
        &mut self,
        def: DefId,
        args: &[Ty],
        cause: SrcSpan,
        owner: DefId,
    ) {
        let params = self.declared_generics(def);
        if params.len() != args.len() {
            return;
        }
        let subst: HashMap<HirId, Ty> = params.iter().copied().zip(args.iter().copied()).collect();

        for &param in params {
            let declared_at = self.hir.generic(param).span;
            for bound in self.bounds_of(param) {
                let goal = self.subst_query(&bound, &subst);
                self.trait_bound_obligations
                    .entry(owner)
                    .or_default()
                    .push(Obligation {
                        query: goal,
                        cause,
                        declared_at,
                    });
            }
        }
    }

    // -----------------------------------------------------------------
    // Draining
    // -----------------------------------------------------------------

    /// This runs at the very end of type checking to attempt to resolve ambiguous queries.
    /// Each definition's obligations are proved together, against the one environment its own
    /// declaration determines, and `DoesNotHold` and `Ambiguous` are reported.
    pub fn select_obligations(&mut self) {
        for (owner, obligations) in mem::take(&mut self.trait_bound_obligations) {
            let env = self.bounds_env(owner);
            for obligation in obligations {
                match self.implements(&obligation.query, &env) {
                    Solution::Holds | Solution::Error => {}
                    Solution::DoesNotHold => {
                        report_unsatisfied_bound(self.hir, self.display_cx(), &obligation)
                    }
                    Solution::Ambiguous => {
                        report_annotations_needed(self.hir, self.display_cx(), &obligation)
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::diagnostics::DiagCtx;
    use crate::hir::Hir;
    use crate::testing::{checker_through, messages, resolve_src, Stage};

    // -----------------------------------------------------------------
    // Source-level
    // -----------------------------------------------------------------

    /// Runs everything up to and including bound checking over `src`, and hands back everything
    /// type checking reported.
    ///
    /// The clear comes *before* collection rather than after it, which is where `coherence`'s and
    /// `members`'s helpers put theirs. One of the registration sites is in `lower_ty`, which runs
    /// during collection, so clearing afterwards would hide exactly what that site says. What is
    /// cleared is name resolution's own output: a fixture is resolved without the core library, so
    /// every one of them reports the whole set of missing lang items first.
    ///
    /// Bodies are deliberately not checked, so that what is exercised here is the program-level
    /// context on its own; a fixture that needs the per-body one instead goes through
    /// [`crate::testing::typeck_rejects`], which runs the whole pipeline.
    fn bounds(hir: &Hir) -> Vec<String> {
        DiagCtx::clear();

        let mut checker = checker_through(hir, Stage::Members);
        checker.check_declared_bounds();
        checker.check_extend_headers();
        checker.select_obligations();

        messages()
    }

    #[test]
    fn a_bound_that_is_not_met_by_the_argument_is_reported() {
        let hir = resolve_src(
            "trait Show { fun show(&self); }
             struct Sorted<T: Show> { inner: T }
             struct Bare {}
             fun f(x: Sorted<Bare>) {}",
        );

        assert_eq!(
            bounds(&hir),
            ["the trait bound `Bare: Show` is not satisfied"]
        );
    }

    /// The failure is at the instantiation, but it is only a failure because of the bound the
    /// declaration writes, so the diagnostic points at both, and the bound's own span survives
    /// being re-raised against the instantiation to get there.
    #[test]
    fn an_unmet_bound_points_at_the_declaration_that_requires_it() {
        let hir = resolve_src(
            "trait Show { fun show(&self); }
             struct Sorted<T: Show> { inner: T }
             struct Bare {}
             fun f(x: Sorted<Bare>) {}",
        );

        DiagCtx::clear();
        let mut checker = Typeck::new(&hir);
        checker.collect_module(hir.root_id());
        checker.build_extend_index();
        checker.select_obligations();

        let diagnostics = DiagCtx::diagnostics();
        let [unmet] = diagnostics.as_slice() else {
            panic!("expected exactly one diagnostic, got {diagnostics:?}");
        };
        let [bound] = unmet.secondary.as_slice() else {
            panic!("expected exactly one secondary label");
        };
        assert_eq!(bound.message, "required by this bound");

        // The bound is written on `Sorted`'s declaration, above the use in `f` that failed it.
        let primary = unmet.span.expect("an unmet bound names a place");
        assert!(bound.span.get_begin() < primary.get_begin());
    }

    #[test]
    fn a_bound_met_by_an_impl_is_accepted() {
        let hir = resolve_src(
            "trait Show { fun show(&self); }
             struct Sorted<T: Show> { inner: T }
             struct Foo {}
             extend Foo with Show { fun show(&self) {} }
             fun f(x: Sorted<Foo>) {}",
        );

        assert!(bounds(&hir).is_empty());
    }

    /// The conditional block's own bound is proved recursively, so the whole chain either holds
    /// or fails as one.
    #[test]
    fn a_bound_met_through_a_conditional_impl_is_accepted() {
        let hir = resolve_src(
            "trait Show { fun show(&self); }
             struct Sorted<T: Show> { inner: T }
             struct Wrap<T> { inner: T }
             struct Foo {}
             extend Foo with Show { fun show(&self) {} }
             extend<T: Show> Wrap<T> with Show { fun show(&self) {} }
             fun f(x: Sorted<Wrap<Foo>>) {}",
        );

        assert!(bounds(&hir).is_empty(), "{:?}", bounds(&hir));
    }

    #[test]
    fn a_conditional_impl_whose_own_bound_fails_does_not_satisfy_the_goal() {
        let hir = resolve_src(
            "trait Show { fun show(&self); }
             struct Sorted<T: Show> { inner: T }
             struct Wrap<T> { inner: T }
             struct Bare {}
             extend<T: Show> Wrap<T> with Show { fun show(&self) {} }
             fun f(x: Sorted<Wrap<Bare>>) {}",
        );

        assert_eq!(
            bounds(&hir),
            ["the trait bound `Wrap<Bare>: Show` is not satisfied"]
        );
    }

    /// The case the `BoundsEnv` exists for: nothing is known about `U` except what `f` declared,
    /// which is sufficient to discharge the bound.
    #[test]
    fn a_bound_met_by_an_assumption_in_scope_is_accepted() {
        let hir = resolve_src(
            "trait Show { fun show(&self); }
             struct Sorted<T: Show> { inner: T }
             fun f<U: Show>(x: Sorted<U>) {}",
        );

        assert!(bounds(&hir).is_empty());
    }

    #[test]
    fn a_parameter_passed_on_without_the_bound_it_needs_is_reported() {
        let hir = resolve_src(
            "trait Show { fun show(&self); }
             struct Sorted<T: Show> { inner: T }
             fun f<U>(x: Sorted<U>) {}",
        );

        assert_eq!(bounds(&hir), ["the trait bound `U: Show` is not satisfied"]);
    }

    /// An `extend` block instantiates the type it extends, so its arguments are checked like any
    /// other.
    #[test]
    fn an_extend_blocks_arguments_have_to_satisfy_the_extended_types_bounds() {
        let hir = resolve_src(
            "trait Show { fun show(&self); }
             struct Sorted<T: Show> { inner: T }
             struct Bare {}
             extend Sorted<Bare> { fun get(&self) {} }",
        );

        assert_eq!(
            bounds(&hir),
            ["the trait bound `Bare: Show` is not satisfied"]
        );
    }

    // -----------------------------------------------------------------
    // Draining
    //
    // What the single pass over real inference still has to get right, now that it is not a
    // loop. See "Why deferral" in the module docs.
    // -----------------------------------------------------------------

    /// A goal built from an already-broken type, such as a reference that failed to resolve,
    /// answers [`Solution::Error`] and is discharged without comment: a diagnostic for the
    /// broken reference already exists, and adding a second one about the bound it happens to
    /// sit in would be noise about the same mistake.
    #[test]
    fn a_bound_about_an_already_broken_type_is_discharged_silently() {
        let hir = resolve_src(
            "trait Show { fun show(&self); }
             struct Sorted<T: Show> { inner: T }
             fun f(x: Sorted<Nope>) {}",
        );

        assert!(bounds(&hir).is_empty());
    }

    /// A goal can still be genuinely undecided once there is nowhere left to check it from: a
    /// generic call whose own type parameter is never pinned down by anything in the body that
    /// calls it. A single pass at the end of the body is enough to report this, since looping
    /// past it would not change the answer, as nothing between one attempt and the next could
    /// have moved.
    #[test]
    fn a_bound_that_never_settles_is_reported_as_needing_an_annotation() {
        crate::testing::typeck_rejects(
            "trait Show { fun show(&self); }
             fun sort<T: Show>() -> T { return sort(); }
             fun f() { let x = sort(); }",
            "type annotations needed",
        );
    }

    // -----------------------------------------------------------------
    // Several bounds at once
    // -----------------------------------------------------------------

    /// `T: A + B` raises one obligation per trait named; both have to hold.
    #[test]
    fn a_type_parameter_with_two_bounds_needs_both_satisfied() {
        crate::testing::typeck_accepts(
            "trait A { fun a(&self); }
             trait B { fun b(&self); }
             struct Both {}
             extend Both with A { fun a(&self) {} }
             extend Both with B { fun b(&self) {} }
             fun f<T: A + B>(x: T) {}
             fun g(x: Both) { f(x); }",
        );
    }

    /// Same shape, but the argument only implements one of the two, so exactly the missing one
    /// is reported.
    #[test]
    fn a_type_parameter_with_two_bounds_reports_whichever_one_is_unmet() {
        let messages = crate::testing::typeck_src(
            "trait A { fun a(&self); }
             trait B { fun b(&self); }
             struct OnlyA {}
             extend OnlyA with A { fun a(&self) {} }
             fun f<T: A + B>(x: T) {}
             fun g(x: OnlyA) { f(x); }",
        );
        assert_eq!(
            messages,
            ["the trait bound `OnlyA: B` is not satisfied"],
            "{messages:?}"
        );
    }

    /// Two independently declared type parameters, each with its own bound, are checked
    /// independently, so a failure on one does not silence or duplicate onto the other.
    #[test]
    fn two_independently_bounded_parameters_are_each_checked_on_their_own() {
        let messages = crate::testing::typeck_src(
            "trait Show { fun show(&self); }
             struct Bare1 {}
             struct Bare2 {}
             fun f<T: Show, U: Show>(x: T, y: U) {}
             fun g(a: Bare1, b: Bare2) { f(a, b); }",
        );
        assert_eq!(
            messages,
            [
                "the trait bound `Bare1: Show` is not satisfied",
                "the trait bound `Bare2: Show` is not satisfied",
            ],
            "{messages:?}"
        );
    }
}
