use std::collections::{HashMap, HashSet};
use std::ops::ControlFlow;

use crate::ast::Mutability;
use crate::hir::{DefId, HirId};
use crate::nameres::PrimTy;
use crate::typeck::ty::adt::AdtDef;
use crate::typeck::ty::visitor::{self, TypeVisitor};
use crate::typeck::ty::{InferVar, Ty, TyKind};

#[derive(Default)]
pub struct TyCtx {
    tykinds: Vec<TyKind>,
    handles: HashMap<TyKind, Ty>,
    vars: InferVarSupply,
    /// The ADTs collected by [`crate::typeck::ty::adt::collect_adt_defs`], looked up for their
    /// field types and variant counts.
    adts: HashMap<DefId, AdtDef>,
    copy: HashSet<Ty>,
}

/// Hands out the ids of inference variables, which are unique within one [`TyCtx`].
#[derive(Default)]
struct InferVarSupply {
    next_id: u32,
}

impl InferVarSupply {
    fn fresh(&mut self) -> u32 {
        let id = self.next_id;
        self.next_id += 1;
        id
    }
}

impl TyCtx {
    pub fn new() -> Self {
        TyCtx::default()
    }

    /// Returns the handle for `kind`; if [`kind`] does not exist, it is added to the table
    pub fn intern(&mut self, kind: TyKind) -> Ty {
        if let Some(&ty) = self.handles.get(&kind) {
            return ty;
        }

        let ty = Ty::from_usize(self.tykinds.len());
        self.tykinds.push(kind.clone());
        self.handles.insert(kind, ty);
        ty
    }

    /// Looks up [`TyKind`] which ty` refers to.
    ///
    /// Panics if `ty` does not refer to any [`TyKind`]
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

    pub fn mk_iso(&mut self, base: Ty) -> Ty {
        self.intern(TyKind::Iso(base))
    }

    pub fn mk_tuple(&mut self, elems: Vec<Ty>) -> Ty {
        self.intern(TyKind::Tuple(elems))
    }

    pub fn mk_array(&mut self, elem: Ty, len: Option<u64>) -> Ty {
        self.intern(TyKind::Array { elem, len })
    }

    pub fn mk_fun(&mut self, params: Vec<Ty>, ret: Option<Ty>) -> Ty {
        self.intern(TyKind::Fun { params, ret })
    }

    pub fn mk_dyn(&mut self, trait_: DefId, args: Vec<Ty>) -> Ty {
        self.intern(TyKind::Dyn { trait_, args })
    }

    pub fn enum_variant_count(&self, def: DefId) -> Option<usize> {
        match self.adt(def) {
            AdtDef::Struct { .. } => None,
            AdtDef::Enum { variants, .. } => Some(variants.len()),
        }
    }

    pub fn struct_field_tys(&mut self, def: DefId, args: &[Ty]) -> Vec<Ty> {
        let AdtDef::Struct { generics, fields } = self.adt(def) else {
            panic!("struct_field_tys: {def:?} is an enum, not a struct");
        };
        let (generics, fields) = (generics.clone(), fields.clone());
        self.subst_declared_tys(&generics, &fields, args)
    }

    pub fn variant_field_tys(&mut self, def: DefId, args: &[Ty], variant: usize) -> Vec<Ty> {
        let AdtDef::Enum { generics, variants } = self.adt(def) else {
            panic!("variant_field_tys: {def:?} is a struct, not an enum");
        };
        let (generics, fields) = (generics.clone(), variants[variant].clone());
        self.subst_declared_tys(&generics, &fields, args)
    }

    pub fn needs_drop(&mut self, ty: Ty) -> bool {
        match self.kind(ty).clone() {
            TyKind::Iso(_) => true,
            TyKind::Fun { .. } => true,
            TyKind::Array { elem, .. } => self.needs_drop(elem),
            TyKind::Tuple(elems) => elems.iter().any(|&elem| self.needs_drop(elem)),
            TyKind::Adt { def, args } => match self.enum_variant_count(def) {
                None => self
                    .struct_field_tys(def, &args)
                    .into_iter()
                    .any(|field| self.needs_drop(field)),
                Some(variant_count) => (0..variant_count).any(|variant| {
                    self.variant_field_tys(def, &args, variant)
                        .into_iter()
                        .any(|field| self.needs_drop(field))
                }),
            },
            _ => false,
        }
    }

    fn adt(&self, def: DefId) -> &AdtDef {
        self.adts
            .get(&def)
            .unwrap_or_else(|| panic!("{def:?} does not name an ADT"))
    }

    fn subst_declared_tys(&mut self, generics: &[HirId], declared: &[Ty], args: &[Ty]) -> Vec<Ty> {
        let subst = crate::typeck::ty::visitor::Subst {
            generics: generics.iter().copied().zip(args.iter().copied()).collect(),
            self_ty: None,
        };
        declared
            .iter()
            .map(|&ty| crate::typeck::ty::visitor::subst_ty(self, ty, &subst))
            .collect()
    }

    pub(crate) fn set_adts(&mut self, adts: HashMap<DefId, AdtDef>) {
        assert!(
            self.adts.is_empty(),
            "Attempt to recollect ADT definitions (which are only to be collected once)"
        );
        self.adts = adts;
    }

    /// Records `ty` as implementing `Copy`, so a read of a place of that type may copy it rather
    /// than move out of it.
    pub fn mark_copy(&mut self, ty: Ty) {
        self.copy.insert(ty);
    }

    /// Returns whether a value of `ty` may be read by copying it. The trait solver records every
    /// type it proves `Copy` -- including the primitives and shared references `core` implements
    /// the trait for -- through [`TyCtx::mark_copy`].
    pub fn is_copy(&self, ty: Ty) -> bool {
        self.copy.contains(&ty)
    }

    pub fn contains_ref(&self, ty: Ty) -> bool {
        visitor::any_ty_outside_funs(self, ty, |tcx, ty| {
            matches!(
                tcx.kind(ty),
                TyKind::Ref { .. } | TyKind::Primitive(PrimTy::Str)
            )
        })
    }

    pub fn contains_bare_dyn(&self, ty: Ty) -> bool {
        struct BareDyn;

        impl TypeVisitor for BareDyn {
            type Output = ();

            fn visit(&mut self, tcx: &TyCtx, ty: Ty) -> ControlFlow<()> {
                if matches!(tcx.kind(ty), TyKind::Dyn { .. }) {
                    ControlFlow::Break(())
                } else {
                    ControlFlow::Continue(())
                }
            }

            fn children(&mut self, tcx: &TyCtx, ty: Ty) -> Vec<Ty> {
                match tcx.kind(ty) {
                    TyKind::Fun { .. } => Vec::new(),
                    TyKind::Ref { base, .. } | TyKind::Iso(base)
                        if matches!(tcx.kind(*base), TyKind::Dyn { .. }) =>
                    {
                        Vec::new()
                    }
                    _ => visitor::children(tcx, ty),
                }
            }
        }

        visitor::walk(&mut BareDyn, self, ty).is_break()
    }

    pub fn contains_any(&self, ty: Ty) -> bool {
        visitor::any_ty_outside_funs(self, ty, |tcx, ty| matches!(tcx.kind(ty), TyKind::Any(_)))
    }

    pub fn next_infer_var(&mut self) -> Ty {
        let var = InferVar::Any(self.vars.fresh());
        self.intern(TyKind::Var(var))
    }

    pub fn next_int_var(&mut self) -> Ty {
        let var = InferVar::Int(self.vars.fresh());
        self.intern(TyKind::Var(var))
    }

    pub fn next_float_var(&mut self) -> Ty {
        let var = InferVar::Float(self.vars.fresh());
        self.intern(TyKind::Var(var))
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
            tcx.next_infer_var(),
            tcx.next_infer_var(),
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
        assert_eq!(first.next_infer_var(), second.next_infer_var());
    }
}
