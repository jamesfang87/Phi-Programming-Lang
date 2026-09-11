use std::collections::HashMap;

use crate::hir::{BindingMode, DefId, HirId};
use crate::typeck::ty::Ty;

/// What a call resolves to.
#[derive(Clone, Debug)]
pub struct ResolvedCall {
    pub def: DefId,
    pub args: Vec<Ty>,
    pub extend_args: Vec<Ty>,
    pub self_ty: Option<Ty>,
}

impl ResolvedCall {
    pub fn all_args(&self) -> Vec<Ty> {
        let mut all = self.extend_args.clone();
        all.extend(self.args.iter().copied());
        all
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum DerefMode {
    Copy,
    Move,
}

#[derive(Clone, Copy, Debug)]
pub struct PatAdjust {
    pub derefs: u32,
    pub mode: BindingMode,
}

#[derive(Default)]
pub struct TypeResolutions {
    ty: HashMap<HirId, Ty>,
    calls: HashMap<HirId, ResolvedCall>,
    derefs: HashMap<HirId, DerefMode>,
    pat_adjusts: HashMap<HirId, PatAdjust>,
    unsizes: HashMap<HirId, Ty>,
}

impl TypeResolutions {
    pub fn new() -> TypeResolutions {
        TypeResolutions::default()
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
        self.record_method_call(id, def, args, Vec::new(), None);
    }

    pub fn record_method_call(
        &mut self,
        id: HirId,
        def: DefId,
        args: Vec<Ty>,
        extend_args: Vec<Ty>,
        self_ty: Option<Ty>,
    ) {
        self.calls.insert(
            id,
            ResolvedCall {
                def,
                args,
                extend_args,
                self_ty,
            },
        );
    }

    pub fn call(&self, id: HirId) -> Option<&ResolvedCall> {
        self.calls.get(&id)
    }

    pub fn calls_iter(&self) -> impl Iterator<Item = (HirId, &ResolvedCall)> + '_ {
        self.calls.iter().map(|(&id, call)| (id, call))
    }

    pub fn record_pat_adjust(&mut self, id: HirId, adjust: PatAdjust) {
        self.pat_adjusts.insert(id, adjust);
    }

    pub fn pat_adjust(&self, id: HirId) -> PatAdjust {
        self.pat_adjusts.get(&id).copied().unwrap_or(PatAdjust {
            derefs: 0,
            mode: BindingMode::Value,
        })
    }

    pub fn record_deref(&mut self, id: HirId, mode: DerefMode) {
        self.derefs.insert(id, mode);
    }

    pub fn deref_mode(&self, id: HirId) -> DerefMode {
        self.derefs[&id]
    }

    pub fn record_unsize(&mut self, id: HirId, target: Ty) {
        self.unsizes.insert(id, target);
    }

    pub fn unsize(&self, id: HirId) -> Option<Ty> {
        self.unsizes.get(&id).copied()
    }
}
