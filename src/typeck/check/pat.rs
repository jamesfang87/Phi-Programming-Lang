use std::collections::HashMap;

use crate::ast::{Ident, Literal, Mutability, Symbol};
use crate::diagnostics::typeck::pat::{
    report_literal_pattern_mismatch, report_match_needs_wildcard, report_match_not_exhaustive,
    report_no_payload_field, report_payload_shape, report_string_pattern_unsupported,
    report_tuple_pattern_mismatch, report_variant_type_unknown,
};
use crate::diagnostics::typeck::report_no_variant;
use crate::driver::source::SrcSpan;
use crate::hir::{
    ArmId, BindingMode, DefId, Hir, HirId, OwnerNode, Pat, PatId, PatKind, Payload, PayloadField,
    VariantPayload,
};
use crate::nameres::PrimTy;
use crate::typeck::Typeck;
use crate::typeck::results::PatAdjust;
use crate::typeck::ty::{Ty, TyKind};

pub(crate) struct ResolvedVariant {
    pub id: HirId,
    pub payload: VariantTys,
}

pub(crate) enum VariantTys {
    Unit,
    Single(Ty),
    Record(Vec<(Ident, Ty)>),
}

type StructFields = (DefId, Vec<(Ident, HirId, Ty)>);

impl VariantTys {
    pub(crate) fn describe(&self) -> &'static str {
        match self {
            VariantTys::Unit => "no payload",
            VariantTys::Single(_) => "a single value",
            VariantTys::Record(_) => "named fields",
        }
    }
}

impl<'hir> Typeck<'hir> {
    pub(crate) fn check_pat(&mut self, id: impl Into<HirId>, expected: Ty, mode: BindingMode) {
        let id = id.into();
        let hir: &'hir Hir = self.hir;
        let pat = hir.pat(id);
        let span = pat.span;

        if matches!(self.tcx.kind(expected), TyKind::Error) {
            return self.check_failed_pat(id, expected, mode);
        }

        let (expected, mode, derefs) = if pat_peels(pat) {
            self.peel_for_pattern(expected, mode)
        } else {
            (expected, mode, 0)
        };
        self.types.record_pat_adjust(id, PatAdjust { derefs, mode });

        let ty = match &pat.kind {
            PatKind::Wildcard => expected,
            PatKind::Binding { .. } => self.binding_ty(expected, mode),
            PatKind::Literal(lit) => self.check_literal_pat(expected, lit, span),
            PatKind::Tuple(elems) => self.check_tuple_pat(expected, elems, mode, span),
            PatKind::Variant { variant, payload } => {
                self.check_variant_pat(expected, *variant, payload, span, mode)
            }

            PatKind::Error => self.tcx.error(),
        };

        self.types.record(id, ty);
    }

    fn check_failed_pat(&mut self, id: HirId, expected: Ty, mode: BindingMode) {
        self.types.record(id, expected);
        self.types
            .record_pat_adjust(id, PatAdjust { derefs: 0, mode });
        for child in self.pat_children(id) {
            self.check_pat(child, expected, mode);
        }
    }

    fn binding_ty(&mut self, expected: Ty, mode: BindingMode) -> Ty {
        match mode {
            BindingMode::Ref => self.tcx.mk_ref(expected, Mutability::Immutable),
            BindingMode::RefMut => self.tcx.mk_ref(expected, Mutability::Mutable),
            BindingMode::Value => expected,
        }
    }

    fn check_literal_pat(&mut self, expected: Ty, lit: &Literal, span: SrcSpan) -> Ty {
        if matches!(lit, Literal::Str(_)) {
            report_string_pattern_unsupported(self.session, span);
            return self.tcx.error();
        }

        let found = self.check_literal(lit, span);
        if let Err(err) = self.unifier.unify(&self.tcx, expected, found) {
            report_literal_pattern_mismatch(self.display_cx(), err, span);
        }
        expected
    }

    fn check_tuple_pat(
        &mut self,
        expected: Ty,
        elems: &[PatId],
        mode: BindingMode,
        span: SrcSpan,
    ) -> Ty {
        let vars: Vec<Ty> = elems.iter().map(|_| self.tcx.next_infer_var()).collect();
        let tuple = self.tcx.mk_tuple(vars.clone());

        if let Err(err) = self.unifier.unify(&self.tcx, expected, tuple) {
            report_tuple_pattern_mismatch(self.display_cx(), err, span);
            for &elem in elems {
                let error = self.tcx.error();
                self.check_pat(elem, error, mode);
            }
            return self.tcx.error();
        }

        for (&elem, &var) in elems.iter().zip(vars.iter()) {
            self.check_pat(elem, var, mode);
        }
        expected
    }

    pub(crate) fn peel_for_pattern(
        &mut self,
        expected: Ty,
        mode: BindingMode,
    ) -> (Ty, BindingMode, u32) {
        let mut expected = self.unifier.find_deep(&mut self.tcx, expected);
        let mut mode = mode;
        let mut derefs = 0;
        while let TyKind::Ref { base, mutability } = *self.tcx.kind(expected) {
            expected = self.unifier.find_deep(&mut self.tcx, base);
            derefs += 1;
            mode = match (mode, mutability) {
                (BindingMode::Ref, _) => BindingMode::Ref,
                (_, Mutability::Mutable) => BindingMode::RefMut,
                (_, Mutability::Immutable) => BindingMode::Ref,
            };
        }
        (expected, mode, derefs)
    }

    fn check_variant_pat(
        &mut self,
        expected: Ty,
        variant: Ident,
        payload: &'hir Payload,
        span: SrcSpan,
        mode: BindingMode,
    ) -> Ty {
        let expected = self.unifier.find_deep(&mut self.tcx, expected);
        if matches!(self.tcx.kind(expected), TyKind::Var(_)) {
            report_variant_type_unknown(self.session, variant, span);
            self.check_failed_payload(payload, mode);
            return self.tcx.error();
        }

        let Some(found) = self.resolve_variant(expected, variant.text) else {
            report_no_variant(self.display_cx(), variant, expected);
            self.check_failed_payload(payload, mode);
            return self.tcx.error();
        };

        self.check_payload_pats(&found, payload, variant, span, mode);
        expected
    }

    fn check_payload_pats(
        &mut self,
        found: &ResolvedVariant,
        payload: &'hir Payload,
        variant: Ident,
        span: SrcSpan,
        mode: BindingMode,
    ) {
        match (&found.payload, payload) {
            (VariantTys::Unit, Payload::None) => {}
            (VariantTys::Single(declared), Payload::Single(pat)) => {
                self.check_pat(*pat, *declared, mode);
            }
            (VariantTys::Record(declared), Payload::Record(written)) => {
                self.check_record_pats(declared, written, found.id, mode);
            }
            _ => {
                report_payload_shape(self.session, self.hir, variant, span, found);
                self.check_failed_payload(payload, mode);
            }
        }
    }

    fn check_record_pats(
        &mut self,
        declared: &[(Ident, Ty)],
        written: &'hir [PayloadField],
        variant: HirId,
        mode: BindingMode,
    ) {
        for field in written {
            let (name, pat) = (field.name, field.value);
            match declared.iter().find(|(field, _)| field.text == name.text) {
                Some(&(_, ty)) => self.check_pat(pat, ty, mode),
                None => {
                    report_no_payload_field(self.session, self.hir, name, variant);
                    let error = self.tcx.error();
                    self.check_pat(pat, error, mode);
                }
            }
        }
    }

    fn check_failed_payload(&mut self, payload: &'hir Payload, mode: BindingMode) {
        let error = self.tcx.error();
        for pat in payload_pats(payload) {
            self.check_pat(pat, error, mode);
        }
    }

    fn pat_children(&self, id: impl Into<HirId>) -> Vec<HirId> {
        let id = id.into();
        match &self.hir.pat(id).kind {
            PatKind::Wildcard | PatKind::Binding { .. } | PatKind::Literal(_) | PatKind::Error => {
                Vec::new()
            }
            PatKind::Tuple(elems) => elems.iter().map(|p| (*p).into()).collect(),
            PatKind::Variant { payload, .. } => payload_pats(payload),
        }
    }

    pub(crate) fn adt_and_generic_substs(&self, ty: Ty) -> Option<(DefId, HashMap<HirId, Ty>)> {
        let TyKind::Adt { def, args } = self.tcx.kind(ty).clone() else {
            return None;
        };
        let generics = match self.hir.def(def) {
            OwnerNode::Struct(struct_) => &struct_.generics,
            OwnerNode::Enum(enum_) => &enum_.generics,
            _ => return None,
        };
        Some((def, generics.iter().copied().zip(args).collect()))
    }

    pub(crate) fn resolve_variant(&mut self, ty: Ty, name: Symbol) -> Option<ResolvedVariant> {
        let hir = self.hir;
        let (def, subst) = self.adt_and_generic_substs(ty)?;
        let OwnerNode::Enum(enum_) = hir.def(def) else {
            return None;
        };
        let id = *enum_
            .variants
            .iter()
            .find(|&&id| hir.variant(id).name.text == name)?;

        let payload = match &hir.variant(id).payload {
            VariantPayload::Unit => VariantTys::Unit,
            VariantPayload::Type(_) => {
                let declared = self
                    .types
                    .ty(id)
                    .expect("collect_enum records a type payload's type on the variant node");
                VariantTys::Single(self.subst_ty(declared, &subst))
            }
            VariantPayload::Record(fields) => VariantTys::Record(
                fields
                    .iter()
                    .map(|&field| {
                        let declared = self
                            .types
                            .ty(field)
                            .expect("collect_fields records every field's declared type");
                        (hir.field(field).name, self.subst_ty(declared, &subst))
                    })
                    .collect(),
            ),
        };

        Some(ResolvedVariant { id, payload })
    }

    pub(crate) fn check_match_exhaustive(
        &mut self,
        scrutinee_ty: Ty,
        arms: &[ArmId],
        span: SrcSpan,
    ) {
        let ty = self.unifier.find_deep(&mut self.tcx, scrutinee_ty);
        if matches!(
            self.tcx.kind(ty),
            TyKind::Var(_) | TyKind::Error | TyKind::Never
        ) {
            return;
        }
        if self.has_irrefutable_arm(arms) {
            return;
        }

        match self.tcx.kind(ty).clone() {
            TyKind::Primitive(PrimTy::Bool) => self.check_bool_exhaustive(arms, span),
            TyKind::Adt { def, .. } => self.check_enum_exhaustive(ty, def, arms, span),
            _ => report_match_needs_wildcard(self.session, span),
        }
    }

    fn has_irrefutable_arm(&self, arms: &[ArmId]) -> bool {
        let hir = self.hir;
        arms.iter()
            .filter(|&&arm| hir.arm(arm).guard.is_none())
            .any(|&arm| {
                matches!(
                    hir.pat(hir.arm(arm).pat).kind,
                    PatKind::Wildcard | PatKind::Binding { .. }
                )
            })
    }

    fn check_bool_exhaustive(&self, arms: &[ArmId], span: SrcSpan) {
        let (mut has_true, mut has_false) = (false, false);
        for &arm in arms
            .iter()
            .filter(|&&arm| self.hir.arm(arm).guard.is_none())
        {
            match &self.hir.pat(self.hir.arm(arm).pat).kind {
                PatKind::Literal(Literal::Bool(true)) => has_true = true,
                PatKind::Literal(Literal::Bool(false)) => has_false = true,
                _ => {}
            }
        }

        let missing: Vec<&str> = [(has_true, "true"), (has_false, "false")]
            .into_iter()
            .filter(|&(seen, _)| !seen)
            .map(|(_, name)| name)
            .collect();
        if !missing.is_empty() {
            report_match_not_exhaustive(self.session, span, &missing);
        }
    }

    fn check_enum_exhaustive(&mut self, ty: Ty, def: DefId, arms: &[ArmId], span: SrcSpan) {
        let hir = self.hir;
        let OwnerNode::Enum(enum_) = hir.def(def) else {
            report_match_needs_wildcard(self.session, span);
            return;
        };

        let missing = self.missing_variants(&enum_.variants, arms);
        if !missing.is_empty() {
            let missing: Vec<&str> = missing.iter().map(String::as_str).collect();
            report_match_not_exhaustive(self.session, span, &missing);
            return;
        }

        let pats: Vec<&'hir Pat> = arms
            .iter()
            .filter(|&&arm| hir.arm(arm).guard.is_none())
            .map(|&arm| hir.pat(hir.arm(arm).pat))
            .collect();
        if !self.pats_cover(ty, &pats) {
            report_match_needs_wildcard(self.session, span);
        }
    }

    fn missing_variants(&self, variants: &[HirId], arms: &[ArmId]) -> Vec<String> {
        let hir = self.hir;
        variants
            .iter()
            .map(|&id| hir.variant(id).name)
            .filter(|&name| {
                !arms
                    .iter()
                    .filter(|&&arm| hir.arm(arm).guard.is_none())
                    .any(|&arm| {
                        matches!(
                            &hir.pat(hir.arm(arm).pat).kind,
                            PatKind::Variant { variant, .. } if variant.text == name.text
                        )
                    })
            })
            .map(|name| self.session.resolve(name.text).to_string())
            .collect()
    }

    fn pats_cover(&mut self, ty: Ty, pats: &[&'hir Pat]) -> bool {
        if pats.iter().any(|pat| self.pat_is_irrefutable(pat.hir_id)) {
            return true;
        }

        let ty = self.unifier.find_deep(&mut self.tcx, ty);
        match self.tcx.kind(ty).clone() {
            TyKind::Primitive(PrimTy::Bool) => bool_pats_cover(pats),
            TyKind::Adt { def, .. } => self.enum_pats_cover(ty, def, pats),
            _ => false,
        }
    }

    fn enum_pats_cover(&mut self, ty: Ty, def: DefId, pats: &[&'hir Pat]) -> bool {
        let OwnerNode::Enum(enum_) = self.hir.def(def) else {
            return false;
        };
        for &variant in &enum_.variants {
            if !self.variant_pats_cover(ty, variant, pats) {
                return false;
            }
        }
        true
    }

    fn variant_pats_cover(&mut self, ty: Ty, variant: HirId, pats: &[&'hir Pat]) -> bool {
        let hir = self.hir;
        let name = hir.variant(variant).name;
        let matching: Vec<&'hir Pat> = pats
            .iter()
            .copied()
            .filter(|pat| {
                matches!(
                    &pat.kind,
                    PatKind::Variant { variant, .. } if variant.text == name.text
                )
            })
            .collect();
        if matching.is_empty() {
            return false;
        }

        let Some(found) = self.resolve_variant(ty, name.text) else {
            return false;
        };
        match &found.payload {
            VariantTys::Unit => true,
            VariantTys::Single(payload_ty) => {
                self.single_payload_pats_cover(*payload_ty, &matching)
            }
            VariantTys::Record(fields) => self.record_payload_pats_cover(fields, &matching),
        }
    }

    fn single_payload_pats_cover(&mut self, payload_ty: Ty, matching: &[&'hir Pat]) -> bool {
        let subpats: Vec<&'hir Pat> = matching
            .iter()
            .filter_map(|pat| match &pat.kind {
                PatKind::Variant {
                    payload: Payload::Single(inner),
                    ..
                } => Some(self.hir.pat(*inner)),
                _ => None,
            })
            .collect();
        subpats.len() != matching.len() || self.pats_cover(payload_ty, &subpats)
    }

    fn record_payload_pats_cover(
        &mut self,
        fields: &[(Ident, Ty)],
        matching: &[&'hir Pat],
    ) -> bool {
        for (field_name, field_ty) in fields {
            let mut subpats = Vec::new();
            let mut omitted = false;
            for pat in matching {
                let PatKind::Variant {
                    payload: Payload::Record(written),
                    ..
                } = &pat.kind
                else {
                    omitted = true;
                    continue;
                };
                match written.iter().find(|f| f.name.text == field_name.text) {
                    Some(field) => subpats.push(self.hir.pat(field.value)),
                    None => omitted = true,
                }
            }
            if !omitted && !self.pats_cover(*field_ty, &subpats) {
                return false;
            }
        }
        true
    }

    pub(crate) fn struct_fields(&mut self, ty: Ty) -> Option<StructFields> {
        let hir = self.hir;
        let (def, subst) = self.adt_and_generic_substs(ty)?;
        let OwnerNode::Struct(struct_) = hir.def(def) else {
            return None;
        };

        let fields = struct_
            .fields
            .iter()
            .map(|&field| {
                let declared = self
                    .types
                    .ty(field)
                    .expect("collect_fields records every field's declared type");
                (
                    hir.field(field).name,
                    field,
                    self.subst_ty(declared, &subst),
                )
            })
            .collect();
        Some((def, fields))
    }
}

fn pat_peels(pat: &Pat) -> bool {
    matches!(
        pat.kind,
        PatKind::Variant { .. } | PatKind::Tuple(_) | PatKind::Literal(_)
    )
}

fn bool_pats_cover(pats: &[&Pat]) -> bool {
    let (mut has_true, mut has_false) = (false, false);
    for pat in pats {
        match &pat.kind {
            PatKind::Literal(Literal::Bool(true)) => has_true = true,
            PatKind::Literal(Literal::Bool(false)) => has_false = true,
            _ => {}
        }
    }
    has_true && has_false
}

fn payload_pats(payload: &Payload) -> Vec<HirId> {
    match payload {
        Payload::None => Vec::new(),
        Payload::Single(inner) => vec![*inner],
        Payload::Record(fields) => fields.iter().map(|field| field.value).collect(),
    }
}

#[cfg(test)]
mod tests {
    use crate::testing::{typeck_accepts as accepts, typeck_rejects as rejects};

    #[test]
    fn a_literal_pattern_has_to_match_the_scrutinees_type() {
        accepts("fun f(n: i32) -> i32 { return match n { 0 => 1, _ => 2, }; }");
        rejects(
            "fun f(n: i32) -> i32 { return match n { true => 1, _ => 2, }; }",
            "mismatched types",
        );
    }

    #[test]
    fn a_float_literal_pattern_matches_a_float_scrutinee() {
        accepts("fun f(n: f64) -> i32 { return match n { 3.14 => 1, _ => 0 }; }");
    }

    #[test]
    fn a_string_literal_pattern_is_rejected() {
        rejects(
            "fun f(s: str) -> i32 { return match s { \"hi\" => 1, _ => 0 }; }",
            "string literal patterns are not supported",
        );
    }

    #[test]
    fn a_wildcard_matches_anything() {
        accepts("fun f(n: bool) -> i32 { return match n { _ => 1, }; }");
    }

    #[test]
    fn a_record_variant_pattern_binds_each_named_field() {
        accepts(
            "enum Shape { square: { l: f64, w: i32 } }
             fun f(s: Shape) -> i32 { return match s { .square { l, w } => w, }; }",
        );
        rejects(
            "enum Shape { square: { l: f64, w: i32 } }
             fun f(s: Shape) -> i32 { return match s { .square { l, w } => l, }; }",
            "mismatched types",
        );
    }

    #[test]
    fn a_record_variant_pattern_may_leave_fields_out() {
        accepts(
            "enum Shape { square: { l: f64, w: i32 } }
             fun f(s: Shape) -> i32 { return match s { .square { w } => w, }; }",
        );
    }

    #[test]
    fn a_record_variant_pattern_naming_a_field_that_is_not_declared_is_reported() {
        rejects(
            "enum Shape { square: { l: f64 } }
             fun f(s: Shape) -> i32 { return match s { .square { h } => 1, }; }",
            "no field `h` on variant `square`",
        );
    }

    #[test]
    fn a_variant_pattern_written_with_the_wrong_payload_shape_is_reported() {
        rejects(
            "enum Shape { unit, circle: f64 }
             fun f(s: Shape) -> i32 { return match s { .circle => 1, .unit => 2, }; }",
            "carries a single value",
        );
    }

    #[test]
    fn a_pattern_nested_two_levels_deep_still_binds_at_the_declared_type() {
        accepts(
            "enum Option<T> { some: T, none }
             fun f(o: Option<(i32, bool)>) -> bool {
                 return match o { .some((n, b)) => b, .none => false, };
             }",
        );
        rejects(
            "enum Option<T> { some: T, none }
             fun f(o: Option<(i32, bool)>) -> bool {
                 return match o { .some((n, b)) => n, .none => false, };
             }",
            "mismatched types",
        );
    }

    #[test]
    fn a_binding_under_a_failed_pattern_does_not_report_again() {
        rejects(
            "enum Shape { unit }
             fun f(s: Shape) -> i32 { return match s { .square(r) => r, .unit => 1, }; }",
            "no variant `square`",
        );
    }

    #[test]
    fn a_match_missing_a_variant_and_with_no_wildcard_is_rejected() {
        rejects(
            "enum Shape { unit, circle: f64 }
             fun f(s: Shape) -> i32 { return match s { .unit => 1, }; }",
            "not covered",
        );
    }

    #[test]
    fn a_match_covering_every_variant_needs_no_wildcard() {
        accepts(
            "enum Shape { unit, circle: f64 }
             fun f(s: Shape) -> i32 { return match s { .unit => 1, .circle(_) => 2, }; }",
        );
    }

    #[test]
    fn a_missing_bool_arm_is_rejected_without_a_wildcard() {
        rejects(
            "fun f(b: bool) -> i32 { return match b { true => 1, }; }",
            "not covered",
        );
    }

    #[test]
    fn a_type_this_check_does_not_enumerate_still_needs_a_wildcard() {
        rejects(
            "fun f(t: (bool, bool)) -> i32 {
                 return match t {
                     (true, true) => 1,
                     (true, false) => 2,
                     (false, true) => 3,
                     (false, false) => 4,
                 };
             }",
            "not covered",
        );
    }

    #[test]
    fn an_unknown_variant_does_not_also_trigger_an_exhaustiveness_diagnostic() {
        rejects(
            "enum Shape { unit, circle: f64 }
             fun f(s: Shape) -> i32 { return match s { .square => 1, .unit => 2, }; }",
            "no variant `square`",
        );
    }

    #[test]
    fn a_guarded_wildcard_does_not_count_toward_exhaustiveness() {
        rejects(
            "fun f(b: bool) -> i32 { return match b { true => 1, _ if b => 2, }; }",
            "not covered",
        );
    }

    #[test]
    fn an_unguarded_wildcard_after_a_guarded_arm_still_covers_everything() {
        accepts("fun f(b: bool) -> i32 { return match b { true if b => 1, _ => 2, }; }");
    }

    #[test]
    fn a_guarded_variant_arm_does_not_count_toward_exhaustiveness() {
        rejects(
            "enum Shape { unit, circle: f64 }
             fun f(s: Shape) -> i32 {
                 return match s { .unit => 1, .circle(r) if r > 0.0 => 2, };
             }",
            "not covered",
        );
    }

    #[test]
    fn a_three_element_tuple_pattern_binds_each_element() {
        accepts(
            "fun f() -> bool {
                 let (a, b, c) = (1, true, 'x');
                 return b;
             }",
        );
        rejects(
            "fun f() -> bool {
                 let (a, b, c) = (1, true, 'x');
                 return a;
             }",
            "mismatched types",
        );
    }

    #[test]
    fn a_tuple_nested_inside_a_tuple_pattern_binds_correctly() {
        accepts(
            "fun f() -> bool {
                 let (a, (b, c)) = (1, (true, 'x'));
                 return b;
             }",
        );
        rejects(
            "fun f() -> bool {
                 let (a, (b, c)) = (1, (true, 'x'));
                 return c;
             }",
            "mismatched types",
        );
    }

    #[test]
    fn a_bool_literal_pattern_matches_a_bool_scrutinee() {
        accepts("fun f(b: bool) -> i32 { return match b { true => 1, false => 2, }; }");
    }

    #[test]
    fn a_char_literal_pattern_matches_a_char_scrutinee() {
        accepts("fun f(c: char) -> i32 { return match c { 'a' => 1, _ => 2, }; }");
    }

    #[test]
    fn a_variant_pattern_nested_inside_another_variant_pattern() {
        accepts(
            "enum Option<T> { some: T, none }
             enum Shape { unit, circle: f64 }
             fun f(o: Option<Shape>) -> f64 {
                 return match o {
                     .some(.circle(r)) => r,
                     .some(.unit) => 0.0,
                     .none => 0.0,
                 };
             }",
        );
        rejects(
            "enum Option<T> { some: T, none }
             enum Shape { unit, circle: f64 }
             fun f(o: Option<Shape>) {
                 let v = match o {
                     .some(.circle(r)) => r,
                     .some(.unit) => true,
                     .none => 0.0,
                 };
             }",
            "mismatched types",
        );
    }

    #[test]
    fn a_two_parameter_generic_enums_variants_bind_each_parameter_separately() {
        accepts(
            "enum Result<T, E> { ok: T, err: E }
             fun f(r: Result<i32, bool>) -> i32 {
                 return match r {
                     .ok(v) => v,
                     .err(e) => if e { 1 } else { 0 },
                 };
             }",
        );
        rejects(
            "enum Result<T, E> { ok: T, err: E }
             fun f(r: Result<i32, bool>) -> bool {
                 return match r {
                     .ok(v) => v,
                     .err(e) => e,
                 };
             }",
            "mismatched types",
        );
    }

    #[test]
    fn a_record_payload_pattern_may_appear_inside_a_tuple_pattern() {
        accepts(
            "enum Shape { square: { l: f64 } }
             fun f(s: Shape) -> f64 {
                 let pair = (s, 1);
                 let (.square { l }, n) = pair;
                 return l;
             }",
        );
    }
}

#[cfg(test)]
mod binding_mode_tests {
    use crate::testing::{OPS_PREAMBLE, typeck_accepts, typeck_rejects, typeck_src_files};

    fn accepts_with_ops(src: &str) {
        let reported = typeck_src_files(&[OPS_PREAMBLE, src]);
        assert!(
            reported.is_empty(),
            "expected {src:?} to check: {reported:?}"
        );
    }

    #[test]
    fn a_match_on_a_shared_reference_sees_the_variants() {
        typeck_accepts(
            "enum Opt<T> { some: T, none }\n\
             extend<T> Opt<T> {\n\
                 fun is_some(&self) -> bool {\n\
                     return match self { .some(_) => true, .none => false, };\n\
                 }\n\
             }",
        );
    }

    #[test]
    fn a_payload_bound_through_a_shared_reference_is_a_reference() {
        accepts_with_ops(
            "module app;\n\
             enum Opt<T> { some: T, none }\n\
             fun first(o: &Opt<i32>) -> i32 {\n\
                 return match o { .some(v) => *v, .none => 0, };\n\
             }",
        );
    }

    #[test]
    fn a_payload_bound_through_a_reference_is_not_the_bare_type() {
        typeck_rejects(
            "enum Opt<T> { some: T, none }\n\
             fun first(o: &Opt<i32>) -> i32 {\n\
                 return match o { .some(v) => v, .none => 0, };\n\
             }",
            "mismatched types",
        );
    }

    #[test]
    fn a_mutable_reference_binds_mutably() {
        accepts_with_ops(
            "module app;\n\
             enum Opt<T> { some: T, none }\n\
             fun take_mut(x: &mut i32) {}\n\
             fun bump(o: &mut Opt<i32>) {\n\
                 match o { .some(v) => { take_mut(v); }, .none => {}, }\n\
             }",
        );
    }

    #[test]
    fn immutability_is_sticky_through_nesting() {
        let reported = typeck_src_files(&[
            OPS_PREAMBLE,
            "module app;\n\
             enum Opt<T> { some: T, none }\n\
             fun take_mut(x: &mut i32) {}\n\
             fun bump(o: & &mut Opt<i32>) {\n\
                 match o { .some(v) => { take_mut(v); }, .none => {}, }\n\
             }",
        ]);
        assert!(
            reported.iter().any(|d| d.contains("mismatched types")),
            "a `&mut` behind a `&` binds as `&`, not `&mut`: {reported:?}"
        );
    }

    #[test]
    fn a_match_on_a_reference_is_still_exhaustive() {
        typeck_accepts(
            "enum Opt<T> { some: T, none }\n\
             fun is_none(o: &Opt<i32>) -> bool {\n\
                 return match o { .some(_) => false, .none => true, };\n\
             }",
        );
    }
}

impl<'hir> Typeck<'hir> {
    pub(crate) fn pat_is_irrefutable(&mut self, pat_id: impl Into<HirId>) -> bool {
        let pat_id = pat_id.into();
        let pat = self.hir.pat(pat_id);
        match &pat.kind {
            PatKind::Wildcard | PatKind::Binding { .. } => true,
            PatKind::Literal(_) => false,
            PatKind::Variant { payload, .. } => {
                let ty = self
                    .types
                    .ty(pat_id)
                    .map(|ty| self.unifier.find_deep(&mut self.tcx, ty))
                    .unwrap_or_else(|| self.tcx.error());
                match self.tcx.kind(ty) {
                    TyKind::Error | TyKind::Var(_) => true,
                    TyKind::Adt { def, .. } => match self.hir.def(*def) {
                        OwnerNode::Enum(enum_) if enum_.variants.len() == 1 => {
                            self.payload_is_irrefutable(payload)
                        }
                        _ => false,
                    },
                    _ => false,
                }
            }
            PatKind::Tuple(elems) => elems.iter().all(|&elem| self.pat_is_irrefutable(elem)),
            PatKind::Error => true,
        }
    }

    fn payload_is_irrefutable(&mut self, payload: &'hir Payload) -> bool {
        match payload {
            Payload::None => true,
            Payload::Single(pat) => self.pat_is_irrefutable(*pat),
            Payload::Record(fields) => fields
                .iter()
                .all(|field| self.pat_is_irrefutable(field.value)),
        }
    }
}
