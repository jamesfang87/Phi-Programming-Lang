use std::collections::HashMap;

use crate::ast::Mutability;
use crate::hir::{DefId, HirId};
use crate::nameres::PrimTy;
use crate::typeck::ty::{Ty, TyKind, TyVar};

#[derive(Default)]
pub struct TyCtx {
    tykinds: Vec<TyKind>,
    handles: HashMap<TyKind, Ty>,
    /// The id the next inference variable is issued, incremented each time one is created.
    next_var: u32,
}

impl TyCtx {
    pub fn new() -> Self {
        TyCtx::default()
    }

    /// Returns the handle for `kind`, interning it if this is the first time it has been seen.
    pub fn intern(&mut self, kind: TyKind) -> Ty {
        if let Some(&ty) = self.handles.get(&kind) {
            return ty;
        }

        let ty = Ty::from_usize(self.tykinds.len());
        self.tykinds.push(kind.clone());
        self.handles.insert(kind, ty);
        ty
    }

    /// Looks up what `ty` stands for.
    ///
    /// Panics if `ty` was interned by a different [`TyCtx`].
    pub fn kind(&self, ty: Ty) -> &TyKind {
        self.tykinds
            .get(ty.index())
            .expect("a Ty handle from another TyCtx (or one built by hand)")
    }

    pub fn error(&mut self) -> Ty {
        self.intern(TyKind::Error)
    }

    pub fn never(&mut self) -> Ty {
        self.intern(TyKind::Never)
    }

    pub fn unit(&mut self) -> Ty {
        self.intern(TyKind::Unit)
    }

    pub fn mk_prim(&mut self, prim: PrimTy) -> Ty {
        self.intern(TyKind::Primitive(prim))
    }

    pub fn mk_adt(&mut self, def: DefId, args: Vec<Ty>) -> Ty {
        self.intern(TyKind::Adt { def, args })
    }

    pub fn mk_generic(&mut self, param: HirId) -> Ty {
        self.intern(TyKind::Generic(param))
    }

    pub fn mk_self_param(&mut self, trait_: DefId) -> Ty {
        self.intern(TyKind::SelfTy(trait_))
    }

    pub fn mk_ref(&mut self, base: Ty, mutability: Mutability) -> Ty {
        self.intern(TyKind::Ref { base, mutability })
    }

    pub fn mk_any(&mut self, base: Ty) -> Ty {
        self.intern(TyKind::Any(base))
    }

    pub fn mk_tuple(&mut self, elems: Vec<Ty>) -> Ty {
        self.intern(TyKind::Tuple(elems))
    }

    pub fn mk_array(&mut self, elem: Ty, len: Option<HirId>) -> Ty {
        self.intern(TyKind::Array { elem, len })
    }

    pub fn mk_fun(&mut self, params: Vec<Ty>, ret: Option<Ty>) -> Ty {
        self.intern(TyKind::Fun { params, ret })
    }

    pub fn mk_dyn(&mut self, trait_: DefId, args: Vec<Ty>) -> Ty {
        self.intern(TyKind::Dyn { trait_, args })
    }

    pub fn next_ty_var(&mut self) -> Ty {
        let var = TyVar::Any(self.take_var_id());
        self.intern(TyKind::Var(var))
    }

    pub fn next_int_var(&mut self) -> Ty {
        let var = TyVar::Int(self.take_var_id());
        self.intern(TyKind::Var(var))
    }

    pub fn next_float_var(&mut self) -> Ty {
        let var = TyVar::Float(self.take_var_id());
        self.intern(TyKind::Var(var))
    }

    fn take_var_id(&mut self) -> u32 {
        let id = self.next_var;
        self.next_var += 1;
        id
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn structurally_equal_types_intern_to_the_same_handle() {
        let mut tcx = TyCtx::new();
        let a = tcx.mk_prim(PrimTy::I32);
        let b = tcx.mk_prim(PrimTy::I32);
        assert_eq!(a, b);

        let ref_a = tcx.mk_ref(a, Mutability::Immutable);
        let ref_b = tcx.mk_ref(b, Mutability::Immutable);
        assert_eq!(ref_a, ref_b);
    }

    #[test]
    fn types_that_differ_intern_to_different_handles() {
        let mut tcx = TyCtx::new();
        let i32_ty = tcx.mk_prim(PrimTy::I32);
        let i64_ty = tcx.mk_prim(PrimTy::I64);
        assert_ne!(i32_ty, i64_ty);

        let shared = tcx.mk_ref(i32_ty, Mutability::Immutable);
        let unique = tcx.mk_ref(i32_ty, Mutability::Mutable);
        assert_ne!(shared, unique);
    }

    #[test]
    fn a_handle_looks_its_own_kind_back_up() {
        let mut tcx = TyCtx::new();
        let elem = tcx.mk_prim(PrimTy::Bool);
        let tuple = tcx.mk_tuple(vec![elem, elem]);

        let TyKind::Tuple(elems) = tcx.kind(tuple) else {
            panic!("a tuple type interns as TyKind::Tuple");
        };
        assert_eq!(elems, &[elem, elem]);
    }

    #[test]
    fn every_inference_variable_is_distinct() {
        let mut tcx = TyCtx::new();
        let vars = [
            tcx.next_ty_var(),
            tcx.next_ty_var(),
            tcx.next_int_var(),
            tcx.next_float_var(),
        ];

        for (i, &a) in vars.iter().enumerate() {
            for &b in &vars[i + 1..] {
                assert_ne!(a, b);
            }
        }
    }

    #[test]
    fn two_contexts_number_their_variables_independently() {
        let mut first = TyCtx::new();
        let mut second = TyCtx::new();
        assert_eq!(first.next_ty_var(), second.next_ty_var());
    }
}
