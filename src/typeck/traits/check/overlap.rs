use std::collections::HashMap;

use crate::hir::HirId;
use crate::typeck::Typeck;
use crate::typeck::traits::collect::ExtendHeader;
use crate::typeck::ty::Ty;
use crate::typeck::tyctx::TyCtx;
use crate::typeck::unify::Unifier;
use crate::typeck::visitor;

impl<'hir> Typeck<'hir> {
    /// Checks whether two headers extend the same type.
    pub fn overlaps(&mut self, a: &ExtendHeader, b: &ExtendHeader) -> bool {
        if visitor::mentions_error(&self.tcx, a.self_ty)
            || visitor::mentions_error(&self.tcx, b.self_ty)
        {
            return false;
        }

        let (a_subst, b_subst) = (self.header_subst(a), self.header_subst(b));
        let mut unifier = Unifier::new();

        self.check_self_overlaps(&mut unifier, a, b, &a_subst, &b_subst)
            && self.check_trait_arguments_overlap(&mut unifier, a, b, &a_subst, &b_subst)
    }

    /// Checks whether the two headers extend the same type, by unifying their self types under
    /// their fresh parameter substitutions.
    fn check_self_overlaps(
        &mut self,
        unifier: &mut Unifier,
        a: &ExtendHeader,
        b: &ExtendHeader,
        a_subst: &visitor::Subst,
        b_subst: &visitor::Subst,
    ) -> bool {
        let (x, y) = (
            visitor::subst_ty(&mut self.tcx, a.self_ty, a_subst),
            visitor::subst_ty(&mut self.tcx, b.self_ty, b_subst),
        );
        unifier.unify(&mut self.tcx, x, y).is_ok()
    }

    /// Checks whether two headers implementing the same trait have overlapping type arguments.
    fn check_trait_arguments_overlap(
        &mut self,
        unifier: &mut Unifier,
        a: &ExtendHeader,
        b: &ExtendHeader,
        a_subst: &visitor::Subst,
        b_subst: &visitor::Subst,
    ) -> bool {
        let (Some(x), Some(y)) = (&a.trait_, &b.trait_) else {
            return true;
        };
        if x.def != y.def {
            return true;
        }

        x.args.len() == y.args.len()
            && x.args.iter().zip(y.args.iter()).all(|(&p, &q)| {
                let (p, q) = (
                    visitor::subst_ty(&mut self.tcx, p, a_subst),
                    visitor::subst_ty(&mut self.tcx, q, b_subst),
                );
                unifier.unify(&mut self.tcx, p, q).is_ok()
            })
    }

    /// Returns a substitution mapping an extend block's type parameters to inference variables.
    fn header_subst(&mut self, header: &ExtendHeader) -> visitor::Subst {
        visitor::Subst {
            generics: instantiate(&mut self.tcx, &header.generics),
            self_ty: None,
        }
    }
}

/// Creates a fresh inference variable for each generic the header declares.
fn instantiate(tcx: &mut TyCtx, generics: &[HirId]) -> HashMap<HirId, Ty> {
    generics
        .iter()
        .map(|&param| (param, tcx.next_infer_var()))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ast::Mutability;
    use crate::hir::DefId;
    use crate::nameres::PrimTy;
    use crate::typeck::traits::TraitRef;

    /// A checker with no program behind it. Every question `overlaps` asks is about headers built
    /// by hand, so nothing has to be collected into the index first.
    fn empty_checker() -> Typeck<'static> {
        let hir = Box::leak(Box::new(crate::testing::lower_to_hir("")));
        Typeck::new(crate::testing::session(), hir)
    }

    fn param(n: usize) -> HirId {
        DefId::from_usize(n).owner_id()
    }

    fn def(n: usize) -> DefId {
        DefId::from_usize(n)
    }

    /// A header with no trait: enough for every question `overlaps` asks about self types alone.
    fn header(generics: Vec<HirId>, self_ty: Ty) -> ExtendHeader {
        ExtendHeader {
            def: def(900),
            generics,
            self_ty,
            trait_: None,
        }
    }

    fn with_trait(mut header: ExtendHeader, trait_def: DefId, args: Vec<Ty>) -> ExtendHeader {
        header.trait_ = Some(TraitRef {
            def: trait_def,
            args,
        });
        header
    }

    /// `Foo` and `Bar` here are just `DefId`s to build `Adt`s around; `Show` likewise for traits.
    const FOO: usize = 1;
    const BAR: usize = 2;
    const SHOW: usize = 3;
    const OTHER_TRAIT: usize = 4;

    #[test]
    fn a_fully_generic_impl_overlaps_a_concrete_one() {
        // `extend<T> Foo<T>` against `extend Foo<i32>`: `T = i32`.
        let mut checker = empty_checker();
        let t = checker.tcx.mk_generic(param(10));
        let i32_ty = checker.tcx.mk_prim(PrimTy::I32);
        let a = header(vec![param(10)], checker.tcx.mk_adt(def(FOO), vec![t]));
        let b = header(vec![], checker.tcx.mk_adt(def(FOO), vec![i32_ty]));

        assert!(checker.overlaps(&a, &b));
        assert!(checker.overlaps(&b, &a), "overlap is symmetric");
    }

    #[test]
    fn two_impls_of_different_types_do_not_overlap() {
        let mut checker = empty_checker();
        let a = header(vec![], checker.tcx.mk_adt(def(FOO), vec![]));
        let b = header(vec![], checker.tcx.mk_adt(def(BAR), vec![]));

        assert!(!checker.overlaps(&a, &b));
    }

    #[test]
    fn two_impls_with_disjoint_concrete_arguments_do_not_overlap() {
        let mut checker = empty_checker();
        let (i32_ty, bool_ty) = (
            checker.tcx.mk_prim(PrimTy::I32),
            checker.tcx.mk_prim(PrimTy::Bool),
        );
        let a = header(vec![], checker.tcx.mk_adt(def(FOO), vec![i32_ty]));
        let b = header(vec![], checker.tcx.mk_adt(def(FOO), vec![bool_ty]));

        assert!(!checker.overlaps(&a, &b));
    }

    /// Neither header is more general than the other, which is exactly the case one-way matching
    /// cannot decide: `Foo<i32, U>` and `Foo<T, bool>` are both satisfied by `Foo<i32, bool>`.
    #[test]
    fn two_partly_concrete_impls_overlap_when_their_arguments_unify() {
        let mut checker = empty_checker();
        let (i32_ty, bool_ty) = (
            checker.tcx.mk_prim(PrimTy::I32),
            checker.tcx.mk_prim(PrimTy::Bool),
        );
        let (t, u) = (
            checker.tcx.mk_generic(param(10)),
            checker.tcx.mk_generic(param(11)),
        );
        let a = header(
            vec![param(11)],
            checker.tcx.mk_adt(def(FOO), vec![i32_ty, u]),
        );
        let b = header(
            vec![param(10)],
            checker.tcx.mk_adt(def(FOO), vec![t, bool_ty]),
        );

        assert!(checker.overlaps(&a, &b));
    }

    #[test]
    fn two_partly_concrete_impls_do_not_overlap_when_a_position_disagrees() {
        let mut checker = empty_checker();
        let (i32_ty, bool_ty) = (
            checker.tcx.mk_prim(PrimTy::I32),
            checker.tcx.mk_prim(PrimTy::Bool),
        );
        let (t, u) = (
            checker.tcx.mk_generic(param(10)),
            checker.tcx.mk_generic(param(11)),
        );
        let a = header(
            vec![param(11)],
            checker.tcx.mk_adt(def(FOO), vec![i32_ty, u]),
        );
        let b = header(
            vec![param(10)],
            checker.tcx.mk_adt(def(FOO), vec![bool_ty, t]),
        );

        assert!(!checker.overlaps(&a, &b));
    }

    /// One parameter used twice has to take one value in both places.
    #[test]
    fn a_repeated_parameter_must_bind_consistently() {
        let mut checker = empty_checker();
        let (i32_ty, bool_ty) = (
            checker.tcx.mk_prim(PrimTy::I32),
            checker.tcx.mk_prim(PrimTy::Bool),
        );
        let t = checker.tcx.mk_generic(param(10));
        let a = header(vec![param(10)], checker.tcx.mk_adt(def(FOO), vec![t, t]));
        let consistent = header(vec![], checker.tcx.mk_adt(def(FOO), vec![i32_ty, i32_ty]));
        let inconsistent = header(vec![], checker.tcx.mk_adt(def(FOO), vec![i32_ty, bool_ty]));

        assert!(checker.overlaps(&a, &consistent));
        assert!(!checker.overlaps(&a, &inconsistent));
    }

    /// A `TyKind::Generic` that is not in the header's own list is a parameter of some enclosing
    /// definition, so it is a constant here and only matches itself.
    #[test]
    fn a_generic_that_is_not_the_impls_own_parameter_is_rigid() {
        let mut checker = empty_checker();
        let outer = checker.tcx.mk_generic(param(20));
        let i32_ty = checker.tcx.mk_prim(PrimTy::I32);
        // Declares nothing of its own, so `outer` is rigid rather than bindable.
        let a = header(vec![], checker.tcx.mk_adt(def(FOO), vec![outer]));
        let concrete = header(vec![], checker.tcx.mk_adt(def(FOO), vec![i32_ty]));
        let same_rigid = header(vec![], checker.tcx.mk_adt(def(FOO), vec![outer]));

        assert!(!checker.overlaps(&a, &concrete));
        assert!(checker.overlaps(&a, &same_rigid));
    }

    /// Both sides may name the same parameter and still be two different variables. This is what
    /// a block compared against a copy of itself looks like, and the two bindings have to be
    /// tracked apart for the comparison to mean anything.
    #[test]
    fn the_two_sides_parameters_are_renamed_apart() {
        let mut checker = empty_checker();
        let t = checker.tcx.mk_generic(param(10));
        let i32_ty = checker.tcx.mk_prim(PrimTy::I32);
        let a = header(
            vec![param(10)],
            checker.tcx.mk_adt(def(FOO), vec![t, i32_ty]),
        );
        let b = header(
            vec![param(10)],
            checker.tcx.mk_adt(def(FOO), vec![i32_ty, t]),
        );

        assert!(checker.overlaps(&a, &b));
    }

    /// `T` against `Bar<T>` has no finite solution. Without the occurs check the substitution
    /// becomes cyclic and resolving it never returns, so a regression here hangs rather than
    /// fails.
    #[test]
    fn the_occurs_check_refuses_a_variable_inside_its_own_binding() {
        let mut checker = empty_checker();
        let (t, u) = (
            checker.tcx.mk_generic(param(10)),
            checker.tcx.mk_generic(param(11)),
        );
        let bar_u = checker.tcx.mk_adt(def(BAR), vec![u]);
        // `Foo<T, T>` against `Foo<Bar<U>, U>`: `T = Bar<U>`, and then `Bar<U>` must equal `U`.
        let a = header(vec![param(10)], checker.tcx.mk_adt(def(FOO), vec![t, t]));
        let b = header(
            vec![param(11)],
            checker.tcx.mk_adt(def(FOO), vec![bar_u, u]),
        );

        assert!(!checker.overlaps(&a, &b));
    }

    #[test]
    fn nested_arguments_are_compared_structurally() {
        let mut checker = empty_checker();
        let (i32_ty, bool_ty) = (
            checker.tcx.mk_prim(PrimTy::I32),
            checker.tcx.mk_prim(PrimTy::Bool),
        );
        let t = checker.tcx.mk_generic(param(10));
        let bar_t = checker.tcx.mk_adt(def(BAR), vec![t]);
        let bar_i32 = checker.tcx.mk_adt(def(BAR), vec![i32_ty]);
        let bar_bool = checker.tcx.mk_adt(def(BAR), vec![bool_ty]);

        let a = header(vec![param(10)], checker.tcx.mk_adt(def(FOO), vec![bar_t]));
        let matching = header(vec![], checker.tcx.mk_adt(def(FOO), vec![bar_i32]));
        let deeper = header(vec![], checker.tcx.mk_adt(def(FOO), vec![bar_bool]));

        assert!(checker.overlaps(&a, &matching));
        assert!(checker.overlaps(&a, &deeper), "`T` binds to either");
        assert!(!checker.overlaps(&matching, &deeper));
    }

    #[test]
    fn impls_of_the_same_trait_with_disjoint_arguments_do_not_overlap() {
        let mut checker = empty_checker();
        let (i32_ty, bool_ty) = (
            checker.tcx.mk_prim(PrimTy::I32),
            checker.tcx.mk_prim(PrimTy::Bool),
        );
        let foo = checker.tcx.mk_adt(def(FOO), vec![]);
        let a = with_trait(header(vec![], foo), def(SHOW), vec![i32_ty]);
        let b = with_trait(header(vec![], foo), def(SHOW), vec![bool_ty]);

        assert!(!checker.overlaps(&a, &b));
    }

    /// The self type and the trait arguments share one substitution, so a parameter used in both
    /// has to satisfy both at once.
    #[test]
    fn a_parameter_shared_between_the_self_type_and_the_trait_arguments_binds_once() {
        let mut checker = empty_checker();
        let (i32_ty, bool_ty) = (
            checker.tcx.mk_prim(PrimTy::I32),
            checker.tcx.mk_prim(PrimTy::Bool),
        );
        let t = checker.tcx.mk_generic(param(10));

        let a = with_trait(
            header(vec![param(10)], checker.tcx.mk_adt(def(FOO), vec![t])),
            def(SHOW),
            vec![t],
        );
        let agrees = with_trait(
            header(vec![], checker.tcx.mk_adt(def(FOO), vec![i32_ty])),
            def(SHOW),
            vec![i32_ty],
        );
        let disagrees = with_trait(
            header(vec![], checker.tcx.mk_adt(def(FOO), vec![i32_ty])),
            def(SHOW),
            vec![bool_ty],
        );

        assert!(checker.overlaps(&a, &agrees));
        assert!(!checker.overlaps(&a, &disagrees));
    }

    /// Two impls of *different* traits are still compared on their self types alone, which is
    /// what lets coherence ask whether they can offer a colliding method name.
    #[test]
    fn impls_of_different_traits_overlap_when_their_self_types_do() {
        let mut checker = empty_checker();
        let foo = checker.tcx.mk_adt(def(FOO), vec![]);
        let a = with_trait(header(vec![], foo), def(SHOW), vec![]);
        let b = with_trait(header(vec![], foo), def(OTHER_TRAIT), vec![]);
        let inherent = header(vec![], foo);

        assert!(checker.overlaps(&a, &b));
        assert!(checker.overlaps(&a, &inherent));
    }

    /// A header that failed to lower is not a conflict with everything in sight.
    #[test]
    fn an_error_type_never_overlaps() {
        let mut checker = empty_checker();
        let error = checker.tcx.error();
        let i32_ty = checker.tcx.mk_prim(PrimTy::I32);
        let a = header(vec![], checker.tcx.mk_adt(def(FOO), vec![error]));
        let b = header(vec![], checker.tcx.mk_adt(def(FOO), vec![i32_ty]));

        assert!(!checker.overlaps(&a, &b));
    }

    #[test]
    fn compound_shapes_are_compared_structurally() {
        let mut checker = empty_checker();
        let (i32_ty, bool_ty) = (
            checker.tcx.mk_prim(PrimTy::I32),
            checker.tcx.mk_prim(PrimTy::Bool),
        );
        let t = checker.tcx.mk_generic(param(10));

        let ref_t = checker.tcx.mk_ref(t, Mutability::Immutable);
        let ref_i32 = checker.tcx.mk_ref(i32_ty, Mutability::Immutable);
        let mut_ref_i32 = checker.tcx.mk_ref(i32_ty, Mutability::Mutable);
        let tuple_t = checker.tcx.mk_tuple(vec![t, bool_ty]);
        let tuple_i32 = checker.tcx.mk_tuple(vec![i32_ty, bool_ty]);

        let a = header(
            vec![param(10)],
            checker.tcx.mk_adt(def(FOO), vec![ref_t, tuple_t]),
        );
        let b = header(
            vec![],
            checker.tcx.mk_adt(def(FOO), vec![ref_i32, tuple_i32]),
        );
        let wrong_mutability = header(
            vec![],
            checker.tcx.mk_adt(def(FOO), vec![mut_ref_i32, tuple_i32]),
        );

        assert!(checker.overlaps(&a, &b));
        assert!(!checker.overlaps(&a, &wrong_mutability));
    }
}
