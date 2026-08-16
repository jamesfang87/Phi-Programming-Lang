use std::collections::HashMap;

use crate::hir::{DefId, HirId};
use crate::typeck::ty::Ty;

// Stores what a call resolves to
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

    pub fn tys_iter(&self) -> impl Iterator<Item = (HirId, Ty)> + '_ {
        self.ty.iter().map(|(&id, &ty)| (id, ty))
    }

    pub fn record_call(&mut self, id: HirId, def: DefId, args: Vec<Ty>) {
        self.calls.insert(id, ResolvedCall { def, args });
    }

    pub fn call(&self, id: HirId) -> Option<&ResolvedCall> {
        self.calls.get(&id)
    }

    pub fn calls_iter(&self) -> impl Iterator<Item = (HirId, &ResolvedCall)> + '_ {
        self.calls.iter().map(|(&id, call)| (id, call))
    }
}
