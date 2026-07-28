//! Lowers the small item-level building blocks every owner is made of: generics, parameters,
//! fields, variants, and imports.

use crate::ast;
use crate::hir::ids::LocalId;
use crate::hir::lower::owner::OwnerLowerer;
use crate::hir::{
    ClosureParam, Field, Generic, Import, Node, Param, SelfParam, Variant as HirVariant,
    VariantPayload,
};

impl OwnerLowerer<'_> {
    pub(super) fn lower_generics(&mut self, generics: &[ast::Generic]) -> Vec<LocalId> {
        generics.iter().map(|g| self.lower_generic(g)).collect()
    }

    fn lower_generic(&mut self, g: &ast::Generic) -> LocalId {
        let id = self.reserve();
        let bounds = g.bounds.clone().unwrap_or_default();
        let hir_id = self.hir_id(id);
        self.fill(
            id,
            Node::Generic(Generic {
                hir_id,
                name: g.name,
                bounds,
                span: g.span,
            }),
        );
        id
    }

    pub(super) fn lower_self_param(&mut self, sp: &ast::SelfParam) -> LocalId {
        let id = self.reserve();
        let hir_id = self.hir_id(id);
        self.fill(
            id,
            Node::SelfParam(SelfParam {
                hir_id,
                mode: sp.mode,
                span: sp.span,
            }),
        );
        id
    }

    pub(super) fn lower_param(&mut self, p: &ast::Param) -> LocalId {
        let id = self.reserve();
        let ty = self.lower_ty(&p.ty);
        let hir_id = self.hir_id(id);
        self.fill(
            id,
            Node::Param(Param {
                hir_id,
                name: p.name,
                ty,
                span: p.span,
            }),
        );
        id
    }

    pub(super) fn lower_closure_param(&mut self, p: &ast::ClosureParam) -> LocalId {
        let id = self.reserve();
        let ty = p.ty.as_ref().map(|t| self.lower_ty(t));
        let hir_id = self.hir_id(id);
        self.fill(
            id,
            Node::ClosureParam(ClosureParam {
                hir_id,
                name: p.name,
                ty,
                span: p.span,
            }),
        );
        id
    }

    pub(super) fn lower_field(&mut self, f: &ast::Field) -> LocalId {
        let id = self.reserve();
        let ty = self.lower_ty(&f.ty);
        let hir_id = self.hir_id(id);
        self.fill(
            id,
            Node::Field(Field {
                hir_id,
                name: f.name,
                ty,
                visibility: f.visibility,
                span: f.span,
            }),
        );
        id
    }

    pub(super) fn lower_variant(&mut self, v: &ast::Variant) -> LocalId {
        let id = self.reserve();
        let payload = match &v.payload {
            ast::VariantPayload::Unit => VariantPayload::Unit,
            ast::VariantPayload::Type(ty) => VariantPayload::Type(self.lower_ty(ty)),
            ast::VariantPayload::Record(fields) => {
                VariantPayload::Record(fields.iter().map(|f| self.lower_field(f)).collect())
            }
        };
        let hir_id = self.hir_id(id);
        self.fill(
            id,
            Node::Variant(HirVariant {
                hir_id,
                name: v.name,
                payload,
                span: v.span,
            }),
        );
        id
    }

    pub(super) fn lower_import(&mut self, imp: &ast::Import) -> LocalId {
        let id = self.reserve();
        let hir_id = self.hir_id(id);
        self.fill(
            id,
            Node::Import(Import {
                hir_id,
                path: imp.path.clone(),
                glob: imp.glob,
                alias: imp.alias,
                span: imp.span,
            }),
        );
        id
    }
}
