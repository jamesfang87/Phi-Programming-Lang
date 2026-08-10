//! [`TyCtx`] is the type checker's arena: it owns every [`TyKind`] in the program and hands out
//! the [`Ty`] handles that address them.
//!
//! Interning is what makes those handles work. A `TyKind` is only ever stored once, so two types
//! that are structurally equal always come back as the same `Ty`, and comparing types is an
//! integer comparison instead of a recursive walk. It also bounds memory: a deeply nested type
//! shares storage with every type that repeats one of its components.
//!
//! A `TyCtx` is created per compilation and passed to whoever needs it. It is deliberately not a
//! global: the handles it hands out are indices into its own storage and mean nothing anywhere
//! else, so two contexts existing at once (two tests running in parallel, say) silently
//! reinterpreting each other's handles is exactly the failure a singleton would invite. Owning
//! it also puts the inference variable counter somewhere that resets with the compilation
//! instead of accumulating for the life of the process.

use std::collections::HashMap;

use crate::ast::Mutability;
use crate::hir::{DefId, HirId};
use crate::nameres::PrimTy;
use crate::typeck::ty::{Ty, TyKind, TyVar};

pub struct TyCtx {
    /// Every interned type, indexed by its own [`Ty`] handle.
    kinds: Vec<TyKind>,

    /// The reverse of [`TyCtx::kinds`], so an already-interned type is found instead of stored
    /// a second time.
    interned: HashMap<TyKind, Ty>,

    /// Counts the inference variables handed out so far. One counter serves all three kinds of
    /// [`TyVar`], so no two variables ever share an id.
    next_var: u32,

    /// [`TyKind::Error`], interned once up front so it can be handed out without a `&mut self`.
    /// Error recovery reaches for it constantly, often from a position that only holds a shared
    /// borrow.
    error: Ty,

    /// [`TyKind::Never`], interned up front for the same reason as [`TyCtx::error`].
    never: Ty,

    /// [`TyKind::Unit`], interned up front for the same reason as [`TyCtx::error`]. A function
    /// with no declared return type reaches for this on every `collect_function` call, which is
    /// often enough to be worth not re-interning.
    unit: Ty,
}

impl TyCtx {
    pub fn new() -> Self {
        let mut tcx = TyCtx {
            kinds: Vec::new(),
            interned: HashMap::new(),
            next_var: 0,
            // Placeholders: `intern` needs the context to exist before it can be called, so the
            // cached handles are filled in immediately below.
            error: Ty::from_usize(0),
            never: Ty::from_usize(0),
            unit: Ty::from_usize(0),
        };
        tcx.error = tcx.intern(TyKind::Error);
        tcx.never = tcx.intern(TyKind::Never);
        tcx.unit = tcx.intern(TyKind::Unit);
        tcx
    }

    /// Returns the handle for `kind`, interning it if this is the first time it has been seen.
    pub fn intern(&mut self, kind: TyKind) -> Ty {
        if let Some(&ty) = self.interned.get(&kind) {
            return ty;
        }

        let ty = Ty::from_usize(self.kinds.len());
        self.kinds.push(kind.clone());
        self.interned.insert(kind, ty);
        ty
    }

    /// Looks up what `ty` stands for.
    ///
    /// Panics if `ty` was interned by a different [`TyCtx`], which is a bug at the call site --
    /// handles are only meaningful paired with the context that produced them.
    pub fn kind(&self, ty: Ty) -> &TyKind {
        self.kinds
            .get(ty.index())
            .expect("a Ty handle from another TyCtx (or one built by hand)")
    }

    pub fn error(&self) -> Ty {
        self.error
    }

    pub fn never(&self) -> Ty {
        self.never
    }

    pub fn unit(&self) -> Ty {
        self.unit
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

    /// Hands out a fresh inference variable of the given kind, along with the type standing for
    /// it. Each call returns a variable distinct from every other, so callers never have to
    /// coordinate ids.
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

impl Default for TyCtx {
    fn default() -> Self {
        Self::new()
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
