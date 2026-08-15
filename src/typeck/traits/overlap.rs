//! [`overlaps`]: can two `extend` blocks both apply to one type?
//!
//! This is the primitive coherence is built out of, and it is the one place in trait solving that
//! needs *two-sided* unification. The query
//! ([`match_ty`](crate::typeck::traits::solve::match_ty)) matches an open impl header against a
//! closed goal, so only one side has variables. Here both sides are open --
//! `extend<T> Box<T> with Show` against `extend<U> Box<Vec<U>> with Show` -- and deciding whether
//! any type satisfies both means binding variables on both sides at once. That is Robinson
//! unification, and it is written here rather than borrowed from
//! [`Unifier`](crate::typeck::unify::Unifier) because the two work over different variable
//! spaces: `Unifier` binds *inference* variables in a union-find that lives for the whole pass,
//! and folding two impls' type parameters into it would leak bindings from a coherence question
//! into the checking of unrelated code.
//!
//! The two headers' parameters are renamed into one shared space by tagging each with the
//! [`Side`] it came from. Two blocks that both call their parameter `T` hold two distinct
//! [`HirId`]s already, so the tag is belt and braces -- but it also makes the function total: an
//! impl compared against a copy of itself still has its two `T`s kept apart instead of silently
//! identified.
//!
//! ## What this deliberately does not look at
//!
//! The bounds on either impl's generics. `extend<T: Show> Box<T> with Show` and
//! `extend Box<i32> with Show` overlap here, even though `i32` might well not implement `Show`
//! and so no type could ever pick the wrong one. Ruling that out means proving a *negative* --
//! that `i32: Show` can never hold -- which nothing in this design can answer. The diagnostic
//! coherence produces says so outright rather than leaving it a mystery.

use std::collections::HashMap;

use crate::hir::HirId;
use crate::typeck::traits::index::ExtendHeader;
use crate::typeck::ty::{Ty, TyKind};
use crate::typeck::tyctx::TyCtx;

/// Whether some type satisfies both headers at once.
///
/// Answers the self types first and the trait arguments after, in one shared substitution, so
/// that a parameter appearing in both positions has to be bound consistently:
/// `extend<T> Foo<T> with Conv<T>` and `extend Foo<i32> with Conv<bool>` do not overlap, because
/// `T` cannot be `i32` and `bool` at once.
///
/// The trait arguments are only compared when both headers name the *same* trait. Two impls of
/// different traits -- or an inherent block against a trait impl -- can still both apply to one
/// type, which is exactly the question coherence's duplicate-method-name check asks; see
/// [`coherence`](crate::typeck::traits::coherence).
pub fn overlaps(tcx: &TyCtx, a: &ExtendHeader, b: &ExtendHeader) -> bool {
    let mut overlap = Overlap {
        tcx,
        generics: [&a.generics, &b.generics],
        subst: HashMap::new(),
    };

    if !overlap.unify(Term::of(Side::A, a.self_ty), Term::of(Side::B, b.self_ty)) {
        return false;
    }

    match (&a.trait_ref, &b.trait_ref) {
        (Some(x), Some(y)) if x.def == y.def => {
            // An arity mismatch between two references to one trait means one of them is already
            // wrong, and a wrong impl is not something to report a conflict about on top of.
            if x.args.len() != y.args.len() {
                return false;
            }
            x.args
                .iter()
                .zip(y.args.iter())
                .all(|(&p, &q)| overlap.unify(Term::of(Side::A, p), Term::of(Side::B, q)))
        }
        _ => true,
    }
}

/// Which of the two headers a type came from. This is the renaming: a parameter is identified by
/// its [`HirId`] *and* the side it was written on, so the two spaces cannot collide.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
enum Side {
    A,
    B,
}

impl Side {
    fn index(self) -> usize {
        match self {
            Side::A => 0,
            Side::B => 1,
        }
    }
}

/// One variable of the shared space: a type parameter belonging to one of the two impls.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
struct Var {
    side: Side,
    param: HirId,
}

/// A type together with the variable space its [`TyKind::Generic`]s belong to.
///
/// This is what lets the substitution map a variable straight to a [`Ty`] without any new term
/// representation: binding `T` (side A) to `Vec<U>` (side B) stores the interned `Vec<U>` tagged
/// with side B, and every parameter inside it is still read against side B's parameter list.
/// Every subterm of a side-B type is itself side B, so the tag propagates for free as the walk
/// descends.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
struct Term {
    ty: Ty,
    side: Side,
}

impl Term {
    fn of(side: Side, ty: Ty) -> Self {
        Term { ty, side }
    }

    /// A component of this term, which lives in the same variable space as the term itself.
    fn nested(self, ty: Ty) -> Self {
        Term {
            ty,
            side: self.side,
        }
    }
}

struct Overlap<'a> {
    tcx: &'a TyCtx,

    /// Each side's own type parameters, indexed by [`Side::index`]. A [`TyKind::Generic`] not in
    /// its side's list belongs to some enclosing definition and is a rigid constant, not
    /// something to bind.
    generics: [&'a [HirId]; 2],

    subst: HashMap<Var, Term>,
}

impl Overlap<'_> {
    /// Whether `a` and `b` can be made equal, binding variables on either side as needed.
    fn unify(&mut self, a: Term, b: Term) -> bool {
        let (a, b) = (self.resolve(a), self.resolve(b));

        match (self.var_of(a), self.var_of(b)) {
            (Some(x), Some(y)) if x == y => true,
            (Some(x), _) => self.bind(x, b),
            (_, Some(y)) => self.bind(y, a),
            (None, None) => match self.decompose(a, b) {
                Some(components) => components
                    .into_iter()
                    .all(|(x, y)| self.unify(a.nested(x), b.nested(y))),
                None => false,
            },
        }
    }

    /// Binds `var` to `term`, unless that would make the type contain itself.
    ///
    /// The occurs check is not an optimization here. Unifying `T` against `Box<T>` without it
    /// stores a substitution whose resolution never terminates, so the *next* question asked
    /// about `T` hangs the compiler rather than merely answering wrongly.
    fn bind(&mut self, var: Var, term: Term) -> bool {
        if self.occurs(var, term) {
            return false;
        }
        self.subst.insert(var, term);
        true
    }

    /// Whether `var` appears anywhere inside `term`, following the substitution as it descends so
    /// that a cycle closed through a second variable is caught too.
    fn occurs(&self, var: Var, term: Term) -> bool {
        let term = self.resolve(term);
        if self.var_of(term) == Some(var) {
            return true;
        }

        self.components(term.ty)
            .into_iter()
            .any(|component| self.occurs(var, term.nested(component)))
    }

    /// Follows `term` through the substitution until it is either not a variable or not bound.
    fn resolve(&self, mut term: Term) -> Term {
        while let Some(var) = self.var_of(term) {
            match self.subst.get(&var) {
                Some(&bound) => term = bound,
                None => break,
            }
        }
        term
    }

    /// The variable `term` is, if it is one of its own side's parameters at all.
    fn var_of(&self, term: Term) -> Option<Var> {
        let TyKind::Generic(param) = *self.tcx.kind(term.ty) else {
            return None;
        };
        self.generics[term.side.index()]
            .contains(&param)
            .then_some(Var {
                side: term.side,
                param,
            })
    }

    /// Checks that two non-variable types have the same immediate shape, and returns the
    /// component pairs that must themselves unify. Mirrors
    /// [`Unifier`](crate::typeck::unify::Unifier)'s own `decompose`, minus everything about
    /// inference variables, which cannot appear in a header.
    fn decompose(&self, a: Term, b: Term) -> Option<Vec<(Ty, Ty)>> {
        let no_components = Some(Vec::new());

        match (self.tcx.kind(a.ty), self.tcx.kind(b.ty)) {
            // A header that failed to lower has already been reported. Answering "these do not
            // overlap" keeps that one mistake from producing a conflict diagnostic on top of it;
            // an absorbing `true` would invent a conflict with every other impl of the type.
            (TyKind::Error, _) | (_, TyKind::Error) => None,

            (TyKind::Primitive(p), TyKind::Primitive(q)) => (p == q).then(Vec::new),
            // Neither is one of *its own* side's parameters, or `unify` would have bound it. Two
            // rigid parameters name the same constant only when they are literally the same
            // declaration -- which the two sides can share, when both name a parameter of some
            // definition enclosing them both.
            (TyKind::Generic(p), TyKind::Generic(q)) => (p == q).then(Vec::new),
            (TyKind::SelfTy(p), TyKind::SelfTy(q)) => (p == q).then(Vec::new),
            (TyKind::Unit, TyKind::Unit) | (TyKind::Never, TyKind::Never) => no_components,

            (TyKind::Adt { def: d, args: x }, TyKind::Adt { def: e, args: y })
            | (TyKind::Dyn { trait_: d, args: x }, TyKind::Dyn { trait_: e, args: y }) => {
                (d == e && x.len() == y.len()).then(|| zip(x, y))
            }

            (
                TyKind::Ref {
                    base: x,
                    mutability: m,
                },
                TyKind::Ref {
                    base: y,
                    mutability: n,
                },
            ) => (m == n).then(|| vec![(*x, *y)]),

            (TyKind::Any(x), TyKind::Any(y)) => Some(vec![(*x, *y)]),

            (TyKind::Tuple(x), TyKind::Tuple(y)) => (x.len() == y.len()).then(|| zip(x, y)),

            (TyKind::Array { elem: x, len: m }, TyKind::Array { elem: y, len: n }) => {
                // `len` addresses an unevaluated constant expression, so two lengths agree only
                // when they are the same expression; see `TyKind::Array`.
                (m == n).then(|| vec![(*x, *y)])
            }

            (
                TyKind::Fun {
                    params: x,
                    ret: r_x,
                },
                TyKind::Fun {
                    params: y,
                    ret: r_y,
                },
            ) => {
                if x.len() != y.len() {
                    return None;
                }
                let mut components = zip(x, y);
                match (r_x, r_y) {
                    (Some(r_x), Some(r_y)) => components.push((*r_x, *r_y)),
                    (None, None) => {}
                    (Some(_), None) | (None, Some(_)) => return None,
                }
                Some(components)
            }

            (TyKind::Var(_), _) | (_, TyKind::Var(_)) => unreachable!(
                "an impl header is lowered from written annotations, which never produce an \
                 inference variable, so coherence can never be asked about one"
            ),

            _ => None,
        }
    }

    /// The immediate components of `ty`, for the occurs check to descend through.
    fn components(&self, ty: Ty) -> Vec<Ty> {
        match self.tcx.kind(ty) {
            TyKind::Adt { args, .. } | TyKind::Dyn { args, .. } | TyKind::Tuple(args) => {
                args.clone()
            }
            TyKind::Ref { base, .. } | TyKind::Any(base) => vec![*base],
            TyKind::Array { elem, .. } => vec![*elem],
            TyKind::Fun { params, ret } => {
                let mut components = params.clone();
                components.extend(ret);
                components
            }
            // Nothing nested to look inside.
            TyKind::Var(_)
            | TyKind::Primitive(_)
            | TyKind::Generic(_)
            | TyKind::SelfTy(_)
            | TyKind::Unit
            | TyKind::Never
            | TyKind::Error => Vec::new(),
        }
    }
}

/// Pairs two equal-length component lists up positionally.
fn zip(a: &[Ty], b: &[Ty]) -> Vec<(Ty, Ty)> {
    debug_assert_eq!(a.len(), b.len());
    a.iter().copied().zip(b.iter().copied()).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ast::Mutability;
    use crate::driver::source::SrcSpan;
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

    /// A header with no trait and no methods: enough for every question `overlaps` asks about
    /// self types alone.
    fn header(generics: Vec<HirId>, self_ty: Ty) -> ExtendHeader {
        ExtendHeader {
            def: def(900),
            generics,
            self_ty,
            trait_ref: None,
            methods: HashMap::new(),
            span: SrcSpan::new(0, 0),
        }
    }

    fn with_trait(mut header: ExtendHeader, trait_def: DefId, args: Vec<Ty>) -> ExtendHeader {
        header.trait_ref = Some(TraitRef {
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

        assert!(overlaps(&tcx, &a, &b));
        assert!(overlaps(&tcx, &b, &a), "overlap is symmetric");
    }

    #[test]
    fn two_impls_of_different_types_do_not_overlap() {
        let mut tcx = TyCtx::new();
        let a = header(vec![], tcx.mk_adt(def(FOO), vec![]));
        let b = header(vec![], tcx.mk_adt(def(BAR), vec![]));

        assert!(!overlaps(&tcx, &a, &b));
    }

    #[test]
    fn two_impls_with_disjoint_concrete_arguments_do_not_overlap() {
        let mut tcx = TyCtx::new();
        let (i32_ty, bool_ty) = (tcx.mk_prim(PrimTy::I32), tcx.mk_prim(PrimTy::Bool));
        let a = header(vec![], tcx.mk_adt(def(FOO), vec![i32_ty]));
        let b = header(vec![], tcx.mk_adt(def(FOO), vec![bool_ty]));

        assert!(!overlaps(&tcx, &a, &b));
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

        assert!(overlaps(&tcx, &a, &b));
    }

    #[test]
    fn two_partly_concrete_impls_do_not_overlap_when_a_position_disagrees() {
        let mut tcx = TyCtx::new();
        let (i32_ty, bool_ty) = (tcx.mk_prim(PrimTy::I32), tcx.mk_prim(PrimTy::Bool));
        let (t, u) = (tcx.mk_generic(param(10)), tcx.mk_generic(param(11)));
        let a = header(vec![param(11)], tcx.mk_adt(def(FOO), vec![i32_ty, u]));
        let b = header(vec![param(10)], tcx.mk_adt(def(FOO), vec![bool_ty, t]));

        assert!(!overlaps(&tcx, &a, &b));
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

        assert!(overlaps(&tcx, &a, &consistent));
        assert!(!overlaps(&tcx, &a, &inconsistent));
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

        assert!(!overlaps(&tcx, &a, &concrete));
        assert!(overlaps(&tcx, &a, &same_rigid));
    }

    /// Both sides may name the same parameter and still be two different variables. This is what
    /// an impl compared against a copy of itself looks like, and the two bindings have to be
    /// tracked apart for the comparison to mean anything.
    #[test]
    fn the_two_sides_parameters_are_renamed_apart() {
        let mut tcx = TyCtx::new();
        let t = tcx.mk_generic(param(10));
        let i32_ty = tcx.mk_prim(PrimTy::I32);
        let a = header(vec![param(10)], tcx.mk_adt(def(FOO), vec![t, i32_ty]));
        let b = header(vec![param(10)], tcx.mk_adt(def(FOO), vec![i32_ty, t]));

        assert!(overlaps(&tcx, &a, &b));

        let mut overlap = Overlap {
            tcx: &tcx,
            generics: [&a.generics, &b.generics],
            subst: HashMap::new(),
        };
        assert!(overlap.unify(Term::of(Side::A, a.self_ty), Term::of(Side::B, b.self_ty)));
        assert_eq!(
            overlap.subst.len(),
            2,
            "one binding per side, not one shared binding"
        );
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

        assert!(!overlaps(&tcx, &a, &b));
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

        assert!(overlaps(&tcx, &a, &matching));
        assert!(overlaps(&tcx, &a, &deeper), "`T` binds to either");
        assert!(!overlaps(&tcx, &matching, &deeper));
    }

    #[test]
    fn impls_of_the_same_trait_with_disjoint_arguments_do_not_overlap() {
        let mut tcx = TyCtx::new();
        let (i32_ty, bool_ty) = (tcx.mk_prim(PrimTy::I32), tcx.mk_prim(PrimTy::Bool));
        let foo = tcx.mk_adt(def(FOO), vec![]);
        let a = with_trait(header(vec![], foo), def(SHOW), vec![i32_ty]);
        let b = with_trait(header(vec![], foo), def(SHOW), vec![bool_ty]);

        assert!(!overlaps(&tcx, &a, &b));
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

        assert!(overlaps(&tcx, &a, &agrees));
        assert!(!overlaps(&tcx, &a, &disagrees));
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

        assert!(overlaps(&tcx, &a, &b));
        assert!(overlaps(&tcx, &a, &inherent));
    }

    /// A header that failed to lower is not a conflict with everything in sight.
    #[test]
    fn an_error_type_never_overlaps() {
        let mut tcx = TyCtx::new();
        let error = tcx.error();
        let i32_ty = tcx.mk_prim(PrimTy::I32);
        let a = header(vec![], tcx.mk_adt(def(FOO), vec![error]));
        let b = header(vec![], tcx.mk_adt(def(FOO), vec![i32_ty]));

        assert!(!overlaps(&tcx, &a, &b));
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

        assert!(overlaps(&tcx, &a, &b));
        assert!(!overlaps(&tcx, &a, &wrong_mutability));
    }
}
