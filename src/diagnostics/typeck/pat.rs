use crate::ast::Ident;
use crate::ast::interner::Interner;
use crate::diag::{DiagCtx, Diagnostic};
use crate::diagnostics::typeck::display::DisplayCx;
use crate::driver::source::SrcSpan;
use crate::hir::{Hir, HirId};
use crate::typeck::pat::VariantDef;
use crate::typeck::ty::Ty;
use crate::typeck::unify::UnifyError;

/// A pattern's literal didn't unify with the type it is matched against.
pub fn report_literal_pattern_mismatch(cx: DisplayCx<'_>, err: UnifyError, span: SrcSpan) {
    DiagCtx::emit(
        Diagnostic::error(cx.show(err).to_string(), span)
            .with_label("this literal cannot match a value of the type being matched"),
    );
}

/// A tuple pattern's shape didn't unify with the type it is matched against.
pub fn report_tuple_pattern_mismatch(cx: DisplayCx<'_>, err: UnifyError, span: SrcSpan) {
    DiagCtx::emit(
        Diagnostic::error(cx.show(err).to_string(), span)
            .with_label("this tuple pattern does not match the value's type"),
    );
}

/// Reported when a `.variant` pattern is matched against a type that is still an inference
/// variable.
pub fn report_variant_type_unknown(variant: Ident, span: SrcSpan) {
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
            "a `.variant` names no enum of its own; write the type of the value being matched",
        ),
    );
}

pub fn report_no_variant(cx: DisplayCx<'_>, variant: Ident, ty: Ty) {
    DiagCtx::emit(
        Diagnostic::error(
            format!(
                "no variant `{}` on `{}`",
                Interner::resolve(variant.text),
                cx.show(ty)
            ),
            variant.span,
        )
        .with_label("not a variant of this type"),
    );
}

pub fn report_no_payload_field(hir: &Hir, field: Ident, variant: HirId) {
    DiagCtx::emit(
        Diagnostic::error(
            format!(
                "no field `{}` on variant `{}`",
                Interner::resolve(field.text),
                Interner::resolve(hir.variant(variant).name.text)
            ),
            field.span,
        )
        .with_label("not declared by this variant")
        .with_secondary(hir.variant(variant).span, "declared here"),
    );
}

/// Reported by [`Typeck::check_match_exhaustive`](crate::typeck::Typeck::check_match_exhaustive)
/// when `missing` names the specific values (variant names, or `"true"`/`"false"`) no arm covers.
pub fn report_match_not_exhaustive(span: SrcSpan, missing: &[&str]) {
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

/// Reported by [`Typeck::check_match_exhaustive`](crate::typeck::Typeck::check_match_exhaustive)
/// for a scrutinee type this check does not enumerate on its own (anything but `bool` or an
/// enum): the only way it can know every arm is accounted for is a catch-all.
pub fn report_match_needs_wildcard(span: SrcSpan) {
    DiagCtx::emit(
        Diagnostic::error("match is not exhaustive: some values are not covered", span)
            .with_label("no arm covers every remaining value")
            .with_help("add a wildcard `_` (or binding) arm to match anything else"),
    );
}

pub fn report_payload_shape(hir: &Hir, variant: Ident, span: SrcSpan, found: &VariantDef) {
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
        .with_secondary(hir.variant(found.id).span, "declared here"),
    );
}
