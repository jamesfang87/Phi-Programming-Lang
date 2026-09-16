use std::cmp::Ordering;
use std::collections::HashMap;
use std::collections::hash_map::Entry;
use std::ops::ControlFlow;

use crate::typeck::ty::ctx::TyCtx;
use crate::typeck::ty::visitor::{self, TypeVisitor};
use crate::typeck::ty::{InferVar, Ty, TyKind};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UnifyError {
    Mismatch { expected: Ty, found: Ty },
    ExpectedInteger { var: Ty, found: Ty },
    ExpectedFloat { var: Ty, found: Ty },
    Infinite { var: Ty, found: Ty },
}

#[derive(Default)]
pub struct Unifier {
    parents: HashMap<Ty, Ty>,
    // Sized for the union-by-size heuristic in `merge`; it has no effect on unification's result.
    sizes: HashMap<Ty, u32>,
}

impl Unifier {
    pub fn new() -> Self {
        Unifier {
            parents: HashMap::new(),
            sizes: HashMap::new(),
        }
    }

    pub fn find_deep(&mut self, tcx: &mut TyCtx, ty: Ty) -> Ty {
        visitor::fold_ty(tcx, ty, &mut |tcx, ty| {
            let root = self.find_shallow(ty);
            (root != ty).then(|| self.find_deep(tcx, root))
        })
    }

    fn find_shallow(&mut self, ty: Ty) -> Ty {
        if let Entry::Vacant(e) = self.parents.entry(ty) {
            e.insert(ty);
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
        let t = self.find_shallow(expected);
        let u = self.find_shallow(found);

        if t == u {
            return Ok(());
        }

        if is_absorbing(tcx, t) || is_absorbing(tcx, u) {
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

    fn merge(&mut self, tcx: &TyCtx, t: Ty, u: Ty) -> Result<(), UnifyError> {
        let (root, child) = match constraint(tcx, t).cmp(&constraint(tcx, u)) {
            Ordering::Less => (u, t),
            Ordering::Greater => (t, u),
            Ordering::Equal if constraint(tcx, t) == Constraint::Concrete => return Ok(()),
            Ordering::Equal => {
                if self.sizes[&t] < self.sizes[&u] {
                    (u, t)
                } else {
                    (t, u)
                }
            }
        };

        if self.occurs(tcx, child, root) {
            return Err(UnifyError::Infinite {
                var: child,
                found: root,
            });
        }

        debug_assert!(
            matches!(tcx.kind(child), TyKind::Var(_)),
            "only an inference variable may become a non-root member of a class, but \
             {:?} ({:?}) was pointed at {:?} ({:?}). Merging a concrete type, especially one \
             of the per-pass singletons `Unit`/`Never`/`Error`, poisons it for the whole pass",
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
        visitor::walk(&mut Occurs { unifier: self, var }, tcx, ty).is_break()
    }

    /// Returns the component pairs of two root types that unification has to unify, or the reason
    /// they cannot be.
    fn decompose(&self, tcx: &TyCtx, t: Ty, u: Ty) -> Result<Vec<(Ty, Ty)>, UnifyError> {
        // Inference variables are handled before the structural decomposition: an `any` matches
        // anything without components, and an integer or float variable only decomposes against a
        // primitive of the matching kind.
        debug_assert_eq!(self.parents.get(&t), Some(&t));
        debug_assert_eq!(self.parents.get(&u), Some(&u));

        let no_components = Ok(Vec::new());

        match (tcx.kind(t), tcx.kind(u)) {
            (TyKind::Var(InferVar::Any(_)), _) | (_, TyKind::Var(InferVar::Any(_))) => {
                no_components
            }
            (TyKind::Var(InferVar::Int(_)), TyKind::Var(InferVar::Int(_))) => no_components,
            (TyKind::Var(InferVar::Int(_)), TyKind::Primitive(p)) => {
                if p.is_integer() {
                    no_components
                } else {
                    Err(UnifyError::ExpectedInteger { var: t, found: u })
                }
            }
            (TyKind::Primitive(p), TyKind::Var(InferVar::Int(_))) => {
                if p.is_integer() {
                    no_components
                } else {
                    Err(UnifyError::ExpectedInteger { var: u, found: t })
                }
            }

            (TyKind::Var(InferVar::Float(_)), TyKind::Var(InferVar::Float(_))) => no_components,
            (TyKind::Var(InferVar::Float(_)), TyKind::Primitive(p)) => {
                if p.is_float() {
                    no_components
                } else {
                    Err(UnifyError::ExpectedFloat { var: t, found: u })
                }
            }
            (TyKind::Primitive(p), TyKind::Var(InferVar::Float(_))) => {
                if p.is_float() {
                    no_components
                } else {
                    Err(UnifyError::ExpectedFloat { var: u, found: t })
                }
            }

            _ => visitor::decompose(tcx, t, u).ok_or(UnifyError::Mismatch {
                expected: t,
                found: u,
            }),
        }
    }
}

struct Occurs<'a> {
    unifier: &'a mut Unifier,
    var: Ty,
}

impl TypeVisitor for Occurs<'_> {
    type Output = ();

    fn visit(&mut self, _tcx: &TyCtx, ty: Ty) -> ControlFlow<()> {
        if self.unifier.find_shallow(ty) == self.var {
            ControlFlow::Break(())
        } else {
            ControlFlow::Continue(())
        }
    }

    fn children(&mut self, tcx: &TyCtx, ty: Ty) -> Vec<Ty> {
        visitor::children(tcx, self.unifier.find_shallow(ty))
    }
}

#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Debug)]
enum Constraint {
    /// `_`: unifies with anything.
    Any,
    /// can only unify with either `{integer}` or `{float}`
    Numeric,
    /// Must only unify with itself
    Concrete,
}

fn constraint(tcx: &TyCtx, ty: Ty) -> Constraint {
    match tcx.kind(ty) {
        TyKind::Var(InferVar::Any(_)) => Constraint::Any,
        TyKind::Var(InferVar::Int(_) | InferVar::Float(_)) => Constraint::Numeric,
        _ => Constraint::Concrete,
    }
}

fn is_absorbing(tcx: &TyCtx, ty: Ty) -> bool {
    matches!(tcx.kind(ty), TyKind::Error | TyKind::Never)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ast::Mutability;
    use crate::hir::{DefId, HirId};
    use crate::typeck::PrimTy;

    fn hir_id(n: u32) -> HirId {
        DefId::from_usize(n as usize).owner_id()
    }

    fn compatible(
        unifier: &mut Unifier,
        tcx: &TyCtx,
        expected: Ty,
        found: Ty,
    ) -> Result<(), UnifyError> {
        let expected = unifier.find_shallow(expected);
        let found = unifier.find_shallow(found);
        if is_absorbing(tcx, expected) || is_absorbing(tcx, found) {
            return Ok(());
        }
        unifier.decompose(tcx, expected, found).map(|_| ())
    }

    #[test]
    fn find_deep_replaces_a_variable_nested_inside_a_type() {
        let mut tcx = TyCtx::new();
        let var = tcx.next_infer_var();
        let i32_ty = tcx.mk_prim(PrimTy::I32);
        let open = tcx.mk_tuple(vec![var, i32_ty]);
        let closed = tcx.mk_tuple(vec![i32_ty, i32_ty]);
        let mut unifier = Unifier::new();

        assert_eq!(unifier.unify(&tcx, var, i32_ty), Ok(()));
        assert_eq!(unifier.find_shallow(open), open);
        assert_eq!(unifier.find_deep(&mut tcx, open), closed);
    }

    #[test]
    fn find_deep_leaves_a_variable_nothing_has_unified_with_as_itself() {
        let mut tcx = TyCtx::new();
        let var = tcx.next_infer_var();
        let open = tcx.mk_tuple(vec![var]);
        let mut unifier = Unifier::new();

        assert_eq!(unifier.find_deep(&mut tcx, var), var);
        assert_eq!(unifier.find_deep(&mut tcx, open), open);
    }

    #[test]
    fn find_deep_resolves_through_every_layer_a_type_can_nest() {
        let mut tcx = TyCtx::new();
        let var = tcx.next_infer_var();
        let i32_ty = tcx.mk_prim(PrimTy::I32);
        let open = {
            let inner = tcx.mk_ref(var, Mutability::Immutable);
            let array = tcx.mk_array(inner, None);
            tcx.mk_fun(vec![array], Some(var))
        };
        let closed = {
            let inner = tcx.mk_ref(i32_ty, Mutability::Immutable);
            let array = tcx.mk_array(inner, None);
            tcx.mk_fun(vec![array], Some(i32_ty))
        };
        let mut unifier = Unifier::new();

        assert_eq!(unifier.unify(&tcx, var, i32_ty), Ok(()));
        assert_eq!(unifier.find_deep(&mut tcx, open), closed);
    }

    #[test]
    fn error_is_compatible_with_a_primitive() {
        let mut tcx = TyCtx::new();
        let i32_ty = tcx.mk_prim(PrimTy::I32);
        let error = tcx.error();
        let mut unifier = Unifier::new();

        assert_eq!(compatible(&mut unifier, &tcx, error, i32_ty), Ok(()));
        assert_eq!(compatible(&mut unifier, &tcx, i32_ty, error), Ok(()));
    }

    #[test]
    fn error_is_compatible_with_a_type_var() {
        let mut tcx = TyCtx::new();
        let var = tcx.next_int_var();
        let error = tcx.error();
        let mut unifier = Unifier::new();

        assert_eq!(compatible(&mut unifier, &tcx, error, var), Ok(()));
    }

    #[test]
    fn error_is_compatible_with_itself() {
        let mut tcx = TyCtx::new();
        let error = tcx.error();
        let mut unifier = Unifier::new();

        assert_eq!(compatible(&mut unifier, &tcx, error, error), Ok(()));
    }

    #[test]
    fn never_is_compatible_with_a_primitive() {
        let mut tcx = TyCtx::new();
        let bool_ty = tcx.mk_prim(PrimTy::Bool);
        let never = tcx.never();
        let mut unifier = Unifier::new();

        assert_eq!(compatible(&mut unifier, &tcx, never, bool_ty), Ok(()));
        assert_eq!(compatible(&mut unifier, &tcx, bool_ty, never), Ok(()));
    }

    #[test]
    fn never_is_compatible_with_error() {
        let mut tcx = TyCtx::new();
        let never = tcx.never();
        let error = tcx.error();
        let mut unifier = Unifier::new();

        assert_eq!(compatible(&mut unifier, &tcx, never, error), Ok(()));
    }

    #[test]
    fn any_var_is_compatible_with_a_primitive() {
        let mut tcx = TyCtx::new();
        let var = tcx.next_infer_var();
        let i32_ty = tcx.mk_prim(PrimTy::I32);
        let mut unifier = Unifier::new();

        assert_eq!(compatible(&mut unifier, &tcx, var, i32_ty), Ok(()));
        assert_eq!(compatible(&mut unifier, &tcx, i32_ty, var), Ok(()));
    }

    #[test]
    fn any_var_is_compatible_with_an_adt() {
        let mut tcx = TyCtx::new();
        let var = tcx.next_infer_var();
        let adt = tcx.mk_adt(DefId::from_usize(0), vec![]);
        let mut unifier = Unifier::new();

        assert_eq!(compatible(&mut unifier, &tcx, var, adt), Ok(()));
    }

    #[test]
    fn any_var_is_compatible_with_another_any_var() {
        let mut tcx = TyCtx::new();
        let a = tcx.next_infer_var();
        let b = tcx.next_infer_var();
        let mut unifier = Unifier::new();

        assert_eq!(compatible(&mut unifier, &tcx, a, b), Ok(()));
    }

    #[test]
    fn any_var_is_compatible_with_an_int_var() {
        let mut tcx = TyCtx::new();
        let any = tcx.next_infer_var();
        let int = tcx.next_int_var();
        let mut unifier = Unifier::new();

        assert_eq!(compatible(&mut unifier, &tcx, any, int), Ok(()));
        assert_eq!(compatible(&mut unifier, &tcx, int, any), Ok(()));
    }

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
            let mut unifier = Unifier::new();

            assert_eq!(
                compatible(&mut unifier, &tcx, var, prim_ty),
                Ok(()),
                "{prim:?}"
            );
            assert_eq!(
                compatible(&mut unifier, &tcx, prim_ty, var),
                Ok(()),
                "{prim:?}"
            );
        }
    }

    #[test]
    fn int_var_rejects_bool() {
        let mut tcx = TyCtx::new();
        let var = tcx.next_int_var();
        let bool_ty = tcx.mk_prim(PrimTy::Bool);
        let mut unifier = Unifier::new();

        assert_eq!(
            compatible(&mut unifier, &tcx, var, bool_ty),
            Err(UnifyError::ExpectedInteger {
                var,
                found: bool_ty
            })
        );
        assert_eq!(
            compatible(&mut unifier, &tcx, bool_ty, var),
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
        let mut unifier = Unifier::new();

        assert_eq!(
            compatible(&mut unifier, &tcx, var, f64_ty),
            Err(UnifyError::ExpectedInteger { var, found: f64_ty })
        );
    }

    #[test]
    fn two_int_vars_are_compatible() {
        let mut tcx = TyCtx::new();
        let a = tcx.next_int_var();
        let b = tcx.next_int_var();
        let mut unifier = Unifier::new();

        assert_eq!(compatible(&mut unifier, &tcx, a, b), Ok(()));
    }

    #[test]
    fn float_var_is_compatible_with_every_float_primitive() {
        for prim in [PrimTy::F32, PrimTy::F64] {
            let mut tcx = TyCtx::new();
            let var = tcx.next_float_var();
            let prim_ty = tcx.mk_prim(prim);
            let mut unifier = Unifier::new();

            assert_eq!(
                compatible(&mut unifier, &tcx, var, prim_ty),
                Ok(()),
                "{prim:?}"
            );
            assert_eq!(
                compatible(&mut unifier, &tcx, prim_ty, var),
                Ok(()),
                "{prim:?}"
            );
        }
    }

    #[test]
    fn float_var_rejects_an_integer_primitive() {
        let mut tcx = TyCtx::new();
        let var = tcx.next_float_var();
        let i32_ty = tcx.mk_prim(PrimTy::I32);
        let mut unifier = Unifier::new();

        assert_eq!(
            compatible(&mut unifier, &tcx, var, i32_ty),
            Err(UnifyError::ExpectedFloat { var, found: i32_ty })
        );
        assert_eq!(
            compatible(&mut unifier, &tcx, i32_ty, var),
            Err(UnifyError::ExpectedFloat { var, found: i32_ty })
        );
    }

    #[test]
    fn two_float_vars_are_compatible() {
        let mut tcx = TyCtx::new();
        let a = tcx.next_float_var();
        let b = tcx.next_float_var();
        let mut unifier = Unifier::new();

        assert_eq!(compatible(&mut unifier, &tcx, a, b), Ok(()));
    }

    #[test]
    fn same_primitive_is_compatible() {
        let mut tcx = TyCtx::new();
        let a = tcx.mk_prim(PrimTy::I32);
        let b = tcx.mk_prim(PrimTy::I32);
        let mut unifier = Unifier::new();

        assert_eq!(compatible(&mut unifier, &tcx, a, b), Ok(()));
    }

    #[test]
    fn different_primitives_are_incompatible() {
        let mut tcx = TyCtx::new();
        let i32_ty = tcx.mk_prim(PrimTy::I32);
        let bool_ty = tcx.mk_prim(PrimTy::Bool);
        let mut unifier = Unifier::new();

        assert_eq!(
            compatible(&mut unifier, &tcx, i32_ty, bool_ty),
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
        let mut unifier = Unifier::new();

        assert_eq!(compatible(&mut unifier, &tcx, a, b), Ok(()));
    }

    #[test]
    fn different_generic_hir_ids_are_incompatible() {
        let mut tcx = TyCtx::new();
        let a = tcx.mk_generic(hir_id(0));
        let b = tcx.mk_generic(hir_id(1));
        let mut unifier = Unifier::new();

        assert_eq!(
            compatible(&mut unifier, &tcx, a, b),
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
        let mut unifier = Unifier::new();

        assert_eq!(compatible(&mut unifier, &tcx, a, b), Ok(()));
    }

    #[test]
    fn different_self_param_defs_are_incompatible() {
        let mut tcx = TyCtx::new();
        let a = tcx.mk_self_param(DefId::from_usize(0));
        let b = tcx.mk_self_param(DefId::from_usize(1));
        let mut unifier = Unifier::new();

        assert_eq!(
            compatible(&mut unifier, &tcx, a, b),
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
        let mut unifier = Unifier::new();

        assert_eq!(compatible(&mut unifier, &tcx, a, b), Ok(()));
    }

    #[test]
    fn adt_with_same_def_and_different_args_is_incompatible() {
        let mut tcx = TyCtx::new();
        let def = DefId::from_usize(0);
        let arg1 = tcx.mk_prim(PrimTy::I32);
        let arg2 = tcx.mk_prim(PrimTy::Bool);
        let a = tcx.mk_adt(def, vec![arg1]);
        let b = tcx.mk_adt(def, vec![arg2]);
        let mut unifier = Unifier::new();

        assert_eq!(
            unifier.unify(&tcx, a, b),
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
        let mut unifier = Unifier::new();

        assert_eq!(
            compatible(&mut unifier, &tcx, a, b),
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
        let mut unifier = Unifier::new();

        assert_eq!(compatible(&mut unifier, &tcx, a, b), Ok(()));
    }

    #[test]
    fn ref_with_different_mutability_is_incompatible() {
        let mut tcx = TyCtx::new();
        let base = tcx.mk_prim(PrimTy::I32);
        let a = tcx.mk_ref(base, Mutability::Immutable);
        let b = tcx.mk_ref(base, Mutability::Mutable);
        let mut unifier = Unifier::new();

        assert_eq!(
            compatible(&mut unifier, &tcx, a, b),
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
        let mut unifier = Unifier::new();

        assert_eq!(
            unifier.unify(&tcx, a, b),
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
        let mut unifier = Unifier::new();

        assert_eq!(compatible(&mut unifier, &tcx, a, b), Ok(()));
    }

    #[test]
    fn any_ty_with_different_inner_is_incompatible() {
        let mut tcx = TyCtx::new();
        let i32_ty = tcx.mk_prim(PrimTy::I32);
        let bool_ty = tcx.mk_prim(PrimTy::Bool);
        let a = tcx.mk_any(i32_ty);
        let b = tcx.mk_any(bool_ty);
        let mut unifier = Unifier::new();

        assert_eq!(
            unifier.unify(&tcx, a, b),
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
        let mut unifier = Unifier::new();

        assert_eq!(compatible(&mut unifier, &tcx, a, b), Ok(()));
    }

    #[test]
    fn tuple_with_different_arity_is_incompatible() {
        let mut tcx = TyCtx::new();
        let i32_ty = tcx.mk_prim(PrimTy::I32);
        let a = tcx.mk_tuple(vec![i32_ty]);
        let b = tcx.mk_tuple(vec![i32_ty, i32_ty]);
        let mut unifier = Unifier::new();

        assert_eq!(
            compatible(&mut unifier, &tcx, a, b),
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
        let mut unifier = Unifier::new();

        assert_eq!(
            unifier.unify(&tcx, a, b),
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
        let a = tcx.mk_array(elem, Some(4));
        let b = tcx.mk_array(elem, Some(4));
        let mut unifier = Unifier::new();

        assert_eq!(compatible(&mut unifier, &tcx, a, b), Ok(()));
    }

    #[test]
    fn array_with_no_len_on_both_sides_is_compatible() {
        let mut tcx = TyCtx::new();
        let elem = tcx.mk_prim(PrimTy::I32);
        let a = tcx.mk_array(elem, None);
        let b = tcx.mk_array(elem, None);
        let mut unifier = Unifier::new();

        assert_eq!(compatible(&mut unifier, &tcx, a, b), Ok(()));
    }

    #[test]
    fn array_with_different_lens_is_incompatible() {
        let mut tcx = TyCtx::new();
        let elem = tcx.mk_prim(PrimTy::I32);
        let a = tcx.mk_array(elem, Some(4));
        let b = tcx.mk_array(elem, Some(8));
        let mut unifier = Unifier::new();

        assert_eq!(
            compatible(&mut unifier, &tcx, a, b),
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
        let mut unifier = Unifier::new();

        assert_eq!(compatible(&mut unifier, &tcx, a, b), Ok(()));
    }

    #[test]
    fn fun_with_different_param_count_is_incompatible() {
        let mut tcx = TyCtx::new();
        let i32_ty = tcx.mk_prim(PrimTy::I32);
        let a = tcx.mk_fun(vec![i32_ty], None);
        let b = tcx.mk_fun(vec![i32_ty, i32_ty], None);
        let mut unifier = Unifier::new();

        assert_eq!(
            compatible(&mut unifier, &tcx, a, b),
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
        let mut unifier = Unifier::new();

        assert_eq!(
            unifier.unify(&tcx, a, b),
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
        let mut unifier = Unifier::new();

        assert_eq!(
            compatible(&mut unifier, &tcx, a, b),
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
        let mut unifier = Unifier::new();

        assert_eq!(compatible(&mut unifier, &tcx, a, b), Ok(()));
    }

    #[test]
    fn dyn_with_different_trait_is_incompatible() {
        let mut tcx = TyCtx::new();
        let a = tcx.mk_dyn(DefId::from_usize(0), vec![]);
        let b = tcx.mk_dyn(DefId::from_usize(1), vec![]);
        let mut unifier = Unifier::new();

        assert_eq!(
            compatible(&mut unifier, &tcx, a, b),
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
        let mut unifier = Unifier::new();

        assert_eq!(
            compatible(&mut unifier, &tcx, tuple, fun),
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
        let mut unifier = Unifier::new();

        assert_eq!(
            compatible(&mut unifier, &tcx, prim, adt),
            Err(UnifyError::Mismatch {
                expected: prim,
                found: adt
            })
        );
    }

    #[test]
    fn unifying_a_type_with_itself_succeeds() {
        let mut tcx = TyCtx::new();
        let i32_ty = tcx.mk_prim(PrimTy::I32);
        let mut unifier = Unifier::new();

        assert_eq!(unifier.unify(&tcx, i32_ty, i32_ty), Ok(()));
    }

    #[test]
    fn successful_unify_merges_the_two_classes() {
        let mut tcx = TyCtx::new();
        let var = tcx.next_infer_var();
        let i32_ty = tcx.mk_prim(PrimTy::I32);
        let mut unifier = Unifier::new();

        assert_eq!(unifier.unify(&tcx, var, i32_ty), Ok(()));
        assert_eq!(unifier.find_shallow(var), unifier.find_shallow(i32_ty));
    }

    #[test]
    fn unify_is_transitive_across_three_types() {
        let mut tcx = TyCtx::new();
        let a = tcx.next_infer_var();
        let b = tcx.next_infer_var();
        let c = tcx.mk_prim(PrimTy::I32);
        let mut unifier = Unifier::new();

        assert_eq!(unifier.unify(&tcx, a, b), Ok(()));
        assert_eq!(unifier.unify(&tcx, b, c), Ok(()));
        assert_eq!(unifier.find_shallow(a), unifier.find_shallow(c));
    }

    #[test]
    fn every_member_of_a_large_class_resolves_to_one_representative() {
        const MEMBERS: usize = 50_000;

        let mut tcx = TyCtx::new();
        let vars: Vec<Ty> = (0..MEMBERS).map(|_| tcx.next_infer_var()).collect();
        let i32_ty = tcx.mk_prim(PrimTy::I32);
        let mut unifier = Unifier::new();

        for pair in vars.windows(2) {
            assert_eq!(unifier.unify(&tcx, pair[0], pair[1]), Ok(()));
        }
        assert_eq!(unifier.unify(&tcx, vars[0], i32_ty), Ok(()));

        for &var in &vars {
            assert_eq!(unifier.find_shallow(var), i32_ty);
        }
    }

    #[test]
    fn unifying_a_var_with_a_concrete_type_makes_the_concrete_type_the_representative() {
        let mut tcx = TyCtx::new();
        let var = tcx.next_infer_var();
        let i32_ty = tcx.mk_prim(PrimTy::I32);
        let mut unifier = Unifier::new();

        assert_eq!(unifier.unify(&tcx, var, i32_ty), Ok(()));
        assert_eq!(unifier.find_shallow(var), i32_ty);
    }

    #[test]
    fn unifying_a_concrete_type_with_a_var_makes_the_concrete_type_the_representative() {
        let mut tcx = TyCtx::new();
        let i32_ty = tcx.mk_prim(PrimTy::I32);
        let var = tcx.next_infer_var();
        let mut unifier = Unifier::new();

        assert_eq!(unifier.unify(&tcx, i32_ty, var), Ok(()));
        assert_eq!(unifier.find_shallow(var), i32_ty);
    }

    #[test]
    fn a_concrete_type_stays_the_representative_no_matter_how_many_vars_join_it() {
        let mut tcx = TyCtx::new();
        let i32_ty = tcx.mk_prim(PrimTy::I32);
        let vars: Vec<Ty> = (0..8).map(|_| tcx.next_infer_var()).collect();
        let mut unifier = Unifier::new();

        for pair in vars.windows(2) {
            assert_eq!(unifier.unify(&tcx, pair[0], pair[1]), Ok(()));
        }
        assert_eq!(unifier.unify(&tcx, vars[0], i32_ty), Ok(()));

        for &var in &vars {
            assert_eq!(unifier.find_shallow(var), i32_ty);
        }
    }

    #[test]
    fn a_var_merged_into_a_var_already_unified_with_a_concrete_type_resolves_to_it() {
        let mut tcx = TyCtx::new();
        let a = tcx.next_infer_var();
        let b = tcx.next_infer_var();
        let i32_ty = tcx.mk_prim(PrimTy::I32);
        let mut unifier = Unifier::new();

        assert_eq!(unifier.unify(&tcx, a, i32_ty), Ok(()));
        assert_eq!(unifier.unify(&tcx, a, b), Ok(()));
        assert_eq!(unifier.find_shallow(b), i32_ty);
    }

    #[test]
    fn a_numeric_var_outranks_an_unconstrained_var() {
        let mut tcx = TyCtx::new();
        let any = tcx.next_infer_var();
        let int = tcx.next_int_var();
        let mut unifier = Unifier::new();

        assert_eq!(unifier.unify(&tcx, any, int), Ok(()));
        assert_eq!(unifier.find_shallow(any), int);

        let mut tcx = TyCtx::new();
        let any = tcx.next_infer_var();
        let float = tcx.next_float_var();
        let mut unifier = Unifier::new();

        assert_eq!(unifier.unify(&tcx, float, any), Ok(()));
        assert_eq!(unifier.find_shallow(any), float);
    }

    #[test]
    fn a_var_that_absorbed_an_int_var_still_rejects_bool() {
        let mut tcx = TyCtx::new();
        let result = tcx.next_infer_var();
        let int = tcx.next_int_var();
        let bool_ty = tcx.mk_prim(PrimTy::Bool);
        let mut unifier = Unifier::new();

        assert_eq!(unifier.unify(&tcx, result, int), Ok(()));
        assert_eq!(
            unifier.unify(&tcx, result, bool_ty),
            Err(UnifyError::ExpectedInteger {
                var: int,
                found: bool_ty
            })
        );
    }

    #[test]
    fn two_vars_unify_by_size_when_neither_is_concrete() {
        let mut tcx = TyCtx::new();
        let a = tcx.next_infer_var();
        let b = tcx.next_infer_var();
        let c = tcx.next_infer_var();
        let mut unifier = Unifier::new();

        assert_eq!(unifier.unify(&tcx, a, b), Ok(()));
        let b_repr = unifier.find_shallow(b);
        assert_eq!(unifier.unify(&tcx, b, c), Ok(()));
        assert_eq!(unifier.find_shallow(c), b_repr);
    }

    #[test]
    fn a_variable_cannot_be_bound_to_a_type_containing_it() {
        let mut tcx = TyCtx::new();
        let var = tcx.next_infer_var();
        let i32_ty = tcx.mk_prim(PrimTy::I32);
        let tuple = tcx.mk_tuple(vec![var, i32_ty]);
        let mut unifier = Unifier::new();

        assert_eq!(
            unifier.unify(&tcx, var, tuple),
            Err(UnifyError::Infinite { var, found: tuple })
        );
        assert_eq!(unifier.find_shallow(var), var);
    }

    #[test]
    fn the_occurs_check_is_symmetric() {
        let mut tcx = TyCtx::new();
        let var = tcx.next_infer_var();
        let tuple = tcx.mk_tuple(vec![var]);
        let mut unifier = Unifier::new();

        assert!(matches!(
            unifier.unify(&tcx, tuple, var),
            Err(UnifyError::Infinite { .. })
        ));
    }

    #[test]
    fn a_cycle_closed_through_another_variable_is_caught() {
        let mut tcx = TyCtx::new();
        let a = tcx.next_infer_var();
        let b = tcx.next_infer_var();
        let tuple_b = tcx.mk_tuple(vec![b]);
        let tuple_a = tcx.mk_tuple(vec![a]);
        let mut unifier = Unifier::new();

        assert_eq!(unifier.unify(&tcx, a, tuple_b), Ok(()));
        assert!(matches!(
            unifier.unify(&tcx, b, tuple_a),
            Err(UnifyError::Infinite { .. })
        ));
    }

    #[test]
    fn a_variable_may_be_bound_to_a_type_containing_another_variable() {
        let mut tcx = TyCtx::new();
        let a = tcx.next_infer_var();
        let b = tcx.next_infer_var();
        let i32_ty = tcx.mk_prim(PrimTy::I32);
        let tuple = tcx.mk_tuple(vec![b, i32_ty]);
        let mut unifier = Unifier::new();

        assert_eq!(unifier.unify(&tcx, a, tuple), Ok(()));
        assert_eq!(unifier.find_shallow(a), tuple);
    }

    #[test]
    fn failed_unify_leaves_both_classes_untouched() {
        let mut tcx = TyCtx::new();
        let i32_ty = tcx.mk_prim(PrimTy::I32);
        let bool_ty = tcx.mk_prim(PrimTy::Bool);
        let mut unifier = Unifier::new();

        assert_eq!(
            unifier.unify(&tcx, i32_ty, bool_ty),
            Err(UnifyError::Mismatch {
                expected: i32_ty,
                found: bool_ty
            })
        );
        assert_eq!(unifier.find_shallow(i32_ty), i32_ty);
        assert_eq!(unifier.find_shallow(bool_ty), bool_ty);
    }

    #[test]
    fn unify_can_be_called_twice_on_the_same_pair() {
        let mut tcx = TyCtx::new();
        let var = tcx.next_infer_var();
        let i32_ty = tcx.mk_prim(PrimTy::I32);
        let mut unifier = Unifier::new();

        assert_eq!(unifier.unify(&tcx, var, i32_ty), Ok(()));
        assert_eq!(unifier.unify(&tcx, var, i32_ty), Ok(()));
    }

    #[test]
    fn a_failed_unify_does_not_poison_later_unrelated_unifications() {
        let mut tcx = TyCtx::new();
        let i32_ty = tcx.mk_prim(PrimTy::I32);
        let bool_ty = tcx.mk_prim(PrimTy::Bool);
        let var = tcx.next_infer_var();
        let mut unifier = Unifier::new();

        assert!(unifier.unify(&tcx, i32_ty, bool_ty).is_err());
        assert_eq!(unifier.unify(&tcx, var, i32_ty), Ok(()));
    }

    #[test]
    fn a_tuple_holding_a_var_unifies_with_the_resolved_tuple() {
        let mut tcx = TyCtx::new();
        let i32_ty = tcx.mk_prim(PrimTy::I32);
        let (v1, v2) = (tcx.next_int_var(), tcx.next_int_var());
        let expected = tcx.mk_tuple(vec![i32_ty, i32_ty]);
        let found = tcx.mk_tuple(vec![v1, v2]);
        let mut unifier = Unifier::new();

        assert_eq!(unifier.unify(&tcx, expected, found), Ok(()));
        assert_eq!(unifier.find_shallow(v1), i32_ty);
        assert_eq!(unifier.find_shallow(v2), i32_ty);
    }

    #[test]
    fn unification_recurses_through_every_composite_shape() {
        let mut tcx = TyCtx::new();
        let i32_ty = tcx.mk_prim(PrimTy::I32);
        let def = DefId::from_usize(0);
        let len = 4;

        let composites: Vec<(&str, Ty, Ty, Ty)> = vec![
            {
                let var = tcx.next_infer_var();
                (
                    "Adt",
                    tcx.mk_adt(def, vec![i32_ty]),
                    tcx.mk_adt(def, vec![var]),
                    var,
                )
            },
            {
                let var = tcx.next_infer_var();
                (
                    "Ref",
                    tcx.mk_ref(i32_ty, Mutability::Immutable),
                    tcx.mk_ref(var, Mutability::Immutable),
                    var,
                )
            },
            {
                let var = tcx.next_infer_var();
                ("Any", tcx.mk_any(i32_ty), tcx.mk_any(var), var)
            },
            {
                let var = tcx.next_infer_var();
                (
                    "Tuple",
                    tcx.mk_tuple(vec![i32_ty]),
                    tcx.mk_tuple(vec![var]),
                    var,
                )
            },
            {
                let var = tcx.next_infer_var();
                (
                    "Array",
                    tcx.mk_array(i32_ty, Some(len)),
                    tcx.mk_array(var, Some(len)),
                    var,
                )
            },
            {
                let var = tcx.next_infer_var();
                (
                    "Fun params",
                    tcx.mk_fun(vec![i32_ty], None),
                    tcx.mk_fun(vec![var], None),
                    var,
                )
            },
            {
                let var = tcx.next_infer_var();
                (
                    "Fun ret",
                    tcx.mk_fun(vec![], Some(i32_ty)),
                    tcx.mk_fun(vec![], Some(var)),
                    var,
                )
            },
            {
                let var = tcx.next_infer_var();
                (
                    "Dyn",
                    tcx.mk_dyn(def, vec![i32_ty]),
                    tcx.mk_dyn(def, vec![var]),
                    var,
                )
            },
        ];

        for (shape, expected, found, var) in composites {
            let mut unifier = Unifier::new();
            assert_eq!(unifier.unify(&tcx, expected, found), Ok(()), "{shape}");
            assert_eq!(
                unifier.find_shallow(var),
                i32_ty,
                "{shape} did not resolve its component"
            );
        }
    }

    #[test]
    fn unification_recurses_more_than_one_level_deep() {
        let mut tcx = TyCtx::new();
        let i32_ty = tcx.mk_prim(PrimTy::I32);
        let var = tcx.next_infer_var();

        let inner_expected = tcx.mk_ref(i32_ty, Mutability::Immutable);
        let inner_found = tcx.mk_ref(var, Mutability::Immutable);
        let expected = tcx.mk_tuple(vec![inner_expected]);
        let found = tcx.mk_tuple(vec![inner_found]);
        let mut unifier = Unifier::new();

        assert_eq!(unifier.unify(&tcx, expected, found), Ok(()));
        assert_eq!(unifier.find_shallow(var), i32_ty);
    }

    #[test]
    fn a_mismatch_inside_a_composite_is_reported_against_the_outer_types() {
        let mut tcx = TyCtx::new();
        let i32_ty = tcx.mk_prim(PrimTy::I32);
        let bool_ty = tcx.mk_prim(PrimTy::Bool);
        let expected = tcx.mk_tuple(vec![i32_ty, i32_ty]);
        let found = tcx.mk_tuple(vec![i32_ty, bool_ty]);
        let mut unifier = Unifier::new();

        assert_eq!(
            unifier.unify(&tcx, expected, found),
            Err(UnifyError::Mismatch { expected, found })
        );
    }

    #[test]
    fn a_var_kind_error_inside_a_composite_keeps_naming_the_variable() {
        let mut tcx = TyCtx::new();
        let bool_ty = tcx.mk_prim(PrimTy::Bool);
        let var = tcx.next_int_var();
        let expected = tcx.mk_tuple(vec![bool_ty]);
        let found = tcx.mk_tuple(vec![var]);
        let mut unifier = Unifier::new();

        assert_eq!(
            unifier.unify(&tcx, expected, found),
            Err(UnifyError::ExpectedInteger {
                var,
                found: bool_ty
            })
        );
    }

    #[test]
    fn two_concrete_composites_that_unify_are_not_merged_into_one_class() {
        let mut tcx = TyCtx::new();
        let i32_ty = tcx.mk_prim(PrimTy::I32);
        let var = tcx.next_int_var();
        let expected = tcx.mk_tuple(vec![i32_ty]);
        let found = tcx.mk_tuple(vec![var]);
        let mut unifier = Unifier::new();

        assert_eq!(unifier.unify(&tcx, expected, found), Ok(()));
        assert_eq!(unifier.find_shallow(expected), expected);
        assert_eq!(unifier.find_shallow(found), found);
    }

    #[test]
    fn unifying_with_never_does_not_merge_the_two_classes() {
        let mut tcx = TyCtx::new();
        let bool_ty = tcx.mk_prim(PrimTy::Bool);
        let never = tcx.never();
        let mut unifier = Unifier::new();

        assert_eq!(unifier.unify(&tcx, never, bool_ty), Ok(()));
        assert_eq!(
            unifier.find_shallow(never),
            never,
            "`never` was folded into bool's class"
        );
        assert_eq!(unifier.find_shallow(bool_ty), bool_ty);
    }

    #[test]
    fn unifying_with_error_does_not_merge_the_two_classes() {
        let mut tcx = TyCtx::new();
        let bool_ty = tcx.mk_prim(PrimTy::Bool);
        let error = tcx.error();
        let mut unifier = Unifier::new();

        assert_eq!(unifier.unify(&tcx, error, bool_ty), Ok(()));
        assert_eq!(
            unifier.find_shallow(error),
            error,
            "`error` was folded into bool's class"
        );
        assert_eq!(unifier.find_shallow(bool_ty), bool_ty);
    }

    #[test]
    fn never_stays_neutral_across_unrelated_unifications() {
        let mut tcx = TyCtx::new();
        let bool_ty = tcx.mk_prim(PrimTy::Bool);
        let int_var = tcx.next_int_var();
        let never = tcx.never();
        let mut unifier = Unifier::new();

        assert_eq!(unifier.unify(&tcx, never, bool_ty), Ok(()));
        assert_eq!(
            unifier.unify(&tcx, never, int_var),
            Ok(()),
            "unifying `Never` with bool poisoned it for every later use"
        );
        assert_eq!(
            unifier.find_shallow(int_var),
            int_var,
            "the int var was bound to bool"
        );
    }

    #[test]
    fn unit_is_never_folded_into_another_class() {
        let mut tcx = TyCtx::new();
        let unit = tcx.unit();
        let var = tcx.next_infer_var();
        let mut unifier = Unifier::new();

        assert_eq!(unifier.unify(&tcx, unit, var), Ok(()));
        assert_eq!(unifier.find_shallow(unit), unit);
        assert_eq!(unifier.find_shallow(var), unit);
    }

    #[test]
    fn unit_does_not_unify_with_an_unrelated_type() {
        let mut tcx = TyCtx::new();
        let unit = tcx.unit();
        let bool_ty = tcx.mk_prim(PrimTy::Bool);
        let mut unifier = Unifier::new();

        assert_eq!(
            unifier.unify(&tcx, unit, bool_ty),
            Err(UnifyError::Mismatch {
                expected: unit,
                found: bool_ty
            })
        );
    }
}
