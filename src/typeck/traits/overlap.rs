use std::collections::HashMap;

use crate::hir::{DefId, HirId};
use crate::typeck::Typeck;
use crate::typeck::fold;
use crate::typeck::traits::TraitRef;
use crate::typeck::ty::Ty;
use crate::typeck::tyctx::TyCtx;
use crate::typeck::unify::Unifier;

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
    /// Reads `block`'s header out of the places its parts live.
    pub(crate) fn extend_header(&self, block: DefId) -> ExtendHeader {
        ExtendHeader {
            def: block,
            self_ty: self.adt_of_with_args(block),
            generics: self.declared_generics(block).to_vec(),
            trait_: self.extends.trait_of(block).cloned(),
        }
    }
}

pub fn overlaps(tcx: &mut TyCtx, a: &ExtendHeader, b: &ExtendHeader) -> bool {
    if fold::mentions_error(tcx, a.self_ty) || fold::mentions_error(tcx, b.self_ty) {
        return false;
    }

    let (a_subst, b_subst) = (instantiate(tcx, &a.generics), instantiate(tcx, &b.generics));
    let mut unifier = Unifier::new();

    let (x, y) = (
        fold::subst_ty(tcx, a.self_ty, &a_subst),
        fold::subst_ty(tcx, b.self_ty, &b_subst),
    );
    if unifier.unify(tcx, x, y).is_err() {
        return false;
    }

    match (&a.trait_, &b.trait_) {
        (Some(x), Some(y)) if x.def == y.def => {
            x.args.len() == y.args.len()
                && x.args.iter().zip(y.args.iter()).all(|(&p, &q)| {
                    let (p, q) = (
                        fold::subst_ty(tcx, p, &a_subst),
                        fold::subst_ty(tcx, q, &b_subst),
                    );
                    unifier.unify(tcx, p, q).is_ok()
                })
        }
        _ => true,
    }
}

/// Creates a fresh inference variable for each generic the header declares.
fn instantiate(tcx: &mut TyCtx, generics: &[HirId]) -> HashMap<HirId, Ty> {
    generics
        .iter()
        .map(|&param| (param, tcx.next_ty_var()))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ast::Mutability;
    use crate::hir::DefId;
    use crate::nameres::PrimTy;
    use crate::typeck::traits::TraitRef;

    /// A stand-in `HirId`. Only its identity matters here: nothing in `overlaps` looks a
    /// parameter's declaration up, it only asks whether the id is in a header's own list.
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
        let mut tcx = TyCtx::new();
        let t = tcx.mk_generic(param(10));
        let i32_ty = tcx.mk_prim(PrimTy::I32);
        let a = header(vec![param(10)], tcx.mk_adt(def(FOO), vec![t]));
        let b = header(vec![], tcx.mk_adt(def(FOO), vec![i32_ty]));

        assert!(overlaps(&mut tcx, &a, &b));
        assert!(overlaps(&mut tcx, &b, &a), "overlap is symmetric");
    }

    #[test]
    fn two_impls_of_different_types_do_not_overlap() {
        let mut tcx = TyCtx::new();
        let a = header(vec![], tcx.mk_adt(def(FOO), vec![]));
        let b = header(vec![], tcx.mk_adt(def(BAR), vec![]));

        assert!(!overlaps(&mut tcx, &a, &b));
    }

    #[test]
    fn two_impls_with_disjoint_concrete_arguments_do_not_overlap() {
        let mut tcx = TyCtx::new();
        let (i32_ty, bool_ty) = (tcx.mk_prim(PrimTy::I32), tcx.mk_prim(PrimTy::Bool));
        let a = header(vec![], tcx.mk_adt(def(FOO), vec![i32_ty]));
        let b = header(vec![], tcx.mk_adt(def(FOO), vec![bool_ty]));

        assert!(!overlaps(&mut tcx, &a, &b));
    }

    /// Neither header is more general than the other, which is exactly the case one-way matching
    /// cannot decide: `Foo<i32, U>` and `Foo<T, bool>` are both satisfied by `Foo<i32, bool>`.
    #[test]
    fn two_partly_concrete_impls_overlap_when_their_arguments_unify() {
        let mut tcx = TyCtx::new();
        let (i32_ty, bool_ty) = (tcx.mk_prim(PrimTy::I32), tcx.mk_prim(PrimTy::Bool));
        let (t, u) = (tcx.mk_generic(param(10)), tcx.mk_generic(param(11)));
        let a = header(vec![param(11)], tcx.mk_adt(def(FOO), vec![i32_ty, u]));
        let b = header(vec![param(10)], tcx.mk_adt(def(FOO), vec![t, bool_ty]));

        assert!(overlaps(&mut tcx, &a, &b));
    }

    #[test]
    fn two_partly_concrete_impls_do_not_overlap_when_a_position_disagrees() {
        let mut tcx = TyCtx::new();
        let (i32_ty, bool_ty) = (tcx.mk_prim(PrimTy::I32), tcx.mk_prim(PrimTy::Bool));
        let (t, u) = (tcx.mk_generic(param(10)), tcx.mk_generic(param(11)));
        let a = header(vec![param(11)], tcx.mk_adt(def(FOO), vec![i32_ty, u]));
        let b = header(vec![param(10)], tcx.mk_adt(def(FOO), vec![bool_ty, t]));

        assert!(!overlaps(&mut tcx, &a, &b));
    }

    /// One parameter used twice has to take one value in both places.
    #[test]
    fn a_repeated_parameter_must_bind_consistently() {
        let mut tcx = TyCtx::new();
        let (i32_ty, bool_ty) = (tcx.mk_prim(PrimTy::I32), tcx.mk_prim(PrimTy::Bool));
        let t = tcx.mk_generic(param(10));
        let a = header(vec![param(10)], tcx.mk_adt(def(FOO), vec![t, t]));
        let consistent = header(vec![], tcx.mk_adt(def(FOO), vec![i32_ty, i32_ty]));
        let inconsistent = header(vec![], tcx.mk_adt(def(FOO), vec![i32_ty, bool_ty]));

        assert!(overlaps(&mut tcx, &a, &consistent));
        assert!(!overlaps(&mut tcx, &a, &inconsistent));
    }

    /// A `TyKind::Generic` that is not in the header's own list is a parameter of some enclosing
    /// definition, so it is a constant here and only matches itself.
    #[test]
    fn a_generic_that_is_not_the_impls_own_parameter_is_rigid() {
        let mut tcx = TyCtx::new();
        let outer = tcx.mk_generic(param(20));
        let i32_ty = tcx.mk_prim(PrimTy::I32);
        // Declares nothing of its own, so `outer` is rigid rather than bindable.
        let a = header(vec![], tcx.mk_adt(def(FOO), vec![outer]));
        let concrete = header(vec![], tcx.mk_adt(def(FOO), vec![i32_ty]));
        let same_rigid = header(vec![], tcx.mk_adt(def(FOO), vec![outer]));

        assert!(!overlaps(&mut tcx, &a, &concrete));
        assert!(overlaps(&mut tcx, &a, &same_rigid));
    }

    /// Both sides may name the same parameter and still be two different variables. This is what
    /// a block compared against a copy of itself looks like, and the two bindings have to be
    /// tracked apart for the comparison to mean anything.
    #[test]
    fn the_two_sides_parameters_are_renamed_apart() {
        let mut tcx = TyCtx::new();
        let t = tcx.mk_generic(param(10));
        let i32_ty = tcx.mk_prim(PrimTy::I32);
        let a = header(vec![param(10)], tcx.mk_adt(def(FOO), vec![t, i32_ty]));
        let b = header(vec![param(10)], tcx.mk_adt(def(FOO), vec![i32_ty, t]));

        assert!(overlaps(&mut tcx, &a, &b));
    }

    /// `T` against `Bar<T>` has no finite solution. Without the occurs check the substitution
    /// becomes cyclic and resolving it never returns, so a regression here hangs rather than
    /// fails.
    #[test]
    fn the_occurs_check_refuses_a_variable_inside_its_own_binding() {
        let mut tcx = TyCtx::new();
        let (t, u) = (tcx.mk_generic(param(10)), tcx.mk_generic(param(11)));
        let bar_u = tcx.mk_adt(def(BAR), vec![u]);
        // `Foo<T, T>` against `Foo<Bar<U>, U>`: `T = Bar<U>`, and then `Bar<U>` must equal `U`.
        let a = header(vec![param(10)], tcx.mk_adt(def(FOO), vec![t, t]));
        let b = header(vec![param(11)], tcx.mk_adt(def(FOO), vec![bar_u, u]));

        assert!(!overlaps(&mut tcx, &a, &b));
    }

    #[test]
    fn nested_arguments_are_compared_structurally() {
        let mut tcx = TyCtx::new();
        let (i32_ty, bool_ty) = (tcx.mk_prim(PrimTy::I32), tcx.mk_prim(PrimTy::Bool));
        let t = tcx.mk_generic(param(10));
        let bar_t = tcx.mk_adt(def(BAR), vec![t]);
        let bar_i32 = tcx.mk_adt(def(BAR), vec![i32_ty]);
        let bar_bool = tcx.mk_adt(def(BAR), vec![bool_ty]);

        let a = header(vec![param(10)], tcx.mk_adt(def(FOO), vec![bar_t]));
        let matching = header(vec![], tcx.mk_adt(def(FOO), vec![bar_i32]));
        let deeper = header(vec![], tcx.mk_adt(def(FOO), vec![bar_bool]));

        assert!(overlaps(&mut tcx, &a, &matching));
        assert!(overlaps(&mut tcx, &a, &deeper), "`T` binds to either");
        assert!(!overlaps(&mut tcx, &matching, &deeper));
    }

    #[test]
    fn impls_of_the_same_trait_with_disjoint_arguments_do_not_overlap() {
        let mut tcx = TyCtx::new();
        let (i32_ty, bool_ty) = (tcx.mk_prim(PrimTy::I32), tcx.mk_prim(PrimTy::Bool));
        let foo = tcx.mk_adt(def(FOO), vec![]);
        let a = with_trait(header(vec![], foo), def(SHOW), vec![i32_ty]);
        let b = with_trait(header(vec![], foo), def(SHOW), vec![bool_ty]);

        assert!(!overlaps(&mut tcx, &a, &b));
    }

    /// The self type and the trait arguments share one substitution, so a parameter used in both
    /// has to satisfy both at once.
    #[test]
    fn a_parameter_shared_between_the_self_type_and_the_trait_arguments_binds_once() {
        let mut tcx = TyCtx::new();
        let (i32_ty, bool_ty) = (tcx.mk_prim(PrimTy::I32), tcx.mk_prim(PrimTy::Bool));
        let t = tcx.mk_generic(param(10));

        let a = with_trait(
            header(vec![param(10)], tcx.mk_adt(def(FOO), vec![t])),
            def(SHOW),
            vec![t],
        );
        let agrees = with_trait(
            header(vec![], tcx.mk_adt(def(FOO), vec![i32_ty])),
            def(SHOW),
            vec![i32_ty],
        );
        let disagrees = with_trait(
            header(vec![], tcx.mk_adt(def(FOO), vec![i32_ty])),
            def(SHOW),
            vec![bool_ty],
        );

        assert!(overlaps(&mut tcx, &a, &agrees));
        assert!(!overlaps(&mut tcx, &a, &disagrees));
    }

    /// Two impls of *different* traits are still compared on their self types alone, which is
    /// what lets coherence ask whether they can offer a colliding method name.
    #[test]
    fn impls_of_different_traits_overlap_when_their_self_types_do() {
        let mut tcx = TyCtx::new();
        let foo = tcx.mk_adt(def(FOO), vec![]);
        let a = with_trait(header(vec![], foo), def(SHOW), vec![]);
        let b = with_trait(header(vec![], foo), def(OTHER_TRAIT), vec![]);
        let inherent = header(vec![], foo);

        assert!(overlaps(&mut tcx, &a, &b));
        assert!(overlaps(&mut tcx, &a, &inherent));
    }

    /// A header that failed to lower is not a conflict with everything in sight.
    #[test]
    fn an_error_type_never_overlaps() {
        let mut tcx = TyCtx::new();
        let error = tcx.error();
        let i32_ty = tcx.mk_prim(PrimTy::I32);
        let a = header(vec![], tcx.mk_adt(def(FOO), vec![error]));
        let b = header(vec![], tcx.mk_adt(def(FOO), vec![i32_ty]));

        assert!(!overlaps(&mut tcx, &a, &b));
    }

    #[test]
    fn compound_shapes_are_compared_structurally() {
        let mut tcx = TyCtx::new();
        let (i32_ty, bool_ty) = (tcx.mk_prim(PrimTy::I32), tcx.mk_prim(PrimTy::Bool));
        let t = tcx.mk_generic(param(10));

        let ref_t = tcx.mk_ref(t, Mutability::Immutable);
        let ref_i32 = tcx.mk_ref(i32_ty, Mutability::Immutable);
        let mut_ref_i32 = tcx.mk_ref(i32_ty, Mutability::Mutable);
        let tuple_t = tcx.mk_tuple(vec![t, bool_ty]);
        let tuple_i32 = tcx.mk_tuple(vec![i32_ty, bool_ty]);

        let a = header(vec![param(10)], tcx.mk_adt(def(FOO), vec![ref_t, tuple_t]));
        let b = header(vec![], tcx.mk_adt(def(FOO), vec![ref_i32, tuple_i32]));
        let wrong_mutability = header(vec![], tcx.mk_adt(def(FOO), vec![mut_ref_i32, tuple_i32]));

        assert!(overlaps(&mut tcx, &a, &b));
        assert!(!overlaps(&mut tcx, &a, &wrong_mutability));
    }
}
