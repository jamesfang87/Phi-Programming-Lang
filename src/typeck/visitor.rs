use std::collections::HashMap;
use std::ops::ControlFlow;

use crate::hir::HirId;
use crate::typeck::Typeck;
use crate::typeck::ty::{Ty, TyKind};
use crate::typeck::tyctx::TyCtx;

// TODO: Give these better names and comments

// ---------------------------------------------------------------------------
// Visiting
// ---------------------------------------------------------------------------

pub trait TypeVisitor {
    type Output;

    fn visit(&mut self, tcx: &TyCtx, ty: Ty) -> ControlFlow<Self::Output>;

    fn children(&mut self, tcx: &TyCtx, ty: Ty) -> Vec<Ty> {
        children(tcx, ty)
    }
}

pub fn walk<V: TypeVisitor>(visitor: &mut V, tcx: &TyCtx, ty: Ty) -> ControlFlow<V::Output> {
    visitor.visit(tcx, ty)?;
    for child in visitor.children(tcx, ty) {
        walk(visitor, tcx, child)?;
    }
    ControlFlow::Continue(())
}

/// Walks `ty` for its side effects, ignoring a visitor that never breaks.
pub fn walk_all<V: TypeVisitor<Output = ()>>(visitor: &mut V, tcx: &TyCtx, ty: Ty) {
    let _ = walk(visitor, tcx, ty);
}

/// Whether any type `accept` accepts is reachable from `ty`.
pub fn any_ty(tcx: &TyCtx, ty: Ty, accept: impl FnMut(&TyCtx, Ty) -> bool) -> bool {
    walk(&mut Search::descending(accept), tcx, ty).is_break()
}

/// Whether any type `accept` accepts is reachable from `ty`, without looking inside a function
/// type's own signature.
///
/// A function's parameters and return type belong to that function, not to the type it appears
/// in, so a search asking about the surrounding type stops at one.
pub fn any_ty_outside_funs(tcx: &TyCtx, ty: Ty, accept: impl FnMut(&TyCtx, Ty) -> bool) -> bool {
    walk(&mut Search::shallow(accept), tcx, ty).is_break()
}

/// Whether `ty` mentions [`TyKind::Error`] anywhere inside it.
pub fn mentions_error(tcx: &TyCtx, ty: Ty) -> bool {
    any_ty(tcx, ty, |tcx, ty| matches!(tcx.kind(ty), TyKind::Error))
}

/// A search for the first type a predicate accepts.
struct Search<F> {
    accept: F,
    enter_funs: bool,
}

impl<F> Search<F> {
    fn descending(accept: F) -> Self {
        Search {
            accept,
            enter_funs: true,
        }
    }

    fn shallow(accept: F) -> Self {
        Search {
            accept,
            enter_funs: false,
        }
    }
}

impl<F: FnMut(&TyCtx, Ty) -> bool> TypeVisitor for Search<F> {
    type Output = ();

    fn visit(&mut self, tcx: &TyCtx, ty: Ty) -> ControlFlow<()> {
        if (self.accept)(tcx, ty) {
            ControlFlow::Break(())
        } else {
            ControlFlow::Continue(())
        }
    }

    fn children(&mut self, tcx: &TyCtx, ty: Ty) -> Vec<Ty> {
        if !self.enter_funs && matches!(tcx.kind(ty), TyKind::Fun { .. }) {
            Vec::new()
        } else {
            children(tcx, ty)
        }
    }
}

/// The immediate sub-types of `ty`, in declaration order.
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

// ---------------------------------------------------------------------------
// Folding
// ---------------------------------------------------------------------------

/// Rebuilds `ty`, giving `rewrite` the chance to replace each type first.
///
/// `rewrite` returning `Some(replacement)` replaces that type outright, without descending into
/// it; `None` walks its children and rebuilds it from the rewritten results.
pub fn fold_ty(
    tcx: &mut TyCtx,
    ty: Ty,
    rewrite: &mut impl FnMut(&mut TyCtx, Ty) -> Option<Ty>,
) -> Ty {
    if let Some(replaced) = rewrite(tcx, ty) {
        return replaced;
    }

    match tcx.kind(ty).clone() {
        TyKind::Adt { def, args } => {
            let args = fold_tys(tcx, &args, rewrite);
            tcx.mk_adt(def, args)
        }
        TyKind::Dyn { trait_, args } => {
            let args = fold_tys(tcx, &args, rewrite);
            tcx.mk_dyn(trait_, args)
        }
        TyKind::Tuple(elems) => {
            let elems = fold_tys(tcx, &elems, rewrite);
            tcx.mk_tuple(elems)
        }
        TyKind::Ref { base, mutability } => {
            let base = fold_ty(tcx, base, rewrite);
            tcx.mk_ref(base, mutability)
        }
        TyKind::Any(base) => {
            let base = fold_ty(tcx, base, rewrite);
            tcx.mk_any(base)
        }
        TyKind::Iso(base) => {
            let base = fold_ty(tcx, base, rewrite);
            tcx.mk_iso(base)
        }
        TyKind::Array { elem, len } => {
            let elem = fold_ty(tcx, elem, rewrite);
            tcx.mk_array(elem, len)
        }
        TyKind::Fun { params, ret } => {
            let params = fold_tys(tcx, &params, rewrite);
            let ret = ret.map(|ret| fold_ty(tcx, ret, rewrite));
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
    rewrite: &mut impl FnMut(&mut TyCtx, Ty) -> Option<Ty>,
) -> Vec<Ty> {
    tys.iter().map(|&ty| fold_ty(tcx, ty, rewrite)).collect()
}

#[derive(Default)]
pub struct Subst {
    pub generics: HashMap<HirId, Ty>,
    pub self_ty: Option<Ty>,
}

pub fn subst_ty(tcx: &mut TyCtx, ty: Ty, subst: &Subst) -> Ty {
    fold_ty(tcx, ty, &mut |tcx, ty| match *tcx.kind(ty) {
        TyKind::Generic(param) => Some(subst.generics.get(&param).copied().unwrap_or(ty)),
        TyKind::SelfTy(_) => subst.self_ty,
        _ => None,
    })
}

impl<'hir> Typeck<'hir> {
    /// Rebuilds `ty` with every generic parameter in `subst` replaced by what it is bound to.
    pub fn subst_ty(&mut self, ty: Ty, subst: &HashMap<HirId, Ty>) -> Ty {
        let subst = Subst {
            generics: subst.clone(),
            self_ty: None,
        };
        subst_ty(&mut self.tcx, ty, &subst)
    }

    /// Rebuilds `ty` with every generic parameter in `subst` replaced by what it is bound to,
    /// and every `Self` replaced by `self_ty`.
    pub(crate) fn subst_sig_ty(&mut self, ty: Ty, subst: &HashMap<HirId, Ty>, self_ty: Ty) -> Ty {
        fold_ty(&mut self.tcx, ty, &mut |tcx, ty| match *tcx.kind(ty) {
            TyKind::Generic(param) => Some(subst.get(&param).copied().unwrap_or(ty)),
            TyKind::SelfTy(_) => Some(self_ty),
            _ => None,
        })
    }
}

// ---------------------------------------------------------------------------
// Pairwise decomposition
// ---------------------------------------------------------------------------

/// Decomposes two same-shaped types into the component pairs that must themselves match, or
/// `None` when their shapes differ.
///
/// This is the structural half of both unification and one-way matching: what each caller does
/// with the pairs (bind variables, recurse, fail) is its own policy.
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
