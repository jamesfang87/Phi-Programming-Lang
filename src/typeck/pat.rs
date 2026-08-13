use std::collections::HashMap;

use crate::ast::interner::Interner;
use crate::ast::{Ident, Literal, Symbol};
use crate::diag::{DiagCtx, Diagnostic};
use crate::driver::source::SrcSpan;
use crate::hir::{HirId, Node, OwnerNode, PatKind, Payload, VariantPayload};
use crate::nameres::PrimTy;
use crate::typeck::Typeck;
use crate::typeck::ty::{Ty, TyKind};

pub(crate) struct VariantDef {
    pub id: HirId,
    pub payload: VariantTys,
}

pub(crate) enum VariantTys {
    Unit,
    Single(Ty),
    Record(Vec<(Ident, Ty)>),
}

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
    pub(crate) fn check_pat(&mut self, id: HirId, expected: Ty) {
        let Node::Pat(pat) = self.hir.node(id) else {
            unreachable!("Node that is not a pattern passed to check_pat");
        };
        let span = pat.span;

        // Keep checking a pattern below a failed one
        if matches!(self.tcx.kind(expected), TyKind::Error) {
            self.types.record(id, expected);
            for child in self.pat_children(id) {
                self.check_pat(child, expected);
            }
            return;
        }

        let ty = match &pat.kind {
            PatKind::Wildcard | PatKind::Binding { .. } => expected,
            PatKind::Literal(lit) => {
                let found = self.check_literal(lit, span);
                if let Err(err) = self.unifier.unify(&self.tcx, expected, found) {
                    DiagCtx::emit(
                        Diagnostic::error(self.cx().show(err).to_string(), span).with_label(
                            "this literal cannot match a value of the type being matched",
                        ),
                    );
                }
                expected
            }
            PatKind::Tuple(elems) => {
                let elems = elems.clone();
                let vars: Vec<Ty> = elems.iter().map(|_| self.tcx.next_ty_var()).collect();
                let tuple = self.tcx.mk_tuple(vars.clone());
                if let Err(err) = self.unifier.unify(&self.tcx, expected, tuple) {
                    DiagCtx::emit(
                        Diagnostic::error(self.cx().show(err).to_string(), span)
                            .with_label("this tuple pattern does not match the value's type"),
                    );
                    for &elem in &elems {
                        let error = self.tcx.error();
                        self.check_pat(elem, error);
                    }
                    self.tcx.error()
                } else {
                    for (&elem, &var) in elems.iter().zip(vars.iter()) {
                        self.check_pat(elem, var);
                    }
                    expected
                }
            }
            PatKind::Variant { variant, payload } => {
                let (variant, payload) = (*variant, self.payload_ids(payload));
                self.check_variant_pat(expected, variant, &payload, span)
            }
            // Already reported by the parser.
            PatKind::Error => self.tcx.error(),
        };

        self.types.record(id, ty);
    }

    fn check_variant_pat(
        &mut self,
        expected: Ty,
        variant: Ident,
        payload: &PayloadIds,
        span: SrcSpan,
    ) -> Ty {
        let expected = self.resolve_deep(expected);
        if matches!(self.tcx.kind(expected), TyKind::Var(_)) {
            self.report_variant_type_unknown(variant, span);
            self.fail_payload(payload);
            return self.tcx.error();
        }

        let Some(found) = self.lookup_variant(expected, variant.text) else {
            self.report_no_variant(variant, expected);
            self.fail_payload(payload);
            return self.tcx.error();
        };

        self.check_payload_pats(&found, payload, variant, span);
        expected
    }

    /// Checks a variant pattern's sub-patterns against what the variant declares it carries.
    fn check_payload_pats(
        &mut self,
        found: &VariantDef,
        payload: &PayloadIds,
        variant: Ident,
        span: SrcSpan,
    ) {
        match (&found.payload, payload) {
            (VariantTys::Unit, PayloadIds::None) => {}
            (VariantTys::Single(declared), PayloadIds::Single(pat)) => {
                self.check_pat(*pat, *declared);
            }
            (VariantTys::Record(declared), PayloadIds::Record(written)) => {
                self.check_record_pats(declared, written, found.id);
            }
            _ => {
                self.report_payload_shape(variant, span, found);
                self.fail_payload(payload);
            }
        }
    }

    /// Checks a record payload's field patterns against the fields the variant declares.
    fn check_record_pats(
        &mut self,
        declared: &[(Ident, Ty)],
        written: &[(Ident, HirId)],
        variant: HirId,
    ) {
        for &(name, pat) in written {
            match declared.iter().find(|(field, _)| field.text == name.text) {
                Some(&(_, ty)) => self.check_pat(pat, ty),
                None => {
                    self.report_no_payload_field(name, variant);
                    let error = self.tcx.error();
                    self.check_pat(pat, error);
                }
            }
        }
    }

    /// Walks the sub-patterns of a failed payload (one that has an error) so
    /// that the names they bind still have types.
    fn fail_payload(&mut self, payload: &PayloadIds) {
        let error = self.tcx.error();
        match payload {
            PayloadIds::None => {}
            PayloadIds::Single(pat) => self.check_pat(*pat, error),
            PayloadIds::Record(fields) => {
                for &(_, pat) in fields {
                    self.check_pat(pat, error);
                }
            }
        }
    }

    fn pat_children(&self, id: HirId) -> Vec<HirId> {
        match &self.hir.pat(id).kind {
            PatKind::Wildcard | PatKind::Binding { .. } | PatKind::Literal(_) | PatKind::Error => {
                Vec::new()
            }
            PatKind::Tuple(elems) => elems.clone(),
            PatKind::Variant { payload, .. } => match payload {
                Payload::None => Vec::new(),
                Payload::Single(inner) => vec![*inner],
                Payload::Record(fields) => fields.iter().map(|field| field.value).collect(),
            },
        }
    }

    fn payload_ids(&self, payload: &Payload) -> PayloadIds {
        match payload {
            Payload::None => PayloadIds::None,
            Payload::Single(inner) => PayloadIds::Single(*inner),
            Payload::Record(fields) => PayloadIds::Record(
                fields
                    .iter()
                    .map(|field| (field.name, field.value))
                    .collect(),
            ),
        }
    }

    // -----------------------------------------------------------------
    // Declaration lookups
    // -----------------------------------------------------------------

    pub(crate) fn lookup_variant(&mut self, ty: Ty, name: Symbol) -> Option<VariantDef> {
        let hir = self.hir;
        let TyKind::Adt { def, args } = self.tcx.kind(ty).clone() else {
            return None;
        };
        let OwnerNode::Enum(enum_) = hir.def(def) else {
            return None;
        };
        let id = *enum_
            .variants
            .iter()
            .find(|&&id| hir.variant(id).name.text == name)?;

        // A variant's declared payload is written in the enum's terms, so it is read through
        // the arguments the matched type applied -- `some`'s payload on `Option<i32>` is `i32`.
        let subst: HashMap<HirId, Ty> = enum_.generics.iter().copied().zip(args).collect();

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

        Some(VariantDef { id, payload })
    }

    /// Checks that `arms`' own patterns -- taken one level deep, ignoring what any payload
    /// sub-pattern does or doesn't cover -- account for every value `scrutinee_ty` could hold.
    ///
    /// Two shapes are judged directly: `bool` (covered exactly by writing both `true` and
    /// `false`) and an enum (covered by naming every one of its variants, in any arm, with any
    /// payload). A wildcard or a bare binding covers anything and short-circuits both. Every
    /// other type -- another primitive, a tuple, a struct, a generic parameter -- has no finite
    /// enumeration this checks against, so it demands a catch-all instead.
    ///
    /// Deliberately shallow: a `.some(.circle(_))` / `.none` pair is accepted as covering
    /// `Option<Shape>` without asking whether `.circle(_)` alone covers `Shape`. Nesting that
    /// check would mean specializing column-by-column the way a real usefulness algorithm does;
    /// this only ever answers the question one pattern position asks of the scrutinee it was
    /// matched against, the same depth every other check in this module works at.
    pub(crate) fn check_match_exhaustive(
        &mut self,
        scrutinee_ty: Ty,
        arms: &[HirId],
        span: SrcSpan,
    ) {
        let ty = self.resolve_deep(scrutinee_ty);
        // A scrutinee that never settled, or already failed, has nothing to judge coverage
        // against -- and one that can never produce a value at all needs no arm to handle it.
        if matches!(
            self.tcx.kind(ty),
            TyKind::Var(_) | TyKind::Error | TyKind::Never
        ) {
            return;
        }

        let hir = self.hir;
        let pat_kind = |arm: HirId| &hir.pat(hir.arm(arm).pat).kind;

        if arms
            .iter()
            .any(|&arm| matches!(pat_kind(arm), PatKind::Wildcard | PatKind::Binding { .. }))
        {
            return;
        }

        match self.tcx.kind(ty) {
            TyKind::Primitive(PrimTy::Bool) => {
                let (mut has_true, mut has_false) = (false, false);
                for &arm in arms {
                    match pat_kind(arm) {
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
                    self.report_match_not_exhaustive(span, &missing);
                }
            }
            TyKind::Adt { def, .. } => {
                let OwnerNode::Enum(enum_) = hir.def(*def) else {
                    self.report_match_needs_wildcard(span);
                    return;
                };
                let missing: Vec<String> = enum_
                    .variants
                    .iter()
                    .map(|&id| hir.variant(id).name)
                    .filter(|&name| {
                        !arms.iter().any(|&arm| {
                            matches!(pat_kind(arm), PatKind::Variant { variant, .. } if variant.text == name.text)
                        })
                    })
                    .map(|name| Interner::resolve(name.text).to_string())
                    .collect();
                if !missing.is_empty() {
                    let missing: Vec<&str> = missing.iter().map(String::as_str).collect();
                    self.report_match_not_exhaustive(span, &missing);
                }
            }
            _ => self.report_match_needs_wildcard(span),
        }
    }

    pub(crate) fn struct_fields(&mut self, ty: Ty) -> Option<Vec<(Ident, Ty)>> {
        let hir = self.hir;
        let TyKind::Adt { def, args } = self.tcx.kind(ty).clone() else {
            return None;
        };
        let OwnerNode::Struct(struct_) = hir.def(def) else {
            return None;
        };
        let subst: HashMap<HirId, Ty> = struct_.generics.iter().copied().zip(args).collect();

        Some(
            struct_
                .fields
                .iter()
                .map(|&field| {
                    let declared = self
                        .types
                        .ty(field)
                        .expect("collect_fields records every field's declared type");
                    (hir.field(field).name, self.subst_ty(declared, &subst))
                })
                .collect(),
        )
    }

    // -----------------------------------------------------------------
    // Diagnostics
    // -----------------------------------------------------------------

    /// Reported when a `.variant` pattern is matched against a type that is still an inference
    /// variable.
    fn report_variant_type_unknown(&self, variant: Ident, span: SrcSpan) {
        DiagCtx::emit(
            Diagnostic::error(
                format!(
                    "type annotations needed: the type `.{}` is matched against is still unknown",
                    Interner::resolve(variant.text)
                ),
                span,
            )
            .with_label("cannot tell which enum this variant belongs to")
            .with_help(
                "a `.variant` names no enum of its own; write the type of the value being \
                 matched",
            ),
        );
    }

    fn report_no_variant(&self, variant: Ident, ty: Ty) {
        DiagCtx::emit(
            Diagnostic::error(
                format!(
                    "no variant `{}` on `{}`",
                    Interner::resolve(variant.text),
                    self.cx().show(ty)
                ),
                variant.span,
            )
            .with_label("not a variant of this type"),
        );
    }

    fn report_no_payload_field(&self, field: Ident, variant: HirId) {
        DiagCtx::emit(
            Diagnostic::error(
                format!(
                    "no field `{}` on variant `{}`",
                    Interner::resolve(field.text),
                    Interner::resolve(self.hir.variant(variant).name.text)
                ),
                field.span,
            )
            .with_label("not declared by this variant")
            .with_secondary(self.hir.variant(variant).span, "declared here"),
        );
    }

    /// Reported by [`Typeck::check_match_exhaustive`] when `missing` names the specific values
    /// (variant names, or `"true"`/`"false"`) no arm covers.
    fn report_match_not_exhaustive(&self, span: SrcSpan, missing: &[&str]) {
        let list = missing
            .iter()
            .map(|m| format!("`{m}`"))
            .collect::<Vec<_>>()
            .join(", ");
        DiagCtx::emit(
            Diagnostic::error(format!("match is not exhaustive: {list} not covered"), span)
                .with_label("this match does not cover every possible value")
                .with_help("add the missing arm(s), or a wildcard `_` to match anything else"),
        );
    }

    /// Reported by [`Typeck::check_match_exhaustive`] for a scrutinee type this check does not
    /// enumerate on its own (anything but `bool` or an enum): the only way it can know every arm
    /// is accounted for is a catch-all.
    fn report_match_needs_wildcard(&self, span: SrcSpan) {
        DiagCtx::emit(
            Diagnostic::error("match is not exhaustive: some values are not covered", span)
                .with_label("no arm covers every remaining value")
                .with_help("add a wildcard `_` (or binding) arm to match anything else"),
        );
    }

    fn report_payload_shape(&self, variant: Ident, span: SrcSpan, found: &VariantDef) {
        let declared = found.payload.describe();
        DiagCtx::emit(
            Diagnostic::error(
                format!(
                    "variant `{}` carries {declared}",
                    Interner::resolve(variant.text)
                ),
                span,
            )
            .with_label(format!("written with a payload that is not {declared}"))
            .with_secondary(self.hir.variant(found.id).span, "declared here"),
        );
    }
}

enum PayloadIds {
    None,
    Single(HirId),
    Record(Vec<(Ident, HirId)>),
}

#[cfg(test)]
mod tests {
    use crate::testing::{typeck_accepts as accepts, typeck_rejects as rejects};

    /// A pattern is checked against the type it matches, so a literal in one has to be able to be
    /// that type -- which is the same question `1 == x` asks, asked in the other direction.
    #[test]
    fn a_literal_pattern_has_to_match_the_scrutinees_type() {
        accepts("fun f(n: i32) -> i32 { return match n { 0 => 1, _ => 2, }; }");
        rejects(
            "fun f(n: i32) -> i32 { return match n { true => 1, _ => 2, }; }",
            "mismatched types",
        );
    }

    /// A wildcard says nothing about the type, so it matches whatever it is given and binds
    /// nothing.
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

    /// Unlike a struct literal, a record pattern may name fewer fields than the variant declares:
    /// the ones left out are simply not bound.
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

    /// The payload's declared shape is what a pattern has to be written in: `.circle` carries one
    /// value, so matching it as though it carried none is not a narrower pattern, it is a wrong
    /// one.
    #[test]
    fn a_variant_pattern_written_with_the_wrong_payload_shape_is_reported() {
        rejects(
            "enum Shape { unit, circle: f64 }
             fun f(s: Shape) -> i32 { return match s { .circle => 1, .unit => 2, }; }",
            "carries a single value",
        );
    }

    /// Nesting is what makes pushing the type down rather than working one up the right shape:
    /// each level hands the next what it declared.
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

    /// One mistake, one diagnostic: a pattern below a failed one still binds its names, at
    /// `Error`, so a use of one of them does not report a second time.
    #[test]
    fn a_binding_under_a_failed_pattern_does_not_report_again() {
        rejects(
            "enum Shape { unit }
             fun f(s: Shape) -> i32 { return match s { .square(r) => r, .unit => 1, }; }",
            "no variant `square`",
        );
    }

    // -----------------------------------------------------------------
    // Exhaustiveness -- see `Typeck::check_match_exhaustive` for exactly how much this does and
    // does not check.
    // -----------------------------------------------------------------

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

    /// Neither a tuple nor a struct is enumerated by this check -- see
    /// `Typeck::check_match_exhaustive`'s doc comment -- so both need an explicit catch-all no
    /// matter how many combinations the arms already spell out.
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

    /// One mistake, one diagnostic: a `match` with an unresolvable arm is not also accused of
    /// leaving a variant uncovered.
    #[test]
    fn an_unknown_variant_does_not_also_trigger_an_exhaustiveness_diagnostic() {
        rejects(
            "enum Shape { unit, circle: f64 }
             fun f(s: Shape) -> i32 { return match s { .square => 1, .unit => 2, }; }",
            "no variant `square`",
        );
    }

    // -----------------------------------------------------------------
    // Nested patterns, more deeply
    // -----------------------------------------------------------------

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

    /// A variant pattern nested two levels deep inside another variant pattern -- `Option<Shape>`
    /// matched all the way down to `Shape`'s own payload.
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
        // The first arm pins the match's result type to `f64` (from `r`, `.circle`'s payload);
        // the second arm's `true` then disagrees with that -- exactly one mismatch, on the one
        // arm that is actually wrong.
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

    /// A generic enum with two type parameters (`Result`-shaped) matches through both.
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

    /// A record payload pattern nested inside a tuple pattern.
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
