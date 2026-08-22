use std::collections::HashMap;

use crate::hir::HirId;
use crate::typeck::fold;
use crate::typeck::ty::{Ty, TyKind};
use crate::typeck::tyctx::TyCtx;

/// Rebuilds `ty` with every parameter in `subst` replaced by what it is bound to.
pub(crate) fn subst_ty(tcx: &mut TyCtx, ty: Ty, subst: &HashMap<HirId, Ty>) -> Ty {
    fold::subst_ty(tcx, ty, subst)
}

/// Whether `ty` mentions [`TyKind::Generic`] or [`TyKind::SelfTy`] anywhere inside it.
pub(crate) fn mentions_generic(tcx: &TyCtx, ty: Ty) -> bool {
    fold::contains(tcx, ty, &mut |ty| {
        matches!(tcx.kind(ty), TyKind::Generic(_) | TyKind::SelfTy(_))
    })
}
