use crate::typeck::ty::{Ty, TyKind};
use crate::typeck::tyctx::TyCtx;
use crate::typeck::visitor::{self, Subst};

pub(crate) fn subst_ty(tcx: &mut TyCtx, ty: Ty, subst: &Subst) -> Ty {
    visitor::subst_ty(tcx, ty, subst)
}

pub(crate) fn mentions_generic(tcx: &TyCtx, ty: Ty) -> bool {
    visitor::any_ty(tcx, ty, |tcx, ty| {
        matches!(tcx.kind(ty), TyKind::Generic(_) | TyKind::SelfTy(_))
    })
}
