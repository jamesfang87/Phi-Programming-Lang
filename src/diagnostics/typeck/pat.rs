use crate::ast::interner::Interner;
use crate::ast::Ident;
use crate::diag::{DiagCtx, Diagnostic};
use crate::diagnostics::typeck::display::DisplayCx;
use crate::driver::source::SrcSpan;
use crate::hir::{Hir, HirId};
use crate::typeck::pat::VariantDef;
use crate::typeck::ty::Ty;
use crate::typeck::unify::UnifyError;

pub fn report_literal_pattern_mismatch(cx: DisplayCx<'_>, err: UnifyError, span: SrcSpan) {
    DiagCtx::emit(
        Diagnostic::error(cx.show(err).to_string(), span)
            .with_label("this literal cannot match a value of the type being matched"),
    );
}

pub fn report_tuple_pattern_mismatch(cx: DisplayCx<'_>, err: UnifyError, span: SrcSpan) {
    DiagCtx::emit(
        Diagnostic::error(cx.show(err).to_string(), span)
            .with_label("this tuple pattern does not match the value's type"),
    );
}

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

pub fn report_match_needs_wildcard(span: SrcSpan) {
    DiagCtx::emit(
        Diagnostic::error("match is not exhaustive: some values are not covered", span)
            .with_label("no arm covers every remaining value")
            .with_help("add a wildcard `_` (or binding) arm to match anything else"),
    );
}

pub fn report_refutable_let_without_else(span: SrcSpan) {
    DiagCtx::emit(
        Diagnostic::error("refutable pattern in a `let` with no `else`", span)
            .with_label("this pattern does not match every value of its type")
            .with_help("add an `else { .. }` block to handle the case it doesn't match"),
    );
}

pub fn report_irrefutable_let_with_else(span: SrcSpan) {
    DiagCtx::emit(
        Diagnostic::error("irrefutable pattern in a `let` with an `else`", span)
            .with_label("this pattern always matches, so this `else` block is unreachable")
            .with_help("remove the `else` block"),
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
