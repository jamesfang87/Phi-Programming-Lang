//! Unification over [`Ty`] handles, implemented as a union-find so that two types once unified
//! stay merged no matter which order later queries visit them in.

use std::collections::HashMap;
use std::mem::swap;

use crate::nameres::results::PrimTy;
use crate::typeck::ty::{Ty, TyKind, TyVar};
use crate::typeck::tyctx::TyCtx;

/// Why [`Unifier::unify`] refused to merge two types, carried back instead of a bare `bool` so the
/// caller can turn it into a diagnostic without re-deriving what went wrong.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UnifyError {
    /// `expected` and `found` are incompatible outright: different shapes entirely (a tuple
    /// against a function type), or matching shapes whose immediate contents differ (two
    /// `Adt`s naming different definitions, two tuples of different arity, mismatched
    /// mutability on a `&`/`&mut`, and so on).
    Mismatch { expected: Ty, found: Ty },
    /// An integer-only inference variable (from an unsuffixed literal such as `1`) met a
    /// non-integer type.
    ExpectedInteger { var: Ty, found: Ty },
    /// A float-only inference variable (from an unsuffixed literal such as `1.0`) met a
    /// non-float type.
    ExpectedFloat { var: Ty, found: Ty },
}

/// Tracks which [`Ty`] handles the checker has decided must denote the same type.
///
/// Every method that needs to inspect a [`TyKind`] takes the owning [`TyCtx`] as a parameter
/// rather than [`Unifier`] holding a borrow of one. A [`Unifier`] is meant to live for the whole of a
/// type-checking pass (see [`Typeck`](crate::typeck::Typeck)), which also owns its [`TyCtx`] --
/// storing a borrow of one field inside a sibling field would make the containing struct
/// self-referential, which safe Rust can't express.
#[derive(Default)]
pub struct Unifier {
    /// Let an entry in parents be (Q, V). V is the representative type for Q
    /// and all Ty's which are equivalent to Q. For example, Q could be an
    /// unknown type. After unifying Q with i32 (for example), V, the
    /// representative Ty for all Ty's equivalent to Q would be i32.
    parents: HashMap<Ty, Ty>,

    /// [`Unifier::sizes`] is used SOLELY for optimization purposes (see optimizations
    /// for disjoint set unions). It has no other use.
    sizes: HashMap<Ty, u32>,
}

impl Unifier {
    pub fn new() -> Self {
        Unifier {
            parents: HashMap::new(),
            sizes: HashMap::new(),
        }
    }

    pub fn root(&mut self, ty: Ty) -> Ty {
        if let None = self.parents.get(&ty) {
            self.parents.insert(ty, ty);
            self.sizes.insert(ty, 1);
        }

        let parent = self.parents.get(&ty).unwrap();
        return if parent == &ty {
            ty
        } else {
            let ret = self.root(*parent);
            self.parents.insert(ty, ret);
            ret
        };
    }

    /// Attempts to unify `expected` and `found`, merging their equivalence classes if they are
    /// compatible.
    ///
    /// The two are named for how a failure reads: `expected` is the type the context demands and
    /// `found` is the type that turned up, which is the order [`UnifyError::Mismatch`] reports
    /// them in. Unification itself is symmetric.
    ///
    /// A failed unification leaves both classes untouched, so the caller is free to report the
    /// returned [`UnifyError`] without corrupting later unification queries.
    pub fn unify(&mut self, tcx: &TyCtx, expected: Ty, found: Ty) -> Result<(), UnifyError> {
        let mut t = self.root(expected);
        let mut u = self.root(found);

        if t == u {
            return Ok(());
        }

        // Before unifying, we must consider error cases which prevent
        // us from unifying
        self.compatible(tcx, t, u)?;

        // A concrete type is always kept as the representative over an inference variable, so
        // that once a variable is unified with something concrete, later lookups resolve
        // straight to that concrete type instead of bouncing through the variable. Between two
        // variables, or two concrete types, fall back to the size heuristic (see optimizations
        // for disjoint set unions).
        let t_is_var = matches!(tcx.kind(t), TyKind::Var(_));
        let u_is_var = matches!(tcx.kind(u), TyKind::Var(_));
        let swap_for_representative = match (t_is_var, u_is_var) {
            (true, false) => true,
            (false, true) => false,
            _ => self.sizes[&t] < self.sizes[&u],
        };
        if swap_for_representative {
            swap(&mut t, &mut u);
        }
        self.sizes.insert(t, self.sizes[&t] + self.sizes[&u]);
        self.parents.insert(u, t);
        Ok(())
    }

    /// [`Unifier::compatible`] returns whether two Ty's, t and u, can be unified, and if not, why.
    ///
    /// Note that t and u are assumed to already be the representatives of
    /// their components. That is, parents[t] == t and parents[u] == u.
    fn compatible(&self, tcx: &TyCtx, t: Ty, u: Ty) -> Result<(), UnifyError> {
        debug_assert_eq!(self.parents.get(&t), Some(&t));
        debug_assert_eq!(self.parents.get(&u), Some(&u));

        // `Error` and `Never` unify with anything: `Error` so one mistake doesn't cascade into
        // more diagnostics, `Never` because a `return`/`break`-typed expression coerces to
        // whatever the surrounding context expects.
        if matches!(tcx.kind(t), TyKind::Error | TyKind::Never)
            || matches!(tcx.kind(u), TyKind::Error | TyKind::Never)
        {
            return Ok(());
        }

        let mismatch = || UnifyError::Mismatch {
            expected: t,
            found: u,
        };

        match (tcx.kind(t), tcx.kind(u)) {
            (TyKind::Var(TyVar::Any(_)), _) | (_, TyKind::Var(TyVar::Any(_))) => Ok(()),

            (TyKind::Var(TyVar::Int(_)), TyKind::Var(TyVar::Int(_))) => Ok(()),
            (TyKind::Var(TyVar::Int(_)), TyKind::Primitive(p)) => {
                if is_integer(*p) {
                    Ok(())
                } else {
                    Err(UnifyError::ExpectedInteger { var: t, found: u })
                }
            }
            (TyKind::Primitive(p), TyKind::Var(TyVar::Int(_))) => {
                if is_integer(*p) {
                    Ok(())
                } else {
                    Err(UnifyError::ExpectedInteger { var: u, found: t })
                }
            }

            (TyKind::Var(TyVar::Float(_)), TyKind::Var(TyVar::Float(_))) => Ok(()),
            (TyKind::Var(TyVar::Float(_)), TyKind::Primitive(p)) => {
                if is_float(*p) {
                    Ok(())
                } else {
                    Err(UnifyError::ExpectedFloat { var: t, found: u })
                }
            }
            (TyKind::Primitive(p), TyKind::Var(TyVar::Float(_))) => {
                if is_float(*p) {
                    Ok(())
                } else {
                    Err(UnifyError::ExpectedFloat { var: u, found: t })
                }
            }

            (TyKind::Primitive(a), TyKind::Primitive(b)) => {
                if a == b {
                    Ok(())
                } else {
                    Err(mismatch())
                }
            }
            (TyKind::Generic(a), TyKind::Generic(b)) => {
                if a == b {
                    Ok(())
                } else {
                    Err(mismatch())
                }
            }
            (TyKind::SelfTy(a), TyKind::SelfTy(b)) => {
                if a == b {
                    Ok(())
                } else {
                    Err(mismatch())
                }
            }

            (TyKind::Adt { def: d1, args: a1 }, TyKind::Adt { def: d2, args: a2 }) => {
                if d1 == d2 && a1 == a2 {
                    Ok(())
                } else {
                    Err(mismatch())
                }
            }

            (
                TyKind::Ref {
                    base: b1,
                    mutability: m1,
                },
                TyKind::Ref {
                    base: b2,
                    mutability: m2,
                },
            ) => {
                if b1 == b2 && m1 == m2 {
                    Ok(())
                } else {
                    Err(mismatch())
                }
            }

            (TyKind::Any(a), TyKind::Any(b)) => {
                if a == b {
                    Ok(())
                } else {
                    Err(mismatch())
                }
            }
            (TyKind::Tuple(a), TyKind::Tuple(b)) => {
                if a == b {
                    Ok(())
                } else {
                    Err(mismatch())
                }
            }

            (TyKind::Array { elem: e1, len: l1 }, TyKind::Array { elem: e2, len: l2 }) => {
                if e1 == e2 && l1 == l2 {
                    Ok(())
                } else {
                    Err(mismatch())
                }
            }

            (
                TyKind::Fun {
                    params: p1,
                    ret: r1,
                },
                TyKind::Fun {
                    params: p2,
                    ret: r2,
                },
            ) => {
                if p1 == p2 && r1 == r2 {
                    Ok(())
                } else {
                    Err(mismatch())
                }
            }

            (
                TyKind::Dyn {
                    trait_: t1,
                    args: a1,
                },
                TyKind::Dyn {
                    trait_: t2,
                    args: a2,
                },
            ) => {
                if t1 == t2 && a1 == a2 {
                    Ok(())
                } else {
                    Err(mismatch())
                }
            }

            _ => Err(mismatch()),
        }
    }
}

fn is_integer(prim: PrimTy) -> bool {
    matches!(
        prim,
        PrimTy::I8
            | PrimTy::I16
            | PrimTy::I32
            | PrimTy::I64
            | PrimTy::U8
            | PrimTy::U16
            | PrimTy::U32
            | PrimTy::U64
    )
}

fn is_float(prim: PrimTy) -> bool {
    matches!(prim, PrimTy::F32 | PrimTy::F64)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ast::Mutability;
    use crate::hir::{DefId, HirId, LocalId};

    fn hir_id(n: u32) -> HirId {
        HirId {
            owner: DefId::from_usize(n as usize),
            local_id: LocalId::from_usize(n as usize),
        }
    }

    /// Runs `compatible` the way `unify` does: after driving both types to their union-find
    /// representatives. `compatible` asserts that precondition, so calling it directly on two
    /// freshly-interned types (as most cases below do) needs this instead.
    fn compatible(
        unifier: &mut Unifier,
        tcx: &TyCtx,
        expected: Ty,
        found: Ty,
    ) -> Result<(), UnifyError> {
        let expected = unifier.root(expected);
        let found = unifier.root(found);
        unifier.compatible(tcx, expected, found)
    }

    // -----------------------------------------------------------------
    // compatible: wildcards (Error, Never, Var::Any)
    // -----------------------------------------------------------------

    #[test]
    fn error_is_compatible_with_a_primitive() {
        let mut tcx = TyCtx::new();
        let i32_ty = tcx.mk_prim(PrimTy::I32);
        let error = tcx.error();
        let mut u = Unifier::new();

        assert_eq!(compatible(&mut u, &tcx, error, i32_ty), Ok(()));
        assert_eq!(compatible(&mut u, &tcx, i32_ty, error), Ok(()));
    }

    #[test]
    fn error_is_compatible_with_a_type_var() {
        let mut tcx = TyCtx::new();
        let var = tcx.next_int_var();
        let error = tcx.error();
        let mut u = Unifier::new();

        assert_eq!(compatible(&mut u, &tcx, error, var), Ok(()));
    }

    #[test]
    fn error_is_compatible_with_itself() {
        let tcx = TyCtx::new();
        let error = tcx.error();
        let mut u = Unifier::new();

        assert_eq!(compatible(&mut u, &tcx, error, error), Ok(()));
    }

    #[test]
    fn never_is_compatible_with_a_primitive() {
        let mut tcx = TyCtx::new();
        let bool_ty = tcx.mk_prim(PrimTy::Bool);
        let never = tcx.never();
        let mut u = Unifier::new();

        assert_eq!(compatible(&mut u, &tcx, never, bool_ty), Ok(()));
        assert_eq!(compatible(&mut u, &tcx, bool_ty, never), Ok(()));
    }

    #[test]
    fn never_is_compatible_with_error() {
        let tcx = TyCtx::new();
        let never = tcx.never();
        let error = tcx.error();
        let mut u = Unifier::new();

        assert_eq!(compatible(&mut u, &tcx, never, error), Ok(()));
    }

    #[test]
    fn any_var_is_compatible_with_a_primitive() {
        let mut tcx = TyCtx::new();
        let var = tcx.next_ty_var();
        let i32_ty = tcx.mk_prim(PrimTy::I32);
        let mut u = Unifier::new();

        assert_eq!(compatible(&mut u, &tcx, var, i32_ty), Ok(()));
        assert_eq!(compatible(&mut u, &tcx, i32_ty, var), Ok(()));
    }

    #[test]
    fn any_var_is_compatible_with_an_adt() {
        let mut tcx = TyCtx::new();
        let var = tcx.next_ty_var();
        let adt = tcx.mk_adt(DefId::from_usize(0), vec![]);
        let mut u = Unifier::new();

        assert_eq!(compatible(&mut u, &tcx, var, adt), Ok(()));
    }

    #[test]
    fn any_var_is_compatible_with_another_any_var() {
        let mut tcx = TyCtx::new();
        let a = tcx.next_ty_var();
        let b = tcx.next_ty_var();
        let mut u = Unifier::new();

        assert_eq!(compatible(&mut u, &tcx, a, b), Ok(()));
    }

    #[test]
    fn any_var_is_compatible_with_an_int_var() {
        let mut tcx = TyCtx::new();
        let any = tcx.next_ty_var();
        let int = tcx.next_int_var();
        let mut u = Unifier::new();

        assert_eq!(compatible(&mut u, &tcx, any, int), Ok(()));
        assert_eq!(compatible(&mut u, &tcx, int, any), Ok(()));
    }

    // -----------------------------------------------------------------
    // compatible: Var::Int
    // -----------------------------------------------------------------

    #[test]
    fn int_var_is_compatible_with_every_integer_primitive() {
        let integers = [
            PrimTy::I8,
            PrimTy::I16,
            PrimTy::I32,
            PrimTy::I64,
            PrimTy::U8,
            PrimTy::U16,
            PrimTy::U32,
            PrimTy::U64,
        ];
        for prim in integers {
            let mut tcx = TyCtx::new();
            let var = tcx.next_int_var();
            let prim_ty = tcx.mk_prim(prim);
            let mut u = Unifier::new();

            assert_eq!(compatible(&mut u, &tcx, var, prim_ty), Ok(()), "{prim:?}");
            assert_eq!(compatible(&mut u, &tcx, prim_ty, var), Ok(()), "{prim:?}");
        }
    }

    #[test]
    fn int_var_rejects_bool() {
        let mut tcx = TyCtx::new();
        let var = tcx.next_int_var();
        let bool_ty = tcx.mk_prim(PrimTy::Bool);
        let mut u = Unifier::new();

        assert_eq!(
            compatible(&mut u, &tcx, var, bool_ty),
            Err(UnifyError::ExpectedInteger {
                var,
                found: bool_ty
            })
        );
        assert_eq!(
            compatible(&mut u, &tcx, bool_ty, var),
            Err(UnifyError::ExpectedInteger {
                var,
                found: bool_ty
            })
        );
    }

    #[test]
    fn int_var_rejects_a_float_primitive() {
        let mut tcx = TyCtx::new();
        let var = tcx.next_int_var();
        let f64_ty = tcx.mk_prim(PrimTy::F64);
        let mut u = Unifier::new();

        assert_eq!(
            compatible(&mut u, &tcx, var, f64_ty),
            Err(UnifyError::ExpectedInteger { var, found: f64_ty })
        );
    }

    #[test]
    fn two_int_vars_are_compatible() {
        let mut tcx = TyCtx::new();
        let a = tcx.next_int_var();
        let b = tcx.next_int_var();
        let mut u = Unifier::new();

        assert_eq!(compatible(&mut u, &tcx, a, b), Ok(()));
    }

    // -----------------------------------------------------------------
    // compatible: Var::Float
    // -----------------------------------------------------------------

    #[test]
    fn float_var_is_compatible_with_every_float_primitive() {
        for prim in [PrimTy::F32, PrimTy::F64] {
            let mut tcx = TyCtx::new();
            let var = tcx.next_float_var();
            let prim_ty = tcx.mk_prim(prim);
            let mut u = Unifier::new();

            assert_eq!(compatible(&mut u, &tcx, var, prim_ty), Ok(()), "{prim:?}");
            assert_eq!(compatible(&mut u, &tcx, prim_ty, var), Ok(()), "{prim:?}");
        }
    }

    #[test]
    fn float_var_rejects_an_integer_primitive() {
        let mut tcx = TyCtx::new();
        let var = tcx.next_float_var();
        let i32_ty = tcx.mk_prim(PrimTy::I32);
        let mut u = Unifier::new();

        assert_eq!(
            compatible(&mut u, &tcx, var, i32_ty),
            Err(UnifyError::ExpectedFloat { var, found: i32_ty })
        );
        assert_eq!(
            compatible(&mut u, &tcx, i32_ty, var),
            Err(UnifyError::ExpectedFloat { var, found: i32_ty })
        );
    }

    #[test]
    fn two_float_vars_are_compatible() {
        let mut tcx = TyCtx::new();
        let a = tcx.next_float_var();
        let b = tcx.next_float_var();
        let mut u = Unifier::new();

        assert_eq!(compatible(&mut u, &tcx, a, b), Ok(()));
    }

    // -----------------------------------------------------------------
    // compatible: structural equality (Primitive, Generic, SelfParam, Adt, Ref, Any, Tuple,
    // Array, Fun, Dyn), and the cross-shape fallback
    // -----------------------------------------------------------------

    #[test]
    fn same_primitive_is_compatible() {
        let mut tcx = TyCtx::new();
        let a = tcx.mk_prim(PrimTy::I32);
        let b = tcx.mk_prim(PrimTy::I32);
        let mut u = Unifier::new();

        assert_eq!(compatible(&mut u, &tcx, a, b), Ok(()));
    }

    #[test]
    fn different_primitives_are_incompatible() {
        let mut tcx = TyCtx::new();
        let i32_ty = tcx.mk_prim(PrimTy::I32);
        let bool_ty = tcx.mk_prim(PrimTy::Bool);
        let mut u = Unifier::new();

        assert_eq!(
            compatible(&mut u, &tcx, i32_ty, bool_ty),
            Err(UnifyError::Mismatch {
                expected: i32_ty,
                found: bool_ty
            })
        );
    }

    #[test]
    fn same_generic_hir_id_is_compatible() {
        let mut tcx = TyCtx::new();
        let id = hir_id(0);
        let a = tcx.mk_generic(id);
        let b = tcx.mk_generic(id);
        let mut u = Unifier::new();

        assert_eq!(compatible(&mut u, &tcx, a, b), Ok(()));
    }

    #[test]
    fn different_generic_hir_ids_are_incompatible() {
        let mut tcx = TyCtx::new();
        let a = tcx.mk_generic(hir_id(0));
        let b = tcx.mk_generic(hir_id(1));
        let mut u = Unifier::new();

        assert_eq!(
            compatible(&mut u, &tcx, a, b),
            Err(UnifyError::Mismatch {
                expected: a,
                found: b
            })
        );
    }

    #[test]
    fn same_self_param_def_is_compatible() {
        let mut tcx = TyCtx::new();
        let trait_ = DefId::from_usize(0);
        let a = tcx.mk_self_param(trait_);
        let b = tcx.mk_self_param(trait_);
        let mut u = Unifier::new();

        assert_eq!(compatible(&mut u, &tcx, a, b), Ok(()));
    }

    #[test]
    fn different_self_param_defs_are_incompatible() {
        let mut tcx = TyCtx::new();
        let a = tcx.mk_self_param(DefId::from_usize(0));
        let b = tcx.mk_self_param(DefId::from_usize(1));
        let mut u = Unifier::new();

        assert_eq!(
            compatible(&mut u, &tcx, a, b),
            Err(UnifyError::Mismatch {
                expected: a,
                found: b
            })
        );
    }

    #[test]
    fn adt_with_same_def_and_args_is_compatible() {
        let mut tcx = TyCtx::new();
        let def = DefId::from_usize(0);
        let arg = tcx.mk_prim(PrimTy::I32);
        let a = tcx.mk_adt(def, vec![arg]);
        let b = tcx.mk_adt(def, vec![arg]);
        let mut u = Unifier::new();

        assert_eq!(compatible(&mut u, &tcx, a, b), Ok(()));
    }

    #[test]
    fn adt_with_same_def_and_different_args_is_incompatible() {
        let mut tcx = TyCtx::new();
        let def = DefId::from_usize(0);
        let arg1 = tcx.mk_prim(PrimTy::I32);
        let arg2 = tcx.mk_prim(PrimTy::Bool);
        let a = tcx.mk_adt(def, vec![arg1]);
        let b = tcx.mk_adt(def, vec![arg2]);
        let mut u = Unifier::new();

        assert_eq!(
            compatible(&mut u, &tcx, a, b),
            Err(UnifyError::Mismatch {
                expected: a,
                found: b
            })
        );
    }

    #[test]
    fn adt_with_different_defs_is_incompatible() {
        let mut tcx = TyCtx::new();
        let a = tcx.mk_adt(DefId::from_usize(0), vec![]);
        let b = tcx.mk_adt(DefId::from_usize(1), vec![]);
        let mut u = Unifier::new();

        assert_eq!(
            compatible(&mut u, &tcx, a, b),
            Err(UnifyError::Mismatch {
                expected: a,
                found: b
            })
        );
    }

    #[test]
    fn ref_with_same_base_and_mutability_is_compatible() {
        let mut tcx = TyCtx::new();
        let base = tcx.mk_prim(PrimTy::I32);
        let a = tcx.mk_ref(base, Mutability::Immutable);
        let b = tcx.mk_ref(base, Mutability::Immutable);
        let mut u = Unifier::new();

        assert_eq!(compatible(&mut u, &tcx, a, b), Ok(()));
    }

    #[test]
    fn ref_with_different_mutability_is_incompatible() {
        let mut tcx = TyCtx::new();
        let base = tcx.mk_prim(PrimTy::I32);
        let a = tcx.mk_ref(base, Mutability::Immutable);
        let b = tcx.mk_ref(base, Mutability::Mutable);
        let mut u = Unifier::new();

        assert_eq!(
            compatible(&mut u, &tcx, a, b),
            Err(UnifyError::Mismatch {
                expected: a,
                found: b
            })
        );
    }

    #[test]
    fn ref_with_different_base_is_incompatible() {
        let mut tcx = TyCtx::new();
        let i32_ty = tcx.mk_prim(PrimTy::I32);
        let bool_ty = tcx.mk_prim(PrimTy::Bool);
        let a = tcx.mk_ref(i32_ty, Mutability::Immutable);
        let b = tcx.mk_ref(bool_ty, Mutability::Immutable);
        let mut u = Unifier::new();

        assert_eq!(
            compatible(&mut u, &tcx, a, b),
            Err(UnifyError::Mismatch {
                expected: a,
                found: b
            })
        );
    }

    #[test]
    fn any_ty_with_same_inner_is_compatible() {
        let mut tcx = TyCtx::new();
        let inner = tcx.mk_prim(PrimTy::I32);
        let a = tcx.mk_any(inner);
        let b = tcx.mk_any(inner);
        let mut u = Unifier::new();

        assert_eq!(compatible(&mut u, &tcx, a, b), Ok(()));
    }

    #[test]
    fn any_ty_with_different_inner_is_incompatible() {
        let mut tcx = TyCtx::new();
        let i32_ty = tcx.mk_prim(PrimTy::I32);
        let bool_ty = tcx.mk_prim(PrimTy::Bool);
        let a = tcx.mk_any(i32_ty);
        let b = tcx.mk_any(bool_ty);
        let mut u = Unifier::new();

        assert_eq!(
            compatible(&mut u, &tcx, a, b),
            Err(UnifyError::Mismatch {
                expected: a,
                found: b
            })
        );
    }

    #[test]
    fn tuple_with_same_elems_is_compatible() {
        let mut tcx = TyCtx::new();
        let i32_ty = tcx.mk_prim(PrimTy::I32);
        let bool_ty = tcx.mk_prim(PrimTy::Bool);
        let a = tcx.mk_tuple(vec![i32_ty, bool_ty]);
        let b = tcx.mk_tuple(vec![i32_ty, bool_ty]);
        let mut u = Unifier::new();

        assert_eq!(compatible(&mut u, &tcx, a, b), Ok(()));
    }

    #[test]
    fn tuple_with_different_arity_is_incompatible() {
        let mut tcx = TyCtx::new();
        let i32_ty = tcx.mk_prim(PrimTy::I32);
        let a = tcx.mk_tuple(vec![i32_ty]);
        let b = tcx.mk_tuple(vec![i32_ty, i32_ty]);
        let mut u = Unifier::new();

        assert_eq!(
            compatible(&mut u, &tcx, a, b),
            Err(UnifyError::Mismatch {
                expected: a,
                found: b
            })
        );
    }

    #[test]
    fn tuple_with_same_arity_different_elem_is_incompatible() {
        let mut tcx = TyCtx::new();
        let i32_ty = tcx.mk_prim(PrimTy::I32);
        let bool_ty = tcx.mk_prim(PrimTy::Bool);
        let a = tcx.mk_tuple(vec![i32_ty]);
        let b = tcx.mk_tuple(vec![bool_ty]);
        let mut u = Unifier::new();

        assert_eq!(
            compatible(&mut u, &tcx, a, b),
            Err(UnifyError::Mismatch {
                expected: a,
                found: b
            })
        );
    }

    #[test]
    fn array_with_same_elem_and_len_is_compatible() {
        let mut tcx = TyCtx::new();
        let elem = tcx.mk_prim(PrimTy::I32);
        let len = hir_id(0);
        let a = tcx.mk_array(elem, Some(len));
        let b = tcx.mk_array(elem, Some(len));
        let mut u = Unifier::new();

        assert_eq!(compatible(&mut u, &tcx, a, b), Ok(()));
    }

    #[test]
    fn array_with_no_len_on_both_sides_is_compatible() {
        let mut tcx = TyCtx::new();
        let elem = tcx.mk_prim(PrimTy::I32);
        let a = tcx.mk_array(elem, None);
        let b = tcx.mk_array(elem, None);
        let mut u = Unifier::new();

        assert_eq!(compatible(&mut u, &tcx, a, b), Ok(()));
    }

    #[test]
    fn array_with_different_len_exprs_is_incompatible() {
        let mut tcx = TyCtx::new();
        let elem = tcx.mk_prim(PrimTy::I32);
        let a = tcx.mk_array(elem, Some(hir_id(0)));
        let b = tcx.mk_array(elem, Some(hir_id(1)));
        let mut u = Unifier::new();

        assert_eq!(
            compatible(&mut u, &tcx, a, b),
            Err(UnifyError::Mismatch {
                expected: a,
                found: b
            })
        );
    }

    #[test]
    fn fun_with_same_params_and_ret_is_compatible() {
        let mut tcx = TyCtx::new();
        let i32_ty = tcx.mk_prim(PrimTy::I32);
        let a = tcx.mk_fun(vec![i32_ty], Some(i32_ty));
        let b = tcx.mk_fun(vec![i32_ty], Some(i32_ty));
        let mut u = Unifier::new();

        assert_eq!(compatible(&mut u, &tcx, a, b), Ok(()));
    }

    #[test]
    fn fun_with_different_param_count_is_incompatible() {
        let mut tcx = TyCtx::new();
        let i32_ty = tcx.mk_prim(PrimTy::I32);
        let a = tcx.mk_fun(vec![i32_ty], None);
        let b = tcx.mk_fun(vec![i32_ty, i32_ty], None);
        let mut u = Unifier::new();

        assert_eq!(
            compatible(&mut u, &tcx, a, b),
            Err(UnifyError::Mismatch {
                expected: a,
                found: b
            })
        );
    }

    #[test]
    fn fun_with_different_ret_is_incompatible() {
        let mut tcx = TyCtx::new();
        let i32_ty = tcx.mk_prim(PrimTy::I32);
        let bool_ty = tcx.mk_prim(PrimTy::Bool);
        let a = tcx.mk_fun(vec![], Some(i32_ty));
        let b = tcx.mk_fun(vec![], Some(bool_ty));
        let mut u = Unifier::new();

        assert_eq!(
            compatible(&mut u, &tcx, a, b),
            Err(UnifyError::Mismatch {
                expected: a,
                found: b
            })
        );
    }

    #[test]
    fn fun_with_ret_present_vs_absent_is_incompatible() {
        let mut tcx = TyCtx::new();
        let i32_ty = tcx.mk_prim(PrimTy::I32);
        let a = tcx.mk_fun(vec![], Some(i32_ty));
        let b = tcx.mk_fun(vec![], None);
        let mut u = Unifier::new();

        assert_eq!(
            compatible(&mut u, &tcx, a, b),
            Err(UnifyError::Mismatch {
                expected: a,
                found: b
            })
        );
    }

    #[test]
    fn dyn_with_same_trait_and_args_is_compatible() {
        let mut tcx = TyCtx::new();
        let trait_ = DefId::from_usize(0);
        let arg = tcx.mk_prim(PrimTy::I32);
        let a = tcx.mk_dyn(trait_, vec![arg]);
        let b = tcx.mk_dyn(trait_, vec![arg]);
        let mut u = Unifier::new();

        assert_eq!(compatible(&mut u, &tcx, a, b), Ok(()));
    }

    #[test]
    fn dyn_with_different_trait_is_incompatible() {
        let mut tcx = TyCtx::new();
        let a = tcx.mk_dyn(DefId::from_usize(0), vec![]);
        let b = tcx.mk_dyn(DefId::from_usize(1), vec![]);
        let mut u = Unifier::new();

        assert_eq!(
            compatible(&mut u, &tcx, a, b),
            Err(UnifyError::Mismatch {
                expected: a,
                found: b
            })
        );
    }

    #[test]
    fn different_shapes_are_incompatible() {
        let mut tcx = TyCtx::new();
        let i32_ty = tcx.mk_prim(PrimTy::I32);
        let tuple = tcx.mk_tuple(vec![i32_ty]);
        let fun = tcx.mk_fun(vec![], None);
        let mut u = Unifier::new();

        assert_eq!(
            compatible(&mut u, &tcx, tuple, fun),
            Err(UnifyError::Mismatch {
                expected: tuple,
                found: fun
            })
        );
    }

    #[test]
    fn primitive_and_adt_are_incompatible() {
        let mut tcx = TyCtx::new();
        let prim = tcx.mk_prim(PrimTy::I32);
        let adt = tcx.mk_adt(DefId::from_usize(0), vec![]);
        let mut u = Unifier::new();

        assert_eq!(
            compatible(&mut u, &tcx, prim, adt),
            Err(UnifyError::Mismatch {
                expected: prim,
                found: adt
            })
        );
    }

    // -----------------------------------------------------------------
    // unify: end-to-end behavior and union-find bookkeeping
    // -----------------------------------------------------------------

    #[test]
    fn unifying_a_type_with_itself_succeeds() {
        let mut tcx = TyCtx::new();
        let i32_ty = tcx.mk_prim(PrimTy::I32);
        let mut u = Unifier::new();

        assert_eq!(u.unify(&tcx, i32_ty, i32_ty), Ok(()));
    }

    #[test]
    fn successful_unify_merges_the_two_classes() {
        let mut tcx = TyCtx::new();
        let var = tcx.next_ty_var();
        let i32_ty = tcx.mk_prim(PrimTy::I32);
        let mut u = Unifier::new();

        assert_eq!(u.unify(&tcx, var, i32_ty), Ok(()));
        assert_eq!(u.root(var), u.root(i32_ty));
    }

    #[test]
    fn unify_is_transitive_across_three_types() {
        let mut tcx = TyCtx::new();
        let a = tcx.next_ty_var();
        let b = tcx.next_ty_var();
        let c = tcx.mk_prim(PrimTy::I32);
        let mut u = Unifier::new();

        assert_eq!(u.unify(&tcx, a, b), Ok(()));
        assert_eq!(u.unify(&tcx, b, c), Ok(()));
        assert_eq!(u.root(a), u.root(c));
    }

    // -----------------------------------------------------------------
    // unify: a concrete type is preferred as the representative over a type variable
    // -----------------------------------------------------------------

    #[test]
    fn unifying_a_var_with_a_concrete_type_makes_the_concrete_type_the_representative() {
        let mut tcx = TyCtx::new();
        let var = tcx.next_ty_var();
        let i32_ty = tcx.mk_prim(PrimTy::I32);
        let mut u = Unifier::new();

        assert_eq!(u.unify(&tcx, var, i32_ty), Ok(()));
        assert_eq!(u.root(var), i32_ty);
    }

    #[test]
    fn unifying_a_concrete_type_with_a_var_makes_the_concrete_type_the_representative() {
        // Same as above with the arguments swapped: which side the concrete type is passed on
        // shouldn't matter.
        let mut tcx = TyCtx::new();
        let i32_ty = tcx.mk_prim(PrimTy::I32);
        let var = tcx.next_ty_var();
        let mut u = Unifier::new();

        assert_eq!(u.unify(&tcx, i32_ty, var), Ok(()));
        assert_eq!(u.root(var), i32_ty);
    }

    #[test]
    fn a_concrete_type_stays_the_representative_no_matter_how_many_vars_join_it() {
        // Regression guard for the size heuristic: without a concrete-over-variable
        // preference, a big enough pile of variables unified first could outweigh the
        // concrete type and make a variable the representative instead.
        let mut tcx = TyCtx::new();
        let i32_ty = tcx.mk_prim(PrimTy::I32);
        let vars: Vec<Ty> = (0..8).map(|_| tcx.next_ty_var()).collect();
        let mut u = Unifier::new();

        // Merge every variable into one class first, so its size heuristically dwarfs the
        // concrete type's size-of-one class.
        for pair in vars.windows(2) {
            assert_eq!(u.unify(&tcx, pair[0], pair[1]), Ok(()));
        }
        assert_eq!(u.unify(&tcx, vars[0], i32_ty), Ok(()));

        for &var in &vars {
            assert_eq!(u.root(var), i32_ty);
        }
    }

    #[test]
    fn a_var_merged_into_a_var_already_unified_with_a_concrete_type_resolves_to_it() {
        let mut tcx = TyCtx::new();
        let a = tcx.next_ty_var();
        let b = tcx.next_ty_var();
        let i32_ty = tcx.mk_prim(PrimTy::I32);
        let mut u = Unifier::new();

        assert_eq!(u.unify(&tcx, a, i32_ty), Ok(()));
        assert_eq!(u.unify(&tcx, a, b), Ok(()));
        assert_eq!(u.root(b), i32_ty);
    }

    #[test]
    fn two_vars_unify_by_size_when_neither_is_concrete() {
        // With no concrete type in play, the original size-based heuristic still governs
        // which variable becomes the representative: the larger class wins.
        let mut tcx = TyCtx::new();
        let a = tcx.next_ty_var();
        let b = tcx.next_ty_var();
        let c = tcx.next_ty_var();
        let mut u = Unifier::new();

        // `a` is folded into `b` first, so `b`'s class has size 2 by the time it meets `c`
        // (size 1), and should remain the representative.
        assert_eq!(u.unify(&tcx, a, b), Ok(()));
        let b_repr = u.root(b);
        assert_eq!(u.unify(&tcx, b, c), Ok(()));
        assert_eq!(u.root(c), b_repr);
    }

    #[test]
    fn failed_unify_leaves_both_classes_untouched() {
        let mut tcx = TyCtx::new();
        let i32_ty = tcx.mk_prim(PrimTy::I32);
        let bool_ty = tcx.mk_prim(PrimTy::Bool);
        let mut u = Unifier::new();

        assert_eq!(
            u.unify(&tcx, i32_ty, bool_ty),
            Err(UnifyError::Mismatch {
                expected: i32_ty,
                found: bool_ty
            })
        );
        // Each type remains its own representative; neither was folded into the other.
        assert_eq!(u.root(i32_ty), i32_ty);
        assert_eq!(u.root(bool_ty), bool_ty);
    }

    #[test]
    fn unify_can_be_called_twice_on_the_same_pair() {
        let mut tcx = TyCtx::new();
        let var = tcx.next_ty_var();
        let i32_ty = tcx.mk_prim(PrimTy::I32);
        let mut u = Unifier::new();

        assert_eq!(u.unify(&tcx, var, i32_ty), Ok(()));
        assert_eq!(u.unify(&tcx, var, i32_ty), Ok(()));
    }

    #[test]
    fn a_failed_unify_does_not_poison_later_unrelated_unifications() {
        let mut tcx = TyCtx::new();
        let i32_ty = tcx.mk_prim(PrimTy::I32);
        let bool_ty = tcx.mk_prim(PrimTy::Bool);
        let var = tcx.next_ty_var();
        let mut u = Unifier::new();

        assert!(u.unify(&tcx, i32_ty, bool_ty).is_err());
        assert_eq!(u.unify(&tcx, var, i32_ty), Ok(()));
    }
}
