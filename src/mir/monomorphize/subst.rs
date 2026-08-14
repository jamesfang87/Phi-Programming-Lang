//! `Ty` substitution, a direct port of `Typeck::subst_ty`'s match arms onto a bare `&mut TyCtx`
//! rather than `&mut Typeck` -- nothing that function touches beyond the type context is
//! actually `Typeck`-only, so the substitution itself needs no new logic, only a new home now
//! that `Typeck` itself no longer exists by the time this pass runs.

use std::collections::HashMap;

use crate::hir::HirId;
use crate::typeck::ty::{Ty, TyKind};
use crate::typeck::tyctx::TyCtx;

/// Rebuilds `ty` with every parameter in `subst` replaced by what it is bound to. Only the
/// parameters in `subst` are touched; anything else -- including a parameter of some enclosing
/// definition -- is left exactly as it was. `TyKind::SelfTy` is left untouched too: by the time
/// a generic `Body` reaches this pass, an impl's `Self` was already replaced by its self type
/// when the impl's header was lowered, the same reasoning `Typeck::subst_ty` documents.
pub(crate) fn subst_ty(tcx: &mut TyCtx, ty: Ty, subst: &HashMap<HirId, Ty>) -> Ty {
    match tcx.kind(ty).clone() {
        TyKind::Generic(param) => subst.get(&param).copied().unwrap_or(ty),
        TyKind::Adt { def, args } => {
            let args = subst_tys(tcx, &args, subst);
            tcx.mk_adt(def, args)
        }
        TyKind::Dyn { trait_, args } => {
            let args = subst_tys(tcx, &args, subst);
            tcx.mk_dyn(trait_, args)
        }
        TyKind::Tuple(elems) => {
            let elems = subst_tys(tcx, &elems, subst);
            tcx.mk_tuple(elems)
        }
        TyKind::Ref { base, mutability } => {
            let base = subst_ty(tcx, base, subst);
            tcx.mk_ref(base, mutability)
        }
        TyKind::Any(base) => {
            let base = subst_ty(tcx, base, subst);
            tcx.mk_any(base)
        }
        TyKind::Array { elem, len } => {
            let elem = subst_ty(tcx, elem, subst);
            tcx.mk_array(elem, len)
        }
        TyKind::Fun { params, ret } => {
            let params = subst_tys(tcx, &params, subst);
            let ret = ret.map(|ret| subst_ty(tcx, ret, subst));
            tcx.mk_fun(params, ret)
        }
        TyKind::Var(_)
        | TyKind::Primitive(_)
        | TyKind::SelfTy(_)
        | TyKind::Unit
        | TyKind::Never
        | TyKind::Error => ty,
    }
}

fn subst_tys(tcx: &mut TyCtx, tys: &[Ty], subst: &HashMap<HirId, Ty>) -> Vec<Ty> {
    tys.iter().map(|&ty| subst_ty(tcx, ty, subst)).collect()
}

/// Whether `ty` still mentions `TyKind::Generic` or `TyKind::SelfTy` anywhere inside it -- a
/// `Body` this returns `false` for needs no substitution at all and is trivially its own root
/// instance.
pub(crate) fn mentions_generic(tcx: &TyCtx, ty: Ty) -> bool {
    match tcx.kind(ty).clone() {
        TyKind::Generic(_) | TyKind::SelfTy(_) => true,
        TyKind::Adt { args, .. } | TyKind::Dyn { args, .. } | TyKind::Tuple(args) => {
            args.iter().any(|&a| mentions_generic(tcx, a))
        }
        TyKind::Ref { base, .. } | TyKind::Any(base) | TyKind::Array { elem: base, .. } => {
            mentions_generic(tcx, base)
        }
        TyKind::Fun { params, ret } => {
            params.iter().any(|&p| mentions_generic(tcx, p))
                || ret.is_some_and(|r| mentions_generic(tcx, r))
        }
        TyKind::Var(_) | TyKind::Primitive(_) | TyKind::Unit | TyKind::Never | TyKind::Error => {
            false
        }
    }
}
