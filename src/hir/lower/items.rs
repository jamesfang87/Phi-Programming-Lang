//! Lowers the small item-level building blocks every owner is made of: generics, parameters,
//! fields, variants, and imports.

use crate::ast;
use crate::hir::lower::owner::OwnerLowerer;
use crate::hir::{
    ClosureParam, Field, Generic, HirId, Import, Node, Param, SelfParam, Variant as HirVariant,
    VariantPayload,
};

impl OwnerLowerer<'_, '_> {
    pub(super) fn lower_generics(&mut self, generics: &[ast::Generic]) -> Vec<HirId> {
        generics.iter().map(|g| self.lower_generic(g)).collect()
    }

    fn lower_generic(&mut self, g: &ast::Generic) -> HirId {
        let hir_id = self.reserve();
        self.cx.record_hir_id(g.id, hir_id);
        let bounds = g
            .bounds
            .as_deref()
            .unwrap_or(&[])
            .iter()
            .map(|bound| self.cx.lower_path(g.id, bound))
            .collect();
        self.fill(
            hir_id,
            Node::Generic(Generic {
                hir_id,
                name: g.name,
                bounds,
                span: g.span,
            }),
        );
        hir_id
    }

    pub(super) fn lower_self_param(&mut self, sp: &ast::SelfParam) -> HirId {
        let hir_id = self.reserve();
        self.cx.record_hir_id(sp.id, hir_id);
        self.fill(
            hir_id,
            Node::SelfParam(SelfParam {
                hir_id,
                mode: sp.mode,
                span: sp.span,
            }),
        );
        hir_id
    }

    pub(super) fn lower_param(&mut self, p: &ast::Param) -> HirId {
        let hir_id = self.reserve();
        self.cx.record_hir_id(p.id, hir_id);
        let ty = self.lower_ty(&p.ty);
        self.fill(
            hir_id,
            Node::Param(Param {
                hir_id,
                name: p.name,
                ty,
                span: p.span,
            }),
        );
        hir_id
    }

    pub(super) fn lower_closure_param(&mut self, p: &ast::ClosureParam) -> HirId {
        let hir_id = self.reserve();
        self.cx.record_hir_id(p.id, hir_id);
        let ty = p.ty.as_ref().map(|t| self.lower_ty(t));
        self.fill(
            hir_id,
            Node::ClosureParam(ClosureParam {
                hir_id,
                name: p.name,
                ty,
                span: p.span,
            }),
        );
        hir_id
    }

    pub(super) fn lower_field(&mut self, f: &ast::Field) -> HirId {
        let hir_id = self.reserve();
        let ty = self.lower_ty(&f.ty);
        self.fill(
            hir_id,
            Node::Field(Field {
                hir_id,
                name: f.name,
                ty,
                visibility: f.visibility,
                span: f.span,
            }),
        );
        hir_id
    }

    pub(super) fn lower_variant(&mut self, v: &ast::Variant) -> HirId {
        let hir_id = self.reserve();
        let payload = match &v.payload {
            ast::VariantPayload::Unit => VariantPayload::Unit,
            ast::VariantPayload::Type(ty) => VariantPayload::Type(self.lower_ty(ty)),
            ast::VariantPayload::Record(fields) => {
                VariantPayload::Record(fields.iter().map(|f| self.lower_field(f)).collect())
            }
        };
        self.fill(
            hir_id,
            Node::Variant(HirVariant {
                hir_id,
                name: v.name,
                payload,
                span: v.span,
            }),
        );
        hir_id
    }

    pub(super) fn lower_import(&mut self, imp: &ast::Import) -> HirId {
        let hir_id = self.reserve();
        self.fill(
            hir_id,
            Node::Import(Import {
                hir_id,
                path: imp.path.clone(),
                glob: imp.glob,
                alias: imp.alias,
                span: imp.span,
            }),
        );
        hir_id
    }
}
