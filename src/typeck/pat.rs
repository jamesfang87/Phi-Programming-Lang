//! Checking a pattern against the type of the value it is matched against, and the two
//! declaration lookups -- an enum's variant, a struct's fields -- that both patterns and the
//! expressions that build the same shapes need.
//!
//! A pattern is checked *against* a type rather than having one worked out for it. That is the
//! whole difference from [`check_expr`](Typeck::check_expr): an expression's type comes from its
//! parts, while `let x = 1` has nothing to say about what `x` is until the initializer does. So
//! [`Typeck::check_pat`] takes the type it is checked against and pushes it down, which is also
//! what gives a binding its type -- [`PatKind::Binding`] records the type it was handed, and
//! [`Res::Local(Local::Variable(..))`](crate::hir::Local::Variable) addresses exactly that
//! `Node::Pat`, so a later use of the name reads it straight back out.

use std::collections::HashMap;

use crate::ast::interner::Interner;
use crate::ast::{Ident, Symbol};
use crate::diag::{DiagCtx, Diagnostic};
use crate::driver::source::SrcSpan;
use crate::hir::{HirId, Node, OwnerNode, PatKind, Payload, VariantPayload};
use crate::typeck::Typeck;
use crate::typeck::ty::{Ty, TyKind};

/// One enum variant found by name on an enum type, with its declared payload already read through
/// that type's own generic arguments -- so the payload of `some` on `Option<i32>` is `i32`, not
/// `T`.
pub(crate) struct VariantDef {
    /// The `Node::Variant` that declares it, for pointing a diagnostic at the declaration.
    pub id: HirId,
    pub payload: VariantTys,
}

/// What a variant carries, in the same three shapes [`VariantPayload`] declares.
pub(crate) enum VariantTys {
    Unit,
    Single(Ty),
    Record(Vec<(Ident, Ty)>),
}

impl VariantTys {
    /// How this payload reads in a diagnostic that says a use site got its shape wrong.
    pub(crate) fn describe(&self) -> &'static str {
        match self {
            VariantTys::Unit => "no payload",
            VariantTys::Single(_) => "a single value",
            VariantTys::Record(_) => "named fields",
        }
    }
}

impl<'hir> Typeck<'hir> {
    /// Checks `id` against `expected`, recording a type for it and for every pattern nested inside
    /// it.
    ///
    /// Every pattern gets a recorded type, not only the ones that bind: the table is what the
    /// `--debug` dump and every later pass read, and a `_` or a literal in the middle of a tuple
    /// is as much a node as the binding beside it.
    pub(crate) fn check_pat(&mut self, id: HirId, expected: Ty) {
        let Node::Pat(pat) = self.hir.node(id) else {
            unreachable!("Node that is not a pattern passed to check_pat");
        };
        let span = pat.span;

        // A pattern below an already-failed one is still walked, so that the names it binds have
        // types and a use of one does not produce a second, unrelated error. `Error` propagates
        // down instead of each level reporting again.
        if matches!(self.tcx.kind(expected), TyKind::Error) {
            self.types.record(id, expected);
            for child in self.pat_children(id) {
                self.check_pat(child, expected);
            }
            return;
        }

        let ty = match &pat.kind {
            // A binding takes whatever it is matched against; a wildcard binds nothing but is
            // still that type.
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
                // The arity is the whole of what the pattern says about the type, so it is stated
                // as a tuple of fresh variables and unified. A mismatch is then one diagnostic
                // naming both tuples, rather than one per element that failed to line up.
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

    /// Checks `.circle(r)`, `.square { l }`, or a bare `.none` against the type being matched.
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
            // The variant exists but is not built the way the pattern writes it: `.circle` with no
            // payload against a `circle: f64`, or `.circle { r }` against the same.
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

        // Unlike a struct literal, a pattern that names fewer fields than the variant declares is
        // not an error: the ones left out are simply not bound.
    }

    /// Walks the sub-patterns of a payload that has already been reported on, so that the names
    /// they bind still have types.
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

    /// The sub-patterns of `id`, read out so the borrow of the node ends before any of them is
    /// checked.
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

    /// Copies a payload's ids out of the node, for the same reason as [`Typeck::pat_children`].
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
    //
    // Both of these are used from `expr` as well: `.circle(1.0)` and `.circle(r)` ask the same
    // question of the same enum, and differ only in what they do with the answer.
    // -----------------------------------------------------------------

    /// Looks `name` up as a variant of the enum `ty` names.
    ///
    /// `None` covers both "not an enum" and "no such variant", because the caller reports them the
    /// same way -- it holds the span and knows whether it is checking a pattern or building a
    /// value, and this does not.
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

        // A variant's declared payload is written in the enum's own terms, so it is read through
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

    /// The fields the struct `ty` names declares, each read through that type's own generic
    /// arguments. `None` if `ty` is not a struct.
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
    /// variable. Unlike a trait bound this cannot be deferred: which enum is meant decides what
    /// the pattern binds, and everything after it is checked against that.
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

/// A payload's child ids, copied out of the node that holds them.
///
/// [`Payload`] borrows the arena, and every arm below needs `&mut self` to check what it found, so
/// the ids are taken first. The field names come along for a record payload because matching them
/// against the declaration is the point.
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
        accepts(
            "fun f(n: i32) -> i32 { return match n { 0 => 1, _ => 2, }; }",
        );
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
}
