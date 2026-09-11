use crate::typeck::fold::{self, Subst};
use crate::typeck::ty::{Ty, TyKind};
use crate::typeck::tyctx::TyCtx;

pub(crate) fn subst_ty(tcx: &mut TyCtx, ty: Ty, subst: &Subst) -> Ty {
    fold::subst_ty(tcx, ty, subst)
}

pub(crate) fn mentions_generic(tcx: &TyCtx, ty: Ty) -> bool {
    fold::contains(tcx, ty, &mut |ty| {
        matches!(tcx.kind(ty), TyKind::Generic(_) | TyKind::SelfTy(_))
    })
}
