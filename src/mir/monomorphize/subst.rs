use crate::typeck::ty::ctx::TyCtx;
use crate::typeck::ty::visitor::{self, Subst};
use crate::typeck::ty::{Ty, TyKind};

pub(crate) fn subst_ty(tcx: &mut TyCtx, ty: Ty, subst: &Subst) -> Ty {
    visitor::subst_ty(tcx, ty, subst)
}

pub(crate) fn mentions_generic(tcx: &TyCtx, ty: Ty) -> bool {
    visitor::any_ty(tcx, ty, |tcx, ty| {
        matches!(tcx.kind(ty), TyKind::Generic(_) | TyKind::SelfTy(_))
    })
}
