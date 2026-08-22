use std::collections::HashMap;

use crate::hir::HirId;
use crate::typeck::ty::{Ty, TyKind};
use crate::typeck::tyctx::TyCtx;

pub fn fold_ty(tcx: &mut TyCtx, ty: Ty, leaf: &mut impl FnMut(&mut TyCtx, Ty) -> Option<Ty>) -> Ty {
    if let Some(replaced) = leaf(tcx, ty) {
        return replaced;
    }

    match tcx.kind(ty).clone() {
        TyKind::Adt { def, args } => {
            let args = fold_tys(tcx, &args, leaf);
            tcx.mk_adt(def, args)
        }
        TyKind::Dyn { trait_, args } => {
            let args = fold_tys(tcx, &args, leaf);
            tcx.mk_dyn(trait_, args)
        }
        TyKind::Tuple(elems) => {
            let elems = fold_tys(tcx, &elems, leaf);
            tcx.mk_tuple(elems)
        }
        TyKind::Ref { base, mutability } => {
            let base = fold_ty(tcx, base, leaf);
            tcx.mk_ref(base, mutability)
        }
        TyKind::Any(base) => {
            let base = fold_ty(tcx, base, leaf);
            tcx.mk_any(base)
        }
        TyKind::Iso(base) => {
            let base = fold_ty(tcx, base, leaf);
            tcx.mk_iso(base)
        }
        TyKind::Array { elem, len } => {
            let elem = fold_ty(tcx, elem, leaf);
            tcx.mk_array(elem, len)
        }
        TyKind::Fun { params, ret } => {
            let params = fold_tys(tcx, &params, leaf);
            let ret = ret.map(|ret| fold_ty(tcx, ret, leaf));
            tcx.mk_fun(params, ret)
        }
        // Nothing to recurse into.
        TyKind::Var(_)
        | TyKind::Primitive(_)
        | TyKind::Generic(_)
        | TyKind::SelfTy(_)
        | TyKind::Unit
        | TyKind::Never
        | TyKind::Error => ty,
    }
}

pub fn fold_tys(
    tcx: &mut TyCtx,
    tys: &[Ty],
    leaf: &mut impl FnMut(&mut TyCtx, Ty) -> Option<Ty>,
) -> Vec<Ty> {
    tys.iter().map(|&ty| fold_ty(tcx, ty, leaf)).collect()
}

pub fn subst_ty(tcx: &mut TyCtx, ty: Ty, subst: &HashMap<HirId, Ty>) -> Ty {
    fold_ty(tcx, ty, &mut |tcx, ty| match *tcx.kind(ty) {
        TyKind::Generic(param) => Some(subst.get(&param).copied().unwrap_or(ty)),
        _ => None,
    })
}

pub fn children(tcx: &TyCtx, ty: Ty) -> Vec<Ty> {
    match tcx.kind(ty) {
        TyKind::Adt { args, .. } | TyKind::Dyn { args, .. } | TyKind::Tuple(args) => args.clone(),
        TyKind::Ref { base, .. } | TyKind::Any(base) | TyKind::Iso(base) => vec![*base],
        TyKind::Array { elem, .. } => vec![*elem],
        TyKind::Fun { params, ret } => {
            let mut children = params.clone();
            children.extend(ret);
            children
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

pub fn contains(tcx: &TyCtx, ty: Ty, pred: &mut impl FnMut(Ty) -> bool) -> bool {
    pred(ty)
        || children(tcx, ty)
            .into_iter()
            .any(|child| contains(tcx, child, pred))
}

pub fn mentions_error(tcx: &TyCtx, ty: Ty) -> bool {
    contains(tcx, ty, &mut |ty| matches!(tcx.kind(ty), TyKind::Error))
}

pub fn decompose(tcx: &TyCtx, a: Ty, b: Ty) -> Option<Vec<(Ty, Ty)>> {
    match (tcx.kind(a), tcx.kind(b)) {
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

        (TyKind::Iso(x), TyKind::Iso(y)) => Some(vec![(*x, *y)]),

        (TyKind::Tuple(x), TyKind::Tuple(y)) => (x.len() == y.len()).then(|| zip(x, y)),

        (TyKind::Array { elem: x, len: m }, TyKind::Array { elem: y, len: n }) => {
            // TODO: const-checking for these
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
                // One returns something and the other returns nothing, which is not the same
                // type, and there is no component pair to blame it on.
                (Some(_), None) | (None, Some(_)) => return None,
            }
            Some(components)
        }

        // Two composites of different shapes, and everything with no components at all.
        _ => (a == b).then(Vec::new),
    }
}

/// Pairs two equal-length component lists up positionally.
fn zip(a: &[Ty], b: &[Ty]) -> Vec<(Ty, Ty)> {
    debug_assert_eq!(a.len(), b.len());
    a.iter().copied().zip(b.iter().copied()).collect()
}
