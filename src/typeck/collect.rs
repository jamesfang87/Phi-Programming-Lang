use std::collections::HashSet;

use crate::ast::{Mutability, SelfMode};
use crate::diagnostics::typeck::report_duplicate_field;
use crate::hir::visit::{self, Visitor};
use crate::hir::{DefId, Hir, HirId, TyKind as HirTyKind, VariantPayload};
use crate::typeck::Typeck;
use crate::typeck::ty::Ty;

impl<'hir> Typeck<'hir> {
    pub fn collect_module(&mut self, module_id: DefId) {
        SignatureCollector(self).visit_module(module_id);
    }

    pub fn collect_function(&mut self, function: DefId) {
        let hir: &'hir Hir = self.hir;
        let function_node = hir.function(function);
        let (generics, self_param, params, ret) = (
            &function_node.generics,
            function_node.self_param,
            &function_node.params,
            function_node.ret,
        );

        self.collect_generics(generics);

        let mut param_tys = Vec::with_capacity(params.len() + usize::from(self_param.is_some()));
        if let Some(id) = self_param {
            param_tys.push(self.collect_self_param(id));
        }

        for &id in params {
            let param = hir.param(id);

            let ty = self.lower_ty(param.ty);
            self.types.record(id, ty);
            self.check_no_dyn(ty, hir.ty(param.ty).span);
            param_tys.push(ty);
        }

        let ret = ret.map(|ret| {
            let ret_span = hir.ty(ret).span;
            let ret = self.lower_ty(ret);
            self.check_no_dyn(ret, ret_span);
            ret
        });

        let sig = self.tcx.mk_fun(param_tys, ret);
        self.types.record_def(function, sig);
    }

    fn collect_self_param(&mut self, id: impl Into<HirId>) -> Ty {
        let id = id.into();
        let self_param = self.hir.self_param(id);
        let (mode, span) = (self_param.mode, self_param.span);

        let self_ty = self.self_ty(id.owner, span);
        let ty = match mode {
            SelfMode::Immutable => self.tcx.mk_ref(self_ty, Mutability::Immutable),
            SelfMode::Mutable => self.tcx.mk_ref(self_ty, Mutability::Mutable),
            SelfMode::Move => self_ty,
            SelfMode::Any => self.tcx.mk_any(self_ty),
        };
        self.types.record(id, ty);
        ty
    }

    pub fn collect_struct(&mut self, r#struct: DefId) {
        let hir: &'hir Hir = self.hir;
        let struct_node = hir.struct_(r#struct);
        let (generics, fields, span) =
            (&struct_node.generics, &struct_node.fields, struct_node.span);

        self.collect_generics(generics);
        let self_ty = self.self_ty(r#struct, span);
        self.types.record_def(r#struct, self_ty);

        self.collect_fields(fields);
    }

    pub fn collect_enum(&mut self, r#enum: DefId) {
        let hir: &'hir Hir = self.hir;
        let enum_node = hir.enum_(r#enum);
        let (generics, variants, span) = (&enum_node.generics, &enum_node.variants, enum_node.span);

        self.collect_generics(generics);
        let self_ty = self.self_ty(r#enum, span);
        self.types.record_def(r#enum, self_ty);

        for &id in variants {
            let variant = hir.variant(id);

            match &variant.payload {
                VariantPayload::Unit => {}
                VariantPayload::Type(ty_id) => {
                    let payload_span = hir.ty(*ty_id).span;
                    let ty = self.lower_ty(*ty_id);
                    self.types.record(id, ty);
                    self.check_not_a_reference(ty, payload_span);
                    self.check_not_any(ty, payload_span);
                    self.check_no_dyn(ty, payload_span);
                }
                VariantPayload::Record(fields) => self.collect_fields(fields),
            }
        }
    }

    pub fn collect_trait(&mut self, r#trait: DefId) {
        let hir: &'hir Hir = self.hir;
        let trait_node = hir.trait_(r#trait);
        let (generics, span) = (&trait_node.generics, trait_node.span);

        self.collect_generics(generics);
        let self_ty = self.self_ty(r#trait, span);
        self.types.record_def(r#trait, self_ty);
    }

    pub fn collect_extend(&mut self, extend: DefId) {
        let hir: &'hir Hir = self.hir;
        let extend_node = hir.extend(extend);
        let (extend_generics, self_ty_id, trait_generics, span) = (
            &extend_node.extend_generics,
            extend_node.self_ty,
            &extend_node.trait_generics,
            extend_node.span,
        );

        self.collect_generics(extend_generics);
        match &self.hir.ty(self_ty_id).kind {
            HirTyKind::Path { args, .. } => {
                let args = args.clone();
                self.lower_tys(&args);
            }
            _ => {
                self.lower_ty(self_ty_id);
            }
        }
        self.lower_tys(trait_generics);

        let self_ty = self.self_ty(extend, span);
        self.types.record_def(extend, self_ty);
    }

    fn collect_generics(&mut self, generics: &[HirId]) {
        for &id in generics {
            let ty = self.tcx.mk_generic(id);
            self.types.record(id, ty);
        }
    }

    fn collect_fields(&mut self, fields: &[HirId]) {
        let mut seen = HashSet::new();
        for &id in fields {
            let field = self.hir.field(id);
            let field_span = field.span;
            if !seen.insert(field.name.text) {
                report_duplicate_field(self.display_cx(), field.name);
            }

            let ty = self.lower_ty(field.ty);
            self.types.record(id, ty);
            self.check_not_a_reference(ty, field_span);
            self.check_not_any(ty, field_span);
            self.check_no_dyn(ty, field_span);
        }
    }
}

struct SignatureCollector<'a, 'hir>(&'a mut Typeck<'hir>);

impl<'hir> Visitor<'hir> for SignatureCollector<'_, 'hir> {
    fn hir(&self) -> &'hir Hir {
        self.0.hir
    }

    fn visit_nested_owner(&mut self, def_id: DefId) {
        visit::walk_item(self, def_id);
    }

    fn visit_function(&mut self, def_id: DefId) {
        self.0.collect_function(def_id);
    }

    fn visit_struct(&mut self, def_id: DefId) {
        self.0.collect_struct(def_id);
    }

    fn visit_enum(&mut self, def_id: DefId) {
        self.0.collect_enum(def_id);
    }

    fn visit_trait(&mut self, def_id: DefId) {
        self.0.collect_trait(def_id);
        visit::walk_trait(self, def_id);
    }

    fn visit_extend(&mut self, def_id: DefId) {
        self.0.collect_extend(def_id);
        visit::walk_extend(self, def_id);
    }

    fn visit_closure(&mut self, def_id: DefId) {
        unreachable!("stage one reached a closure ({def_id:?}), which owns no signature to collect")
    }
}
