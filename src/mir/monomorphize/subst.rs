//! The two questions monomorphization asks about a type, both of them walks that
//! [`typeck::fold`](crate::typeck::fold) already performs.
//!
//! Substitution is the identical operation the checker calls
//! [`subst_ty`](crate::typeck::fold::subst_ty), and it is reached from here rather than reimplemented
//! because nothing about it is particular to this pass: `Typeck` is gone by the time a `Body` is
//! monomorphized, and the [`TyCtx`] the substitution actually works on outlives it.

use std::collections::HashMap;

use crate::hir::HirId;
use crate::typeck::fold;
use crate::typeck::ty::{Ty, TyKind};
use crate::typeck::tyctx::TyCtx;

/// Rebuilds `ty` with every parameter in `subst` replaced by what it is bound to.
pub(crate) fn subst_ty(tcx: &mut TyCtx, ty: Ty, subst: &HashMap<HirId, Ty>) -> Ty {
    fold::subst_ty(tcx, ty, subst)
}

/// Whether `ty` still mentions [`TyKind::Generic`] or [`TyKind::SelfTy`] anywhere inside it. A
/// `Body` this returns `false` for needs no substitution at all and is trivially its own root
/// instance.
pub(crate) fn mentions_generic(tcx: &TyCtx, ty: Ty) -> bool {
    fold::contains(tcx, ty, &mut |ty| {
        matches!(tcx.kind(ty), TyKind::Generic(_) | TyKind::SelfTy(_))
    })
}
