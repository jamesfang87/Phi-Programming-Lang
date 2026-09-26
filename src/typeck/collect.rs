use std::collections::HashSet;

use crate::ast::{Mutability, SelfMode};
use crate::diagnostics::typeck::{
    report_any_outside_signature, report_duplicate_field, report_reference_field,
};
use crate::driver::source::SrcSpan;
use crate::hir::visit::{self, Visitor};
use crate::hir::{DefId, Hir, HirId, TyId, TyKind as HirTyKind, VariantPayload};
use crate::typeck::Typeck;
use crate::typeck::ty::Ty;

impl<'hir> Typeck<'hir> {
    pub fn collect_module(&mut self, module_id: DefId) {
        SignatureCollector(self).visit_module(module_id);
    }

    pub fn collect_function(&mut self, function: DefId) {
        let function_node = self.hir.function(function);
        self.collect_generics(&function_node.generics);

        let mut params = Vec::with_capacity(
            function_node.params.len() + usize::from(function_node.self_param.is_some()),
        );
        if let Some(self_param) = function_node.self_param {
            params.push(self.collect_self_param(self_param));
        }
        params.extend(
            function_node
                .params
                .iter()
                .map(|&param| self.collect_param(param)),
        );
        let ret = function_node.ret.map(|ret| self.collect_return(ret));

        let sig = self.tcx.mk_fun(params, ret);
        self.types.record_def(function, sig);
    }

    fn collect_param(&mut self, param: HirId) -> Ty {
        let ty_id = self.hir.param(param).ty;
        let ty = self.lower_ty(ty_id);
        self.types.record(param, ty);
        self.check_no_dyn(ty, self.hir.ty(ty_id).span);
        ty
    }

    fn collect_return(&mut self, ty_id: TyId) -> Ty {
        let ty = self.lower_ty(ty_id);
        self.check_no_dyn(ty, self.hir.ty(ty_id).span);
        ty
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
        self.self_ty(r#struct, span);

        self.collect_fields(fields);
    }

    pub fn collect_enum(&mut self, r#enum: DefId) {
        let hir: &'hir Hir = self.hir;
        let enum_node = hir.enum_(r#enum);
        self.collect_generics(&enum_node.generics);
        self.self_ty(r#enum, enum_node.span);

        for &variant in &enum_node.variants {
            self.collect_variant(variant);
        }
    }

    fn collect_variant(&mut self, variant: HirId) {
        let hir: &'hir Hir = self.hir;
        match &hir.variant(variant).payload {
            VariantPayload::Unit => {}
            VariantPayload::Type(ty_id) => self.collect_payload_type(variant, *ty_id),
            VariantPayload::Record(fields) => self.collect_fields(fields),
        }
    }

    fn collect_payload_type(&mut self, variant: HirId, ty_id: TyId) {
        let span = self.hir.ty(ty_id).span;
        let ty = self.lower_ty(ty_id);
        self.types.record(variant, ty);
        self.check_not_a_reference(ty, span);
        self.check_not_any(ty, span);
        self.check_no_dyn(ty, span);
    }

    pub fn collect_trait(&mut self, r#trait: DefId) {
        let hir: &'hir Hir = self.hir;
        let trait_node = hir.trait_(r#trait);
        let (generics, span) = (&trait_node.generics, trait_node.span);

        self.collect_generics(generics);
        self.self_ty(r#trait, span);
    }

    pub fn collect_extend(&mut self, extend: DefId) {
        let hir: &'hir Hir = self.hir;
        let extend_node = hir.extend(extend);
        self.collect_generics(&extend_node.extend_generics);
        self.lower_extend_self_ty(extend_node.self_ty);
        self.lower_tys(&extend_node.trait_generics);
        self.self_ty(extend, extend_node.span);
    }

    fn lower_extend_self_ty(&mut self, self_ty: TyId) {
        match &self.hir.ty(self_ty).kind {
            HirTyKind::Path { args, .. } => {
                let args = args.clone();
                self.lower_tys(&args);
            }
            _ => {
                self.lower_ty(self_ty);
            }
        }
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

    fn check_not_a_reference(&mut self, ty: Ty, span: SrcSpan) {
        if self.tcx.contains_ref(ty) {
            report_reference_field(self.display_cx(), ty, span);
        }
    }

    pub(crate) fn check_not_any(&mut self, ty: Ty, span: SrcSpan) {
        if self.tcx.contains_any(ty) {
            report_any_outside_signature(self.display_cx(), ty, span);
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

#[cfg(test)]
mod tests {
    use crate::testing::{typeck_accepts as accepts, typeck_rejects as rejects};

    #[test]
    fn a_struct_field_that_is_a_reference_is_rejected() {
        rejects(
            "struct Node { next: &Node }",
            "a field cannot hold a reference",
        );
    }

    #[test]
    fn an_enum_variant_payload_that_is_a_reference_is_rejected() {
        rejects(
            "enum List { cons: &List, nil }",
            "a field cannot hold a reference",
        );
    }

    #[test]
    fn an_enum_record_variant_field_that_is_a_reference_is_rejected() {
        rejects(
            "enum List { cons: { next: &List }, nil }",
            "a field cannot hold a reference",
        );
    }

    #[test]
    fn a_tuple_field_containing_a_reference_is_rejected() {
        rejects(
            "struct Pair { both: (i32, &i32) }",
            "a field cannot hold a reference",
        );
    }

    #[test]
    fn an_array_field_containing_a_reference_is_rejected() {
        rejects(
            "struct Bucket { items: [&i32; 3] }",
            "a field cannot hold a reference",
        );
    }

    #[test]
    fn instantiating_a_generic_struct_with_a_reference_argument_in_a_field_is_rejected() {
        rejects(
            "struct Boxed<T> { value: T }
             struct Outer { boxed: Boxed<&i32> }",
            "a field cannot hold a reference",
        );
    }

    #[test]
    fn instantiating_a_generic_struct_with_a_reference_argument_as_a_parameter_checks() {
        accepts(
            "struct Boxed<T> { value: T }
             fun f(b: Boxed<&i32>) {}",
        );
    }

    #[test]
    fn instantiating_a_generic_struct_with_a_reference_argument_as_a_return_type_checks() {
        accepts(
            "struct Boxed<T> { value: T }
             fun f(b: &i32) -> Boxed<&i32> { return Boxed { value: b }; }",
        );
    }

    #[test]
    fn extending_a_generic_struct_with_a_reference_argument_is_rejected() {
        rejects(
            "struct Boxed<T> { value: T }
             extend Boxed<&i32> { fun get(&self) {} }",
            "a struct or enum cannot be instantiated with a reference",
        );
    }

    #[test]
    fn instantiating_a_generic_struct_with_an_owned_argument_checks() {
        accepts(
            "struct Boxed<T> { value: T }
             fun f(b: Boxed<i32>) {}",
        );
    }

    #[test]
    fn a_reference_typed_function_parameter_still_checks() {
        accepts("fun f(x: &i32) {}");
    }

    #[test]
    fn any_as_a_parameter_or_return_type_checks() {
        accepts("fun f(x: any i32) -> any i32 { return x; }");
    }

    #[test]
    fn a_struct_field_that_is_any_is_rejected() {
        rejects(
            "struct Wrap { inner: any i32 }",
            "`any` may only appear in a parameter or return type",
        );
    }

    #[test]
    fn an_enum_variant_payload_that_is_any_is_rejected() {
        rejects(
            "enum Opt { some: any i32, none }",
            "`any` may only appear in a parameter or return type",
        );
    }

    #[test]
    fn a_tuple_field_containing_any_is_rejected() {
        rejects(
            "struct Pair { both: (i32, any i32) }",
            "`any` may only appear in a parameter or return type",
        );
    }

    #[test]
    fn instantiating_a_generic_struct_with_any_as_the_argument_is_rejected() {
        rejects(
            "struct Boxed<T> { value: T }
             fun f(b: Boxed<any i32>) {}",
            "`any` may only appear in a parameter or return type",
        );
    }

    #[test]
    fn a_let_binding_annotated_any_is_rejected() {
        rejects(
            "fun f(x: any i32) { let y: any i32 = x; }",
            "`any` may only appear in a parameter or return type",
        );
    }

    #[test]
    fn a_bare_dyn_parameter_is_rejected() {
        rejects(
            "trait Show { fun show(&self); }
             fun f(x: dyn Show) {}",
            "`dyn` has no size known at compile time",
        );
    }

    #[test]
    fn a_bare_dyn_return_type_is_rejected() {
        let messages = crate::testing::typeck_src(
            "trait Show { fun show(&self); }
             fun f() -> dyn Show { }",
        );
        assert!(
            messages
                .iter()
                .any(|m| m.contains("`dyn` has no size known at compile time")),
            "{messages:?}"
        );
    }

    #[test]
    fn a_reference_typed_dyn_parameter_checks() {
        accepts(
            "trait Show { fun show(&self); }
             fun f(x: &dyn Show) {}",
        );
    }

    #[test]
    fn an_iso_typed_dyn_parameter_checks() {
        accepts(
            "trait Show { fun show(&self); }
             fun f(x: iso dyn Show) {}",
        );
    }

    #[test]
    fn a_struct_field_that_is_a_bare_dyn_is_rejected() {
        rejects(
            "trait Show { fun show(&self); }
             struct Wrap { inner: dyn Show }",
            "`dyn` has no size known at compile time",
        );
    }

    #[test]
    fn a_struct_field_that_is_a_reference_to_dyn_is_rejected() {
        rejects(
            "trait Show { fun show(&self); }
             struct Wrap { inner: &dyn Show }",
            "a field cannot hold a reference",
        );
    }

    #[test]
    fn a_struct_field_that_is_an_iso_dyn_checks() {
        accepts(
            "trait Show { fun show(&self); }
             struct Wrap { inner: iso dyn Show }",
        );
    }

    #[test]
    fn instantiating_a_generic_struct_with_a_bare_dyn_argument_in_a_field_is_rejected() {
        rejects(
            "trait Show { fun show(&self); }
             struct Boxed<T> { value: T }
             struct Outer { boxed: Boxed<dyn Show> }",
            "`dyn` has no size known at compile time",
        );
    }

    #[test]
    fn instantiating_a_generic_struct_with_an_iso_dyn_argument_in_a_field_checks() {
        accepts(
            "trait Show { fun show(&self); }
             struct Boxed<T> { value: T }
             struct Outer { boxed: Boxed<iso dyn Show> }",
        );
    }
}
