use crate::ast::Ident;
use crate::diagnostics::Diagnostic;
use crate::diagnostics::codes;
use crate::diagnostics::display::DisplayCtx;
use crate::driver::source::SrcSpan;
use crate::hir::{Hir, HirId};
use crate::session::Session;
use crate::typeck::check::pat::ResolvedVariant;
use crate::typeck::ty::unify::UnifyError;

pub fn report_literal_pattern_mismatch(cx: DisplayCtx<'_>, err: UnifyError, span: SrcSpan) {
    cx.emit_unify(
        err,
        span,
        "this literal cannot match a value of the type being matched",
    );
}

pub fn report_or_pattern_binding(session: &Session, span: SrcSpan) {
    session.emit(
        Diagnostic::error("or-patterns cannot bind names", span)
            .with_code(codes::OR_PATTERN_BINDING)
            .with_label("a binding inside `|` has no single value to refer to")
            .with_help("use separate match arms, or bind the whole value outside the pattern"),
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
        .with_label("cannot tell which enum this variant belongs to"),
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

pub fn report_let_else_does_not_diverge(session: &Session, span: SrcSpan) {
    session.emit(
        Diagnostic::error("this `else` block does not diverge", span)
            .with_code(codes::LET_ELSE_DOES_NOT_DIVERGE)
            .with_label("control flow can reach the end of the block")
            .with_help(
                "a `let ... else` block has to leave the scope with `return`, `break`, \
                 `continue`, `panic`, or `unreachable`, because its pattern did not match",
            ),
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::hir::OwnerNode;
    use crate::testing::{intern, lower_to_hir, named_def, session};

    fn ident(name: &str) -> Ident {
        Ident {
            text: intern(name),
            span: SrcSpan::new(0, 1),
        }
    }

    #[test]
    fn an_or_pattern_binding_is_reported() {
        session().clear_diagnostics();

        report_or_pattern_binding(session(), SrcSpan::new(0, 1));

        assert_eq!(session().messages(), ["or-patterns cannot bind names"]);
    }

    #[test]
    fn an_unknown_variant_type_names_the_variant() {
        session().clear_diagnostics();

        report_variant_type_unknown(session(), ident("some"), SrcSpan::new(0, 1));

        assert_eq!(
            session().messages(),
            ["type annotations needed: the type `.some` is matched against is still unknown"]
        );
    }

    #[test]
    fn a_let_else_block_that_can_fall_through_is_reported() {
        session().clear_diagnostics();

        report_let_else_does_not_diverge(session(), SrcSpan::new(0, 1));

        assert_eq!(session().messages(), ["this `else` block does not diverge"]);
    }

    #[test]
    fn a_payload_field_is_looked_up_on_the_variant() {
        let hir = lower_to_hir("enum E { some: { value: i32 } }");
        let enum_def = named_def(&hir, "E");
        let OwnerNode::Enum(enumeration) = hir.def(enum_def) else {
            panic!("E is an enum");
        };
        let variant = enumeration.variants[0];
        session().clear_diagnostics();

        report_no_payload_field(session(), &hir, ident("missing"), variant);

        assert_eq!(
            session().messages(),
            ["no field `missing` on variant `some`"]
        );
    }
}
