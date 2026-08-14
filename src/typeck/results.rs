use std::collections::HashMap;

use crate::hir::{DefId, HirId};
use crate::typeck::ty::Ty;

/// What a `Call` or a method-call `Access` expression resolved to: the concrete definition being
/// called, and the type arguments its generics were instantiated with at this call site.
///
/// `def` alone does not pick out one `Body` for a generic definition, the same reason
/// [`crate::mir::ConstKind::FnDef`] carries a `Vec<Ty>` alongside its `DefId`; `args` is that
/// same instantiation, computed once here rather than re-derived by every later pass that needs
/// it.
#[derive(Clone, Debug)]
pub struct ResolvedCall {
    pub def: DefId,
    pub args: Vec<Ty>,
}

#[derive(Default)]
pub struct TypeResolutions {
    ty: HashMap<HirId, Ty>,
    calls: HashMap<HirId, ResolvedCall>,
}

impl TypeResolutions {
    pub fn new() -> TypeResolutions {
        TypeResolutions {
            ty: HashMap::new(),
            calls: HashMap::new(),
        }
    }

    pub fn record(&mut self, id: HirId, ty: Ty) {
        self.ty.insert(id, ty);
    }

    pub fn record_def(&mut self, def: DefId, ty: Ty) {
        self.record(def.owner_id(), ty);
    }

    pub fn ty(&self, id: HirId) -> Option<Ty> {
        self.ty.get(&id).copied()
    }

    pub fn ty_of_def(&self, def: DefId) -> Option<Ty> {
        self.ty(def.owner_id())
    }

    pub fn iter(&self) -> impl Iterator<Item = (HirId, Ty)> + '_ {
        self.ty.iter().map(|(&id, &ty)| (id, ty))
    }

    /// Records that the `Call` or method-call `Access` expression `id` resolved to `def`,
    /// instantiated with `args`. `id` is the call expression's own id, not the callee's: a
    /// method call's callee (`Access`'s `member`) is a bare `Ident` with no `HirId` of its own,
    /// so the call expression is the only id both call shapes have in common to key this on.
    pub fn record_call(&mut self, id: HirId, def: DefId, args: Vec<Ty>) {
        self.calls.insert(id, ResolvedCall { def, args });
    }

    pub fn call(&self, id: HirId) -> Option<&ResolvedCall> {
        self.calls.get(&id)
    }

    /// Every recorded call, for [`Typeck::writeback`](crate::typeck::Typeck::writeback) to
    /// re-resolve and default each one's `args` the same way it does every plain [`Ty`] entry.
    pub fn calls_iter(&self) -> impl Iterator<Item = (HirId, &ResolvedCall)> + '_ {
        self.calls.iter().map(|(&id, call)| (id, call))
    }
}
