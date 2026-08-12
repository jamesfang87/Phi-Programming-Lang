use std::collections::HashMap;

use crate::hir::{DefId, HirId};
use crate::typeck::ty::Ty;

#[derive(Default)]
pub struct TypeResolutions {
    ty: HashMap<HirId, Ty>,
}

impl TypeResolutions {
    pub fn new() -> TypeResolutions {
        TypeResolutions { ty: HashMap::new() }
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
}
