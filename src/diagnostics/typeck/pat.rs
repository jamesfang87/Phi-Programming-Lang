use crate::ast::Ident;
use crate::diagnostics::Diagnostic;
use crate::diagnostics::codes;
use crate::diagnostics::display::DisplayCtx;
use crate::driver::source::SrcSpan;
use crate::hir::{Hir, HirId};
use crate::session::Session;
use crate::typeck::pat::ResolvedVariant;
use crate::typeck::ty::unify::UnifyError;

pub fn report_literal_pattern_mismatch(cx: DisplayCtx<'_>, err: UnifyError, span: SrcSpan) {
    cx.emit_unify(
        err,
        span,
        "this literal cannot match a value of the type being matched",
    );
}

pub fn report_string_pattern_unsupported(session: &Session, span: SrcSpan) {
    session.emit(
        Diagnostic::error("string literal patterns are not supported", span)
            .with_code(codes::STRING_PATTERN_UNSUPPORTED)
            .with_label("a string cannot be matched by value")
            .with_help("bind the value with a binding or variant pattern, then compare it"),
    );
}

pub fn report_tuple_pattern_mismatch(cx: DisplayCtx<'_>, err: UnifyError, span: SrcSpan) {
    cx.emit_unify(
        err,
        span,
        "this tuple pattern does not match the value's type",
    );
}

pub fn report_variant_type_unknown(session: &Session, variant: Ident, span: SrcSpan) {
    session.emit(
        Diagnostic::error(
            format!(
                "type annotations needed: the type `.{}` is matched against is still unknown",
                session.resolve(variant.text)
            ),
            span,
        )
        .with_code(codes::VARIANT_TYPE_UNKNOWN)
        .with_label("cannot tell which enum this variant belongs to")
        .with_help(
            "a `.variant` names no enum of its own; write the type of the value being matched",
        ),
    );
}

pub fn report_no_payload_field(session: &Session, hir: &Hir, field: Ident, variant: HirId) {
    session.emit(
        Diagnostic::error(
            format!(
                "no field `{}` on variant `{}`",
                session.resolve(field.text),
                session.resolve(hir.variant(variant).name.text)
            ),
            field.span,
        )
        .with_code(codes::NO_PAYLOAD_FIELD)
        .with_label("not declared by this variant")
        .with_secondary(hir.variant(variant).span, "declared here"),
    );
}

pub fn report_match_not_exhaustive(session: &Session, span: SrcSpan, missing: &[&str]) {
    let list = missing
        .iter()
        .map(|m| format!("`{m}`"))
        .collect::<Vec<_>>()
        .join(", ");
    session.emit(
        Diagnostic::error(format!("match is not exhaustive: {list} not covered"), span)
            .with_code(codes::NON_EXHAUSTIVE_MATCH)
            .with_label("this match does not cover every possible value")
            .with_help("add the missing arm(s), or a wildcard `_` to match anything else"),
    );
}

pub fn report_match_needs_wildcard(session: &Session, span: SrcSpan) {
    session.emit(
        Diagnostic::error("match is not exhaustive: some values are not covered", span)
            .with_code(codes::MATCH_NEEDS_WILDCARD)
            .with_label("no arm covers every remaining value")
            .with_help("add a wildcard `_` (or binding) arm to match anything else"),
    );
}

pub fn report_refutable_let_without_else(session: &Session, span: SrcSpan) {
    session.emit(
        Diagnostic::error("refutable pattern in a `let` with no `else`", span)
            .with_code(codes::REFUTABLE_LET_WITHOUT_ELSE)
            .with_label("this pattern does not match every value of its type")
            .with_help("add an `else { .. }` block to handle the case it doesn't match"),
    );
}

pub fn report_irrefutable_let_with_else(session: &Session, span: SrcSpan) {
    session.emit(
        Diagnostic::error("irrefutable pattern in a `let` with an `else`", span)
            .with_code(codes::IRREFUTABLE_LET_WITH_ELSE)
            .with_label("this pattern always matches, so this `else` block is unreachable")
            .with_help("remove the `else` block"),
    );
}

pub fn report_payload_shape(
    session: &Session,
    hir: &Hir,
    variant: Ident,
    span: SrcSpan,
    found: &ResolvedVariant,
) {
    let declared = found.payload.describe();
    session.emit(
        Diagnostic::error(
            format!(
                "variant `{}` carries {declared}",
                session.resolve(variant.text)
            ),
            span,
        )
        .with_code(codes::PATTERN_PAYLOAD_SHAPE)
        .with_label(format!("written with a payload that is not {declared}"))
        .with_secondary(hir.variant(found.id).span, "declared here"),
    );
}
