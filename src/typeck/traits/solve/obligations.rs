use std::collections::HashMap;
use std::mem;

use crate::diagnostics::typeck::traits::bounds::{
    report_annotations_needed, report_unsatisfied_bound,
};
use crate::driver::source::SrcSpan;
use crate::hir::{DefId, HirId};
use crate::typeck::Typeck;
use crate::typeck::traits::TraitRef;
use crate::typeck::ty::{Ty, TyKind};

use super::{BoundsEnv, Goal, Solution};

/// A trait bound which must hold.
#[derive(Clone, Debug)]
pub struct Obligation {
    /// The bound to prove, for example `Bare: Show`.
    pub query: Goal,
    /// Where the instantiation that raised this obligation was written.
    pub cause: SrcSpan,
    /// Where the bound itself was declared, e.g. on `Sorted`'s own `<T: Show>`.
    pub declared_at: SrcSpan,
}

impl<'hir> Typeck<'hir> {
    // -----------------------------------------------------------------
    // Recording
    // -----------------------------------------------------------------

    /// Records what `def` applied to `args` requires of its own parameters.
    pub fn register_bound_obligations(
        &mut self,
        def: DefId,
        args: &[Ty],
        cause: SrcSpan,
        owner: DefId,
    ) {
        // The bounds are proved after the whole body is checked, not here, because the body may
        // still constrain the types they mention.
        let obligations = self.bound_obligations_of(def, args, cause);
        self.trait_bound_obligations
            .entry(owner)
            .or_default()
            .extend(obligations);
    }

    /// Returns the obligations `def` applied to `args` raises for its own parameters. It is empty
    /// when the arguments do not line up with the parameters, since there is nothing to
    /// substitute.
    fn bound_obligations_of(&mut self, def: DefId, args: &[Ty], cause: SrcSpan) -> Vec<Obligation> {
        let params = self.declared_generics(def);
        if params.len() != args.len() {
            return Vec::new();
        }
        let subst: HashMap<HirId, Ty> = params.iter().copied().zip(args.iter().copied()).collect();

        let mut obligations = Vec::new();
        for &param in params {
            let declared_at = self.hir.generic(param).span;
            for bound in self.bounds_of(param) {
                obligations.push(Obligation {
                    query: self.subst_query(&bound, &subst),
                    cause,
                    declared_at,
                });
            }
        }
        obligations
    }

    /// Records the bounds every `extend` block's header arguments must satisfy.
    pub fn register_extend_header_bounds(&mut self) {
        for block in self.extends.all() {
            self.register_extended_type_bounds(block);
            self.register_implemented_trait_bounds(block);
        }
    }

    /// Records the bounds the extended type's declaration requires of the arguments it was
    /// applied to. Only an ADT has such a declaration.
    fn register_extended_type_bounds(&mut self, block: DefId) {
        let self_ty = self.extended_type(block);
        let span = self.hir.ty(self.hir.extend(block).self_ty).span;
        if let TyKind::Adt { def, args } = self.tcx.kind(self_ty).clone() {
            self.register_bound_obligations(def, &args, span, block);
        }
    }

    /// Records the bounds the `with`-clause trait's declaration requires of the arguments it was
    /// applied to.
    fn register_implemented_trait_bounds(&mut self, block: DefId) {
        let Some(trait_ref) = self.extends.trait_of(block).cloned() else {
            return;
        };
        let span = self.trait_path_span(block);
        self.register_bound_obligations(trait_ref.def, &trait_ref.args, span, block);
    }

    // -----------------------------------------------------------------
    // Proving
    // -----------------------------------------------------------------

    /// Proves every bound recorded during checking, now that all bodies are done, and reports
    /// the ones that do not hold or never resolved.
    pub fn check_bound_obligations(&mut self) {
        for (owner, obligations) in mem::take(&mut self.trait_bound_obligations) {
            self.check_obligations(owner, obligations);
        }
    }

    /// Proves `obligations` against the one environment `owner`'s declaration determines.
    fn check_obligations(&mut self, owner: DefId, obligations: Vec<Obligation>) {
        let env = self.bounds_env(owner);
        for obligation in obligations {
            self.check_obligation(&obligation, &env);
        }
    }

    fn check_obligation(&mut self, obligation: &Obligation, env: &BoundsEnv) {
        let query = self.default_numeric_goal(&obligation.query);
        match self.implements(&query, env) {
            Solution::Holds | Solution::Error => {}
            Solution::DoesNotHold => {
                report_unsatisfied_bound(self.hir, self.display_cx(), obligation)
            }
            Solution::Ambiguous => {
                report_annotations_needed(self.hir, self.display_cx(), obligation)
            }
        }
    }

    /// Returns `goal` with the language's default type committed to every unconstrained numeric
    /// variable it still holds.
    fn default_numeric_goal(&mut self, goal: &Goal) -> Goal {
        Goal {
            self_ty: self.default_numeric_ty(goal.self_ty),
            trait_: TraitRef {
                def: goal.trait_.def,
                args: goal
                    .trait_
                    .args
                    .iter()
                    .map(|&arg| self.default_numeric_ty(arg))
                    .collect(),
            },
        }
    }

    fn default_numeric_ty(&mut self, ty: Ty) -> Ty {
        let resolved = self.unifier.find_deep(&mut self.tcx, ty);
        self.commit_numeric_defaults(resolved);
        self.unifier.find_deep(&mut self.tcx, resolved)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::hir::Hir;
    use crate::testing::{TypeckStage, checker_through, lower_to_hir};

    fn bounds(hir: &Hir) -> Vec<String> {
        crate::testing::clear_diagnostics();

        let mut checker = checker_through(hir, TypeckStage::Members);
        checker.check_all_bounds_are_traits();
        checker.check_extend_headers_arity();
        checker.register_extend_header_bounds();
        checker.check_bound_obligations();

        crate::testing::messages()
    }

    #[test]
    fn a_bound_that_is_not_met_by_the_argument_is_reported() {
        let hir = lower_to_hir(
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

    #[test]
    fn an_unmet_bound_points_at_the_declaration_that_requires_it() {
        let hir = lower_to_hir(
            "trait Show { fun show(&self); }
             struct Sorted<T: Show> { inner: T }
             struct Bare {}
             fun f(x: Sorted<Bare>) {}",
        );

        crate::testing::clear_diagnostics();
        let mut checker = Typeck::new(crate::testing::session(), &hir);
        checker.collect_module(hir.root_id());
        checker.collect_traits();
        checker.check_bound_obligations();

        let diagnostics = crate::testing::diagnostics();
        let [unmet] = diagnostics.as_slice() else {
            panic!("expected exactly one diagnostic, got {diagnostics:?}");
        };
        let [bound] = unmet.secondary.as_slice() else {
            panic!("expected exactly one secondary label");
        };
        assert_eq!(bound.message, "required by this bound");

        let primary = unmet.span.expect("an unmet bound names a place");
        assert!(bound.span.get_begin() < primary.get_begin());
    }

    #[test]
    fn a_bound_met_by_an_impl_is_accepted() {
        let hir = lower_to_hir(
            "trait Show { fun show(&self); }
             struct Sorted<T: Show> { inner: T }
             struct Foo {}
             extend Foo with Show { fun show(&self) {} }
             fun f(x: Sorted<Foo>) {}",
        );

        assert!(bounds(&hir).is_empty());
    }

    #[test]
    fn a_bound_met_through_a_conditional_impl_is_accepted() {
        let hir = lower_to_hir(
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
        let hir = lower_to_hir(
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

    #[test]
    fn a_bound_met_by_an_assumption_in_scope_is_accepted() {
        let hir = lower_to_hir(
            "trait Show { fun show(&self); }
             struct Sorted<T: Show> { inner: T }
             fun f<U: Show>(x: Sorted<U>) {}",
        );

        assert!(bounds(&hir).is_empty());
    }

    #[test]
    fn a_parameter_passed_on_without_the_bound_it_needs_is_reported() {
        let hir = lower_to_hir(
            "trait Show { fun show(&self); }
             struct Sorted<T: Show> { inner: T }
             fun f<U>(x: Sorted<U>) {}",
        );

        assert_eq!(bounds(&hir), ["the trait bound `U: Show` is not satisfied"]);
    }

    #[test]
    fn an_extend_blocks_arguments_have_to_satisfy_the_extended_types_bounds() {
        let hir = lower_to_hir(
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

    #[test]
    fn a_bound_about_an_already_broken_type_is_discharged_silently() {
        let hir = lower_to_hir(
            "trait Show { fun show(&self); }
             struct Sorted<T: Show> { inner: T }
             fun f(x: Sorted<Nope>) {}",
        );

        assert!(bounds(&hir).is_empty());
    }

    #[test]
    fn a_bound_that_never_resolves_is_reported_as_needing_an_annotation() {
        crate::testing::typeck_rejects(
            "trait Show { fun show(&self); }
             fun sort<T: Show>() -> T { return sort(); }
             fun f() { let x = sort(); }",
            "type annotations needed",
        );
    }

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

/// A bound written with trait arguments, `T: Conv<i32>`, holds only against an impl whose
/// own trait arguments match, so `extend W with Conv<bool>` does not satisfy it.
#[test]
fn a_bound_with_arguments_must_match_the_impls_arguments() {
    crate::testing::typeck_accepts(
        "trait Conv<From> { fun conv(&self) -> From; }
             struct W { v: i32 }
             extend W with Conv<i32> { fun conv(&self) -> i32 { return self.v; } }
             fun f<T: Conv<i32>>(x: &T) -> i32 { return x.conv(); }
             fun g() { let w: W = W { v: 1 }; let n = f(&w); }",
    );

    let messages = crate::testing::typeck_src(
        "trait Conv<From> { fun conv(&self) -> From; }
             struct W { v: i32 }
             extend W with Conv<bool> { fun conv(&self) -> bool { return true; } }
             fun f<T: Conv<i32>>(x: &T) -> i32 { return x.conv(); }
             fun g() { let w: W = W { v: 1 }; let n = f(&w); }",
    );
    assert_eq!(
        messages,
        ["the trait bound `W: Conv<i32>` is not satisfied"],
        "{messages:?}"
    );
}

/// A trait default dispatched through a bounded parameter resolves to the implementing
/// type's own methods, with the trait's own arguments substituted from the bound.
#[test]
fn a_trait_default_through_a_bound_with_arguments_dispatches() {
    crate::testing::typeck_accepts(
            "trait Conv<From> { fun conv(&self) -> From; fun boxed(&self) -> From { return self.conv(); } }
             struct W { v: i32 }
             extend W with Conv<i32> { fun conv(&self) -> i32 { return self.v; } }
             fun f<T: Conv<i32>>(x: &T) -> i32 { return x.boxed(); }
             fun g() -> i32 { let w: W = W { v: 1 }; return f(&w); }",
        );
}
