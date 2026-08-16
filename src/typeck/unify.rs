use std::cmp::Ordering;
use std::collections::HashMap;

use crate::nameres::PrimTy;
use crate::typeck::ty::{Ty, TyKind, TyVar};
use crate::typeck::tyctx::TyCtx;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UnifyError {
    Mismatch { expected: Ty, found: Ty },
    ExpectedInteger { var: Ty, found: Ty },
    ExpectedFloat { var: Ty, found: Ty },
    Infinite { var: Ty, ty: Ty },
}

#[derive(Default)]
pub struct Unifier {
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

    /// The representative of `ty`'s equivalence class, registering `ty` as its
    /// equivalence class if this is the first time it has been seen.
    pub fn root(&mut self, ty: Ty) -> Ty {
        if !self.parents.contains_key(&ty) {
            self.parents.insert(ty, ty);
            self.sizes.insert(ty, 1);
            return ty;
        }

        let mut current = ty;
        while let Some(&parent) = self.parents.get(&current) {
            if parent == current {
                break;
            }
            current = parent;
        }
        let root = current;

        let mut current = ty;
        while current != root {
            let parent = self.parents[&current];
            self.parents.insert(current, root);
            current = parent;
        }

        root
    }

    pub fn unify(&mut self, tcx: &TyCtx, expected: Ty, found: Ty) -> Result<(), UnifyError> {
        let t = self.root(expected);
        let u = self.root(found);

        if t == u {
            return Ok(());
        }

        // `Error` and `Never` can match against anything, but we record nothing
        if absorbs(tcx, t) || absorbs(tcx, u) {
            return Ok(());
        }

        let components = self.decompose(tcx, t, u)?;
        for (t_component, u_component) in components {
            self.unify(tcx, t_component, u_component)
                .map_err(|err| match err {
                    UnifyError::Mismatch { .. } => UnifyError::Mismatch {
                        expected: t,
                        found: u,
                    },
                    other => other,
                })?;
        }

        self.merge(tcx, t, u)?;
        Ok(())
    }

    /// Points one of `t`, `u` at the other, once the two are known to unify.
    /// `t` and `u` must both be roots.
    fn merge(&mut self, tcx: &TyCtx, t: Ty, u: Ty) -> Result<(), UnifyError> {
        let (root, child) = match constraint(tcx, t).cmp(&constraint(tcx, u)) {
            Ordering::Less => (u, t),
            Ordering::Greater => (t, u),
            Ordering::Equal if constraint(tcx, t) == Constraint::Concrete => return Ok(()),
            // Two variables that constrain equally
            // use the size heuristic (see optimizations for disjoint set unions).
            Ordering::Equal => {
                if self.sizes[&t] < self.sizes[&u] {
                    (u, t)
                } else {
                    (t, u)
                }
            }
        };

        // Make sure we don't create a type of infinite size
        if self.occurs(tcx, child, root) {
            return Err(UnifyError::Infinite {
                var: child,
                ty: root,
            });
        }

        debug_assert!(
            matches!(tcx.kind(child), TyKind::Var(_)),
            "only an inference variable may become a non-root member of a class, but \
             {:?} ({:?}) was pointed at {:?} ({:?}); merging a concrete type -- especially one \
             of the per-pass singletons `Unit`/`Never`/`Error` -- poisons it for the whole pass",
            child,
            tcx.kind(child),
            root,
            tcx.kind(root),
        );

        self.sizes
            .insert(root, self.sizes[&root] + self.sizes[&child]);
        self.parents.insert(child, root);
        Ok(())
    }

    fn occurs(&mut self, tcx: &TyCtx, var: Ty, ty: Ty) -> bool {
        let ty = self.root(ty);
        if ty == var {
            return true;
        }

        match tcx.kind(ty).clone() {
            TyKind::Adt { args, .. } | TyKind::Dyn { args, .. } | TyKind::Tuple(args) => {
                args.iter().any(|&arg| self.occurs(tcx, var, arg))
            }
            TyKind::Ref { base, .. } | TyKind::Any(base) => self.occurs(tcx, var, base),
            TyKind::Array { elem, .. } => self.occurs(tcx, var, elem),
            TyKind::Fun { params, ret } => {
                params.iter().any(|&param| self.occurs(tcx, var, param))
                    || ret.is_some_and(|ret| self.occurs(tcx, var, ret))
            }
            // Nothing nested to look inside. A `Var` that is not `var` itself cannot contain it.
            TyKind::Var(_)
            | TyKind::Primitive(_)
            | TyKind::Generic(_)
            | TyKind::SelfTy(_)
            | TyKind::Unit
            | TyKind::Never
            | TyKind::Error => false,
        }
    }

    /// Checks that `t` and `u` have the same immediate shape, and returns the component pairs
    /// that must themselves unify for the two to be the same type.
    /// `t` and `u` must both be roots.
    fn decompose(&self, tcx: &TyCtx, t: Ty, u: Ty) -> Result<Vec<(Ty, Ty)>, UnifyError> {
        debug_assert_eq!(self.parents.get(&t), Some(&t));
        debug_assert_eq!(self.parents.get(&u), Some(&u));

        // `Error` and `Never` are compatible with anything
        if absorbs(tcx, t) || absorbs(tcx, u) {
            return Ok(Vec::new());
        }

        let mismatch = || UnifyError::Mismatch {
            expected: t,
            found: u,
        };
        let no_components = Ok(Vec::new());

        match (tcx.kind(t), tcx.kind(u)) {
            // An `Any` variable takes on the whole of the other type, whatever its shape, so
            // there is nothing to recurse into: `merge` binds it below.
            (TyKind::Var(TyVar::Any(_)), _) | (_, TyKind::Var(TyVar::Any(_))) => no_components,

            (TyKind::Var(TyVar::Int(_)), TyKind::Var(TyVar::Int(_))) => no_components,
            (TyKind::Var(TyVar::Int(_)), TyKind::Primitive(p)) => {
                if is_integer(*p) {
                    no_components
                } else {
                    Err(UnifyError::ExpectedInteger { var: t, found: u })
                }
            }
            (TyKind::Primitive(p), TyKind::Var(TyVar::Int(_))) => {
                if is_integer(*p) {
                    no_components
                } else {
                    Err(UnifyError::ExpectedInteger { var: u, found: t })
                }
            }

            (TyKind::Var(TyVar::Float(_)), TyKind::Var(TyVar::Float(_))) => no_components,
            (TyKind::Var(TyVar::Float(_)), TyKind::Primitive(p)) => {
                if is_float(*p) {
                    no_components
                } else {
                    Err(UnifyError::ExpectedFloat { var: t, found: u })
                }
            }
            (TyKind::Primitive(p), TyKind::Var(TyVar::Float(_))) => {
                if is_float(*p) {
                    no_components
                } else {
                    Err(UnifyError::ExpectedFloat { var: u, found: t })
                }
            }

            (TyKind::Primitive(a), TyKind::Primitive(b)) => {
                if a == b {
                    no_components
                } else {
                    Err(mismatch())
                }
            }
            (TyKind::Generic(a), TyKind::Generic(b)) => {
                if a == b {
                    no_components
                } else {
                    Err(mismatch())
                }
            }
            (TyKind::SelfTy(a), TyKind::SelfTy(b)) => {
                if a == b {
                    no_components
                } else {
                    Err(mismatch())
                }
            }
            (TyKind::Unit, TyKind::Unit) => no_components,

            (TyKind::Adt { def: d1, args: a1 }, TyKind::Adt { def: d2, args: a2 }) => {
                if d1 == d2 && a1.len() == a2.len() {
                    Ok(zip(a1, a2))
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
                if m1 == m2 {
                    Ok(vec![(*b1, *b2)])
                } else {
                    Err(mismatch())
                }
            }

            (TyKind::Any(a), TyKind::Any(b)) => Ok(vec![(*a, *b)]),

            (TyKind::Tuple(a), TyKind::Tuple(b)) => {
                if a.len() == b.len() {
                    Ok(zip(a, b))
                } else {
                    Err(mismatch())
                }
            }

            (TyKind::Array { elem: e1, len: l1 }, TyKind::Array { elem: e2, len: l2 }) => {
                // TODO: const-checking for these
                if l1 == l2 {
                    Ok(vec![(*e1, *e2)])
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
            ) => match (r1, r2) {
                _ if p1.len() != p2.len() => Err(mismatch()),
                (Some(r1), Some(r2)) => {
                    let mut components = zip(p1, p2);
                    components.push((*r1, *r2));
                    Ok(components)
                }
                (None, None) => Ok(zip(p1, p2)),
                // One returns something and the other returns nothing: not the same type, and
                // there is no component pair to blame it on.
                (Some(_), None) | (None, Some(_)) => Err(mismatch()),
            },

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
                if t1 == t2 && a1.len() == a2.len() {
                    Ok(zip(a1, a2))
                } else {
                    Err(mismatch())
                }
            }

            _ => Err(mismatch()),
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Debug)]
enum Constraint {
    /// `_`: unifies with anything.
    AnyVar,
    /// `{integer}` or `{float}`: unifies only within its own family of primitives.
    NumericVar,
    /// Not a variable at all.
    Concrete,
}

fn constraint(tcx: &TyCtx, ty: Ty) -> Constraint {
    match tcx.kind(ty) {
        TyKind::Var(TyVar::Any(_)) => Constraint::AnyVar,
        TyKind::Var(TyVar::Int(_) | TyVar::Float(_)) => Constraint::NumericVar,
        _ => Constraint::Concrete,
    }
}

fn absorbs(tcx: &TyCtx, ty: Ty) -> bool {
    matches!(tcx.kind(ty), TyKind::Error | TyKind::Never)
}

fn zip(a: &[Ty], b: &[Ty]) -> Vec<(Ty, Ty)> {
    debug_assert_eq!(a.len(), b.len());
    a.iter().copied().zip(b.iter().copied()).collect()
}

pub(crate) fn is_integer(prim: PrimTy) -> bool {
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

pub(crate) fn is_float(prim: PrimTy) -> bool {
    matches!(prim, PrimTy::F32 | PrimTy::F64)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ast::Mutability;
    use crate::hir::{DefId, HirId};

    fn hir_id(n: u32) -> HirId {
        DefId::from_usize(n as usize).owner_id()
    }

    /// Runs `decompose` the way `unify` does: after driving both types to their union-find
    /// representatives. `decompose` asserts that precondition, so calling it directly on two
    /// freshly-interned types (as most cases below do) needs this instead.
    ///
    /// Only the immediate shape is answered here, which is all `decompose` decides. Anything
    /// that depends on a *component* -- whether `(i32,)` unifies with `(bool,)` -- is a question
    /// for `unify`, and the cases below that ask it go through `unify` directly.
    fn compatible(
        unifier: &mut Unifier,
        tcx: &TyCtx,
        expected: Ty,
        found: Ty,
    ) -> Result<(), UnifyError> {
        let expected = unifier.root(expected);
        let found = unifier.root(found);
        unifier.decompose(tcx, expected, found).map(|_| ())
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
        let mut tcx = TyCtx::new();
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
        let mut tcx = TyCtx::new();
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

    /// The shapes agree -- same `def`, same argument count -- so this is not something
    /// `decompose` can answer; it takes recursing into the argument to find `i32` against
    /// `bool`. The failure is still reported against the two `Adt`s the caller passed.
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
            u.unify(&tcx, a, b),
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

        // Mutability matches, so only recursing into the base type finds the mismatch.
        assert_eq!(
            u.unify(&tcx, a, b),
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

        // `any T` is the same shape whatever `T` is, so this is found by recursing into it.
        assert_eq!(
            u.unify(&tcx, a, b),
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

        // Same arity, so the elements have to be compared to find the mismatch.
        assert_eq!(
            u.unify(&tcx, a, b),
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

        // Both return *something*, so the return types have to be compared to tell them apart.
        assert_eq!(
            u.unify(&tcx, a, b),
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

    /// Union by size keeps a class shallow, so a large class still resolves through the loop in
    /// `root` without the chain ever getting deep. This is a correctness check on that loop over
    /// a class far larger than the handful the other tests build, not a stack-depth guard --
    /// there is no construction here that makes the tree tall.
    #[test]
    fn every_member_of_a_large_class_resolves_to_one_representative() {
        const MEMBERS: usize = 50_000;

        let mut tcx = TyCtx::new();
        let vars: Vec<Ty> = (0..MEMBERS).map(|_| tcx.next_ty_var()).collect();
        let i32_ty = tcx.mk_prim(PrimTy::I32);
        let mut u = Unifier::new();

        for pair in vars.windows(2) {
            assert_eq!(u.unify(&tcx, pair[0], pair[1]), Ok(()));
        }
        assert_eq!(u.unify(&tcx, vars[0], i32_ty), Ok(()));

        // The concrete type outranks every variable however large their class grew.
        for &var in &vars {
            assert_eq!(u.root(var), i32_ty);
        }
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

    /// An `{integer}` merged with an `_` has to stay the representative: `_` unifies with
    /// anything, so pointing the integer variable at it would drop the one thing the class knew.
    #[test]
    fn a_numeric_var_outranks_an_unconstrained_var() {
        let mut tcx = TyCtx::new();
        let any = tcx.next_ty_var();
        let int = tcx.next_int_var();
        let mut u = Unifier::new();

        assert_eq!(u.unify(&tcx, any, int), Ok(()));
        assert_eq!(u.root(any), int);

        // Same in the other order: which side it was passed on must not decide this.
        let mut tcx = TyCtx::new();
        let any = tcx.next_ty_var();
        let float = tcx.next_float_var();
        let mut u = Unifier::new();

        assert_eq!(u.unify(&tcx, float, any), Ok(()));
        assert_eq!(u.root(any), float);
    }

    /// The end-to-end shape of the same defect: `match .. { _ => 1, _ => true }` checks both arms
    /// against one type, and the first unification is `_` against `{integer}`. With the `_` kept
    /// as the representative, the second arm then unified `bool` against an unconstrained
    /// variable and the whole `match` was accepted.
    #[test]
    fn a_var_that_absorbed_an_int_var_still_rejects_bool() {
        let mut tcx = TyCtx::new();
        let result = tcx.next_ty_var();
        let int = tcx.next_int_var();
        let bool_ty = tcx.mk_prim(PrimTy::Bool);
        let mut u = Unifier::new();

        assert_eq!(u.unify(&tcx, result, int), Ok(()));
        assert_eq!(
            u.unify(&tcx, result, bool_ty),
            Err(UnifyError::ExpectedInteger {
                var: int,
                found: bool_ty
            })
        );
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

    // -----------------------------------------------------------------
    // unify: the occurs check
    // -----------------------------------------------------------------

    /// Without this the bind succeeds and the *structure* becomes cyclic: resolving the tuple
    /// resolves `?0`, which resolves back to the tuple. Anything walking a type's structure then
    /// runs forever, so the test would hang rather than fail.
    #[test]
    fn a_variable_cannot_be_bound_to_a_type_containing_it() {
        let mut tcx = TyCtx::new();
        let var = tcx.next_ty_var();
        let i32_ty = tcx.mk_prim(PrimTy::I32);
        let tuple = tcx.mk_tuple(vec![var, i32_ty]);
        let mut u = Unifier::new();

        assert_eq!(
            u.unify(&tcx, var, tuple),
            Err(UnifyError::Infinite { var, ty: tuple })
        );
        // The refused bind leaves the variable free.
        assert_eq!(u.root(var), var);
    }

    #[test]
    fn the_occurs_check_is_symmetric() {
        let mut tcx = TyCtx::new();
        let var = tcx.next_ty_var();
        let tuple = tcx.mk_tuple(vec![var]);
        let mut u = Unifier::new();

        assert!(matches!(
            u.unify(&tcx, tuple, var),
            Err(UnifyError::Infinite { .. })
        ));
    }

    /// The indirect case: neither bind contains a variable syntactically, but the second
    /// closes a loop through the first. `occurs` resolves as it descends, which is what sees it.
    #[test]
    fn a_cycle_closed_through_another_variable_is_caught() {
        let mut tcx = TyCtx::new();
        let a = tcx.next_ty_var();
        let b = tcx.next_ty_var();
        let tuple_b = tcx.mk_tuple(vec![b]);
        let tuple_a = tcx.mk_tuple(vec![a]);
        let mut u = Unifier::new();

        // `a = (b,)` is fine on its own.
        assert_eq!(u.unify(&tcx, a, tuple_b), Ok(()));
        // `b = (a,)` would make both infinite.
        assert!(matches!(
            u.unify(&tcx, b, tuple_a),
            Err(UnifyError::Infinite { .. })
        ));
    }

    /// The check must not reject an ordinary nested bind involving a *different* variable.
    #[test]
    fn a_variable_may_be_bound_to_a_type_containing_another_variable() {
        let mut tcx = TyCtx::new();
        let a = tcx.next_ty_var();
        let b = tcx.next_ty_var();
        let i32_ty = tcx.mk_prim(PrimTy::I32);
        let tuple = tcx.mk_tuple(vec![b, i32_ty]);
        let mut u = Unifier::new();

        assert_eq!(u.unify(&tcx, a, tuple), Ok(()));
        assert_eq!(u.root(a), tuple);
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

    // -----------------------------------------------------------------
    // unify: structural recursion into composites
    //
    // Interning makes handle equality mean type equality only for ground types. A composite
    // holding an unresolved variable is a different handle from the resolved composite, so
    // unifying the two takes recursing into the components -- which is what these cover.
    // -----------------------------------------------------------------

    #[test]
    fn a_tuple_holding_a_var_unifies_with_the_resolved_tuple() {
        // The shape `fun f() -> (i32, i32) { return (1, 2); }` produces: the literals are
        // integer variables, so the returned tuple is `({integer}, {integer})` against the
        // declared `(i32, i32)`.
        let mut tcx = TyCtx::new();
        let i32_ty = tcx.mk_prim(PrimTy::I32);
        let (v1, v2) = (tcx.next_int_var(), tcx.next_int_var());
        let expected = tcx.mk_tuple(vec![i32_ty, i32_ty]);
        let found = tcx.mk_tuple(vec![v1, v2]);
        let mut u = Unifier::new();

        assert_eq!(u.unify(&tcx, expected, found), Ok(()));
        // Unifying the tuples is what resolved the elements.
        assert_eq!(u.root(v1), i32_ty);
        assert_eq!(u.root(v2), i32_ty);
    }

    #[test]
    fn unification_recurses_through_every_composite_shape() {
        // One case per variant that holds a component, so a shape that stops recursing can't
        // slip through by being the only one left out.
        let mut tcx = TyCtx::new();
        let i32_ty = tcx.mk_prim(PrimTy::I32);
        let def = DefId::from_usize(0);
        let len = hir_id(0);

        let composites: Vec<(&str, Ty, Ty, Ty)> = vec![
            {
                let var = tcx.next_ty_var();
                (
                    "Adt",
                    tcx.mk_adt(def, vec![i32_ty]),
                    tcx.mk_adt(def, vec![var]),
                    var,
                )
            },
            {
                let var = tcx.next_ty_var();
                (
                    "Ref",
                    tcx.mk_ref(i32_ty, Mutability::Immutable),
                    tcx.mk_ref(var, Mutability::Immutable),
                    var,
                )
            },
            {
                let var = tcx.next_ty_var();
                ("Any", tcx.mk_any(i32_ty), tcx.mk_any(var), var)
            },
            {
                let var = tcx.next_ty_var();
                (
                    "Tuple",
                    tcx.mk_tuple(vec![i32_ty]),
                    tcx.mk_tuple(vec![var]),
                    var,
                )
            },
            {
                let var = tcx.next_ty_var();
                (
                    "Array",
                    tcx.mk_array(i32_ty, Some(len)),
                    tcx.mk_array(var, Some(len)),
                    var,
                )
            },
            {
                let var = tcx.next_ty_var();
                (
                    "Fun params",
                    tcx.mk_fun(vec![i32_ty], None),
                    tcx.mk_fun(vec![var], None),
                    var,
                )
            },
            {
                let var = tcx.next_ty_var();
                (
                    "Fun ret",
                    tcx.mk_fun(vec![], Some(i32_ty)),
                    tcx.mk_fun(vec![], Some(var)),
                    var,
                )
            },
            {
                let var = tcx.next_ty_var();
                (
                    "Dyn",
                    tcx.mk_dyn(def, vec![i32_ty]),
                    tcx.mk_dyn(def, vec![var]),
                    var,
                )
            },
        ];

        for (shape, expected, found, var) in composites {
            let mut u = Unifier::new();
            assert_eq!(u.unify(&tcx, expected, found), Ok(()), "{shape}");
            assert_eq!(u.root(var), i32_ty, "{shape} did not resolve its component");
        }
    }

    #[test]
    fn unification_recurses_more_than_one_level_deep() {
        let mut tcx = TyCtx::new();
        let i32_ty = tcx.mk_prim(PrimTy::I32);
        let var = tcx.next_ty_var();

        let inner_expected = tcx.mk_ref(i32_ty, Mutability::Immutable);
        let inner_found = tcx.mk_ref(var, Mutability::Immutable);
        let expected = tcx.mk_tuple(vec![inner_expected]);
        let found = tcx.mk_tuple(vec![inner_found]);
        let mut u = Unifier::new();

        assert_eq!(u.unify(&tcx, expected, found), Ok(()));
        assert_eq!(u.root(var), i32_ty);
    }

    #[test]
    fn a_mismatch_inside_a_composite_is_reported_against_the_outer_types() {
        // The caller asked about two tuples, so that is what the diagnostic should name -- not
        // the `i32`/`bool` pair the recursion happened to bottom out on.
        let mut tcx = TyCtx::new();
        let i32_ty = tcx.mk_prim(PrimTy::I32);
        let bool_ty = tcx.mk_prim(PrimTy::Bool);
        let expected = tcx.mk_tuple(vec![i32_ty, i32_ty]);
        let found = tcx.mk_tuple(vec![i32_ty, bool_ty]);
        let mut u = Unifier::new();

        assert_eq!(
            u.unify(&tcx, expected, found),
            Err(UnifyError::Mismatch { expected, found })
        );
    }

    #[test]
    fn a_var_kind_error_inside_a_composite_keeps_naming_the_variable() {
        // Unlike a plain mismatch, `ExpectedInteger` already says which variable went wrong, so
        // it is more useful than the outer types and is passed through as-is.
        let mut tcx = TyCtx::new();
        let bool_ty = tcx.mk_prim(PrimTy::Bool);
        let var = tcx.next_int_var();
        let expected = tcx.mk_tuple(vec![bool_ty]);
        let found = tcx.mk_tuple(vec![var]);
        let mut u = Unifier::new();

        assert_eq!(
            u.unify(&tcx, expected, found),
            Err(UnifyError::ExpectedInteger {
                var,
                found: bool_ty
            })
        );
    }

    #[test]
    fn two_concrete_composites_that_unify_are_not_merged_into_one_class() {
        // Nothing to record: they were already proven equal componentwise, and merging them
        // would make a concrete type a non-root member of a class.
        let mut tcx = TyCtx::new();
        let i32_ty = tcx.mk_prim(PrimTy::I32);
        let var = tcx.next_int_var();
        let expected = tcx.mk_tuple(vec![i32_ty]);
        let found = tcx.mk_tuple(vec![var]);
        let mut u = Unifier::new();

        assert_eq!(u.unify(&tcx, expected, found), Ok(()));
        assert_eq!(u.root(expected), expected);
        assert_eq!(u.root(found), found);
    }

    // -----------------------------------------------------------------
    // unify: Never/Error succeed without merging
    //
    // All three of `Never`, `Error` and `Unit` are interned once per pass, so a single merge
    // would make every later use of that singleton -- anywhere in the program -- resolve to
    // whatever it was merged with.
    // -----------------------------------------------------------------

    #[test]
    fn unifying_with_never_does_not_merge_the_two_classes() {
        let mut tcx = TyCtx::new();
        let bool_ty = tcx.mk_prim(PrimTy::Bool);
        let never = tcx.never();
        let mut u = Unifier::new();

        assert_eq!(u.unify(&tcx, never, bool_ty), Ok(()));
        assert_eq!(u.root(never), never, "`never` was folded into bool's class");
        assert_eq!(u.root(bool_ty), bool_ty);
    }

    #[test]
    fn unifying_with_error_does_not_merge_the_two_classes() {
        let mut tcx = TyCtx::new();
        let bool_ty = tcx.mk_prim(PrimTy::Bool);
        let error = tcx.error();
        let mut u = Unifier::new();

        assert_eq!(u.unify(&tcx, error, bool_ty), Ok(()));
        assert_eq!(u.root(error), error, "`error` was folded into bool's class");
        assert_eq!(u.root(bool_ty), bool_ty);
    }

    #[test]
    fn never_stays_neutral_across_unrelated_unifications() {
        // The shape of `fun f() { return true; }` followed by `fun g() { return 1; }` back when
        // a missing return type lowered to `Never`: unifying `Never` with `bool` for `f` used to
        // make `Never` *be* `bool`, so `g`'s integer literal was then checked against `bool`.
        let mut tcx = TyCtx::new();
        let bool_ty = tcx.mk_prim(PrimTy::Bool);
        let int_var = tcx.next_int_var();
        let never = tcx.never();
        let mut u = Unifier::new();

        assert_eq!(u.unify(&tcx, never, bool_ty), Ok(()));
        assert_eq!(
            u.unify(&tcx, never, int_var),
            Ok(()),
            "unifying `Never` with bool poisoned it for every later use"
        );
        assert_eq!(u.root(int_var), int_var, "the int var was bound to bool");
    }

    #[test]
    fn unit_is_never_folded_into_another_class() {
        // `Unit` is a per-pass singleton like `Never`, but unlike `Never` it does not absorb --
        // it is a real type that only unifies with itself and with a variable. A variable
        // unified with it must therefore point at `Unit`, not the other way round.
        let mut tcx = TyCtx::new();
        let unit = tcx.unit();
        let var = tcx.next_ty_var();
        let mut u = Unifier::new();

        assert_eq!(u.unify(&tcx, unit, var), Ok(()));
        assert_eq!(u.root(unit), unit);
        assert_eq!(u.root(var), unit);
    }

    #[test]
    fn unit_does_not_unify_with_an_unrelated_type() {
        let mut tcx = TyCtx::new();
        let unit = tcx.unit();
        let bool_ty = tcx.mk_prim(PrimTy::Bool);
        let mut u = Unifier::new();

        assert_eq!(
            u.unify(&tcx, unit, bool_ty),
            Err(UnifyError::Mismatch {
                expected: unit,
                found: bool_ty
            })
        );
    }
}
