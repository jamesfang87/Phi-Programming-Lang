use crate::ast::Ident;
use crate::diagnostics::Diagnostic;
use crate::diagnostics::codes;
use crate::diagnostics::display::DisplayCtx;
use crate::driver::source::SrcSpan;
use crate::hir::{Hir, HirId};
use crate::session::Session;
use crate::typeck::ty::Ty;
use crate::typeck::ty::unify::UnifyError;

pub fn report_not_assignable(session: &Session, span: SrcSpan) {
    session.emit(
        Diagnostic::error("this expression cannot be assigned to", span)
            .with_code(codes::NOT_ASSIGNABLE)
            .with_label("not a place")
            .with_help("the left side of an assignment has refer to a place"),
    );
}

pub fn report_assign_mismatch(cx: DisplayCtx<'_>, err: UnifyError, span: SrcSpan) {
    cx.emit_unify(
        err,
        span,
        "this value cannot be assigned to the place on the left",
    );
}

pub fn report_compound_assign_mismatch(cx: DisplayCtx<'_>, err: UnifyError, span: SrcSpan) {
    cx.emit_unify(
        err,
        span,
        "both sides of a compound assignment must have the same type",
    );
}

pub fn report_compound_assign_result_mismatch(cx: DisplayCtx<'_>, err: UnifyError, span: SrcSpan) {
    cx.emit_unify(
        err,
        span,
        "this operator does not produce the type it would be assigned back to",
    );
}

pub fn report_deref_not_a_reference(cx: DisplayCtx<'_>, ty: Ty, span: SrcSpan) {
    cx.emit(
        Diagnostic::error(format!("`{}` cannot be dereferenced", cx.show(ty)), span)
            .with_code(codes::DEREF_NOT_A_REFERENCE)
            .with_label("not a reference or owned pointer type")
            .with_help("`*` only applies to a value of type `&T`, `&mut T`, or `iso T`"),
    );
}

pub fn report_move_out_of_reference(cx: DisplayCtx<'_>, ty: Ty, span: SrcSpan) {
    cx.emit(
        Diagnostic::error(
            format!(
                "cannot move a value of type `{}` out of a reference",
                cx.show(ty)
            ),
            span,
        )
        .with_code(codes::MOVE_OUT_OF_REFERENCE)
        .with_label("this reference does not own the value it points to")
        .with_help(
            "a `&T`/`&mut T` you don't own can only be read through, not moved out of -- \
                 implement `Copy` for the type, or dereference an owned pointer (`iso T`) \
                 instead",
        ),
    );
}

pub fn report_index_base_unknown(session: &Session, span: SrcSpan) {
    session.emit(
        Diagnostic::error(
            "type annotations needed: the type being indexed is still unknown",
            span,
        )
        .with_code(codes::INDEX_BASE_UNKNOWN)
        .with_label("the type here is still unknown"),
    );
}

pub fn report_index_not_int(cx: DisplayCtx<'_>, err: UnifyError, span: SrcSpan) {
    cx.emit_unify(err, span, "an array is indexed by an integer");
}

pub fn report_new_array_count_not_usize(cx: DisplayCtx<'_>, err: UnifyError, span: SrcSpan) {
    cx.emit_unify(err, span, "`new [elem; count]`'s count is a `usize`");
}

pub fn report_reference_in_new(cx: DisplayCtx<'_>, ty: Ty, span: SrcSpan) {
    cx.emit(
        Diagnostic::error(
            format!("`new` cannot store a reference, found `{}`", cx.show(ty)),
            span,
        )
        .with_code(codes::REFERENCE_IN_NEW)
        .with_label("this type stores a reference"),
    );
}

pub fn report_owned_element_in_new_array(cx: DisplayCtx<'_>, ty: Ty, span: SrcSpan) {
    cx.emit(
        Diagnostic::error(
            format!(
                "`new [elem; count]` cannot repeat an owning element, found `{}`",
                cx.show(ty)
            ),
            span,
        )
        .with_code(codes::OWNED_ELEMENT_IN_NEW_ARRAY)
        .with_label("this type owns an allocation, so it cannot be copied into every slot"),
    );
}

pub fn report_not_indexable(cx: DisplayCtx<'_>, base: Ty, span: SrcSpan) {
    cx.emit(
        Diagnostic::error(format!("`{}` cannot be indexed", cx.show(base)), span)
            .with_code(codes::NOT_INDEXABLE)
            .with_label("no `index` method on this type"),
    );
}

pub fn report_elided_ctor_unknown(session: &Session, span: SrcSpan) {
    session.emit(
        Diagnostic::error(
            "type annotations needed, cannot deduce the type of `.{ .. }`",
            span,
        )
        .with_code(codes::ELIDED_CTOR_UNKNOWN)
        .with_label("cannot tell which struct this builds")
        .with_help(
            "consider writing the struct's name or give the surrounding binding, parameter, or \
                 return type an annotation",
        ),
    );
}

pub fn report_ctor_not_a_struct(session: &Session, span: SrcSpan) {
    session.emit(
        Diagnostic::error("only a struct can be built with `{ .. }`", span)
            .with_code(codes::CTOR_NOT_A_STRUCT)
            .with_label("not a struct"),
    );
}

pub fn report_not_a_struct_literal(cx: DisplayCtx<'_>, ty: Ty, span: SrcSpan) {
    cx.emit(
        Diagnostic::error(format!("`{}` is not a struct", cx.show(ty)), span)
            .with_code(codes::NOT_A_STRUCT_LITERAL)
            .with_label("only a struct is built with `{ .. }`")
            .with_help("an enum variant is built with `.variant`, not with a struct literal"),
    );
}

pub fn report_duplicate_field(session: &Session, field: Ident) {
    session.emit(
        Diagnostic::error(
            format!(
                "field `{}` is given a value twice",
                session.resolve(field.text)
            ),
            field.span,
        )
        .with_code(codes::DUPLICATE_FIELD_VALUE)
        .with_label("already given a value above"),
    );
}

pub fn report_field_type_mismatch(cx: DisplayCtx<'_>, err: UnifyError, span: SrcSpan) {
    cx.emit_unify(
        err,
        span,
        "this value does not match the field's declared type",
    );
}

pub fn report_missing_fields(cx: DisplayCtx<'_>, missing: &[&str], ty: Ty, span: SrcSpan) {
    cx.emit(
        Diagnostic::error(
            format!("`{}` is missing {}", cx.show(ty), list(missing)),
            span,
        )
        .with_code(codes::MISSING_FIELDS)
        .with_label("every field has to be given a value"),
    );
}

pub fn report_variant_enum_unknown(session: &Session, variant: Ident, span: SrcSpan) {
    session.emit(
        Diagnostic::error(
            format!(
                "type annotations needed: the enum `.{}` belongs to is unknown here",
                session.resolve(variant.text)
            ),
            span,
        )
        .with_code(codes::VARIANT_ENUM_UNKNOWN)
        .with_label("cannot tell which enum this variant belongs to"),
    );
}

pub fn report_variant_base_not_a_type(session: &Session, span: SrcSpan) {
    session.emit(
        Diagnostic::error("type of base is not an enum", span)
            .with_code(codes::VARIANT_BASE_NOT_A_TYPE)
            .with_label("this names a value, not an enum"),
    );
}

pub fn report_variant_payload_mismatch(cx: DisplayCtx<'_>, err: UnifyError, span: SrcSpan) {
    cx.emit_unify(
        err,
        span,
        "this value does not match the variant's declared payload",
    );
}

pub fn report_variant_expr_payload_shape(
    session: &Session,
    hir: &Hir,
    variant: Ident,
    span: SrcSpan,
    declared: &str,
    variant_id: HirId,
) {
    session.emit(
        Diagnostic::error(
            format!(
                "variant `{}` carries {declared}",
                session.resolve(variant.text)
            ),
            span,
        )
        .with_code(codes::VARIANT_EXPR_PAYLOAD_SHAPE)
        .with_label(format!("built with a payload that is not {declared}"))
        .with_secondary(hir.variant(variant_id).span, "declared here"),
    );
}

pub fn report_record_field_unknown(session: &Session, hir: &Hir, field: Ident, variant: HirId) {
    session.emit(
        Diagnostic::error(
            format!("no field `{}` on this variant", session.resolve(field.text)),
            field.span,
        )
        .with_code(codes::RECORD_FIELD_UNKNOWN)
        .with_label("not declared by this variant")
        .with_secondary(hir.variant(variant).span, "declared here"),
    );
}

pub fn report_variant_missing_fields(
    session: &Session,
    hir: &Hir,
    variant: HirId,
    missing: &[&str],
) {
    session.emit(
        Diagnostic::error(
            format!("this variant's payload is missing {}", list(missing)),
            hir.variant(variant).span,
        )
        .with_code(codes::VARIANT_MISSING_FIELDS)
        .with_label("every declared field has to be given a value"),
    );
}

pub fn report_if_cond_not_bool(cx: DisplayCtx<'_>, err: UnifyError, span: SrcSpan) {
    cx.emit_unify(err, span, "an `if` condition has to be a `bool`");
}

pub fn report_if_no_else_mismatch(cx: DisplayCtx<'_>, err: UnifyError, span: SrcSpan) {
    cx.emit(
        Diagnostic::error(cx.show(err).to_string(), span)
            .with_code(codes::IF_NO_ELSE_MISMATCH)
            .with_label("an `if` with no `else` produces no value"),
    );
}

pub fn report_if_branches_mismatch(cx: DisplayCtx<'_>, err: UnifyError, span: SrcSpan) {
    cx.emit_unify(
        err,
        span,
        "both branches of an `if` have to produce the same type",
    );
}

pub fn report_assert_cond_not_bool(cx: DisplayCtx<'_>, err: UnifyError, span: SrcSpan) {
    cx.emit_unify(err, span, "an `assert` condition has to be a `bool`");
}

pub fn report_panic_message_not_str(cx: DisplayCtx<'_>, err: UnifyError, span: SrcSpan) {
    cx.emit_unify(err, span, "a panic message has to be a `str`");
}

pub fn report_match_arm_mismatch(cx: DisplayCtx<'_>, err: UnifyError, arm_span: SrcSpan) {
    cx.emit_unify(
        err,
        arm_span,
        "every arm of a `match` has to produce the same type",
    );
}

pub fn report_match_guard_not_bool(cx: DisplayCtx<'_>, err: UnifyError, span: SrcSpan) {
    cx.emit_unify(err, span, "a match guard has to be a `bool`");
}

pub fn report_try_operand_unknown(session: &Session, span: SrcSpan) {
    session.emit(
        Diagnostic::error(
            "type annotations needed: the type `?` is applied to is still unknown",
            span,
        )
        .with_code(codes::TRY_OPERAND_UNKNOWN)
        .with_label("the type here is still unknown")
        .with_help("`?` produces what a `Result` or an `Option` carries, so it needs one"),
    );
}

pub fn report_not_try(cx: DisplayCtx<'_>, ty: Ty, span: SrcSpan) {
    cx.emit(
        Diagnostic::error(format!("`?` cannot be applied to `{}`", cx.show(ty)), span)
            .with_code(codes::NOT_TRY)
            .with_label("not a `Result` or an `Option`")
            .with_help(
                "`?` takes the value out of a `Result` or an `Option`, propagating the rest",
            ),
    );
}

pub fn report_untrusted_intrinsic(session: &Session, span: SrcSpan) {
    session.emit(
        Diagnostic::error(
            "this intrinsic may only be called from the standard library",
            span,
        )
        .with_code(codes::UNTRUSTED_INTRINSIC)
        .with_help("use of untrusted instrinsics may not be used out of standard library."),
    );
}

pub fn report_try_outside(cx: DisplayCtx<'_>, operand_ty: Ty, span: SrcSpan) {
    cx.emit(
        Diagnostic::error(
            format!(
                "`?` on `{}` has nowhere to propagate to",
                cx.show(operand_ty)
            ),
            span,
        )
        .with_code(codes::TRY_OUTSIDE)
        .with_label("the enclosing definition declares no return type"),
    );
}

pub fn report_try_return_mismatch(cx: DisplayCtx<'_>, operand_ty: Ty, ret: Ty, span: SrcSpan) {
    cx.emit(
        Diagnostic::error(
            format!(
                "`?` on `{}` cannot propagate out of a function returning `{}`",
                cx.show(operand_ty),
                cx.show(ret)
            ),
            span,
        )
        .with_code(codes::TRY_RETURN_MISMATCH)
        .with_label("the two are not the same kind of value"),
    );
}

pub fn report_try_error_mismatch(cx: DisplayCtx<'_>, err: UnifyError, span: SrcSpan) {
    cx.emit_unify(
        err,
        span,
        "`?` propagates this error out of the function, whose declared error type it has to match",
    );
}

pub fn report_break_outside_loop(session: &Session, span: SrcSpan) {
    session.emit(
        Diagnostic::error("`break` outside a loop", span)
            .with_code(codes::BREAK_OUTSIDE_LOOP)
            .with_label("there is no loop for this to exit")
            .with_help("move this statement into a `while` loop in the same function"),
    );
}

pub fn report_continue_outside_loop(session: &Session, span: SrcSpan) {
    session.emit(
        Diagnostic::error("`continue` outside a loop", span)
            .with_code(codes::CONTINUE_OUTSIDE_LOOP)
            .with_label("there is no loop for this to continue")
            .with_help("move this statement into a `while` loop in the same function"),
    );
}

pub fn report_closure_body_mismatch(cx: DisplayCtx<'_>, err: UnifyError, span: SrcSpan) {
    cx.emit_unify(
        err,
        span,
        "this closure's body does not produce the return type it was checked against",
    );
}

pub fn report_cast_target_not_primitive(cx: DisplayCtx<'_>, ty: Ty, span: SrcSpan) {
    cx.emit(
        Diagnostic::error(format!("cannot cast to `{}`", cx.show(ty)), span)
            .with_code(codes::CAST_TARGET_NOT_PRIMITIVE)
            .with_label("not a primitive type")
            .with_help("`as` can only convert between primitive types"),
    );
}

pub fn report_cast_source_not_primitive(cx: DisplayCtx<'_>, ty: Ty, span: SrcSpan) {
    cx.emit(
        Diagnostic::error(
            format!("cannot cast a value of type `{}`", cx.show(ty)),
            span,
        )
        .with_code(codes::CAST_SOURCE_NOT_PRIMITIVE)
        .with_label("not a primitive type")
        .with_help("`as` can only convert between primitive types"),
    );
}

pub fn report_cast_operand_unknown(session: &Session, span: SrcSpan) {
    session.emit(
        Diagnostic::error(
            "type annotations needed: the type being cast is still unknown",
            span,
        )
        .with_code(codes::CAST_OPERAND_UNKNOWN)
        .with_label("the type here is still unknown"),
    );
}

pub fn report_cast_not_allowed(cx: DisplayCtx<'_>, from: Ty, to: Ty, reason: &str, span: SrcSpan) {
    cx.emit(
        Diagnostic::error(
            format!("cannot cast `{}` to `{}`", cx.show(from), cx.show(to)),
            span,
        )
        .with_code(codes::CAST_NOT_ALLOWED)
        .with_label(reason.to_string())
        .with_help("`as` does not allow lossy conversions"),
    );
}

fn list(names: &[&str]) -> String {
    let quoted: Vec<String> = names.iter().map(|name| format!("`{name}`")).collect();
    match quoted.split_last() {
        None => String::new(),
        Some((last, [])) => format!("field {last}"),
        Some((last, rest)) => format!("fields {} and {last}", rest.join(", ")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ast::Ident;
    use crate::driver::source::SrcSpan;
    use crate::testing::{intern, session};

    fn ident(name: &str) -> Ident {
        Ident {
            text: intern(name),
            span: SrcSpan::new(0, 1),
        }
    }

    #[test]
    fn a_repeated_field_value_names_the_field() {
        session().clear_diagnostics();

        report_duplicate_field(session(), ident("value"));

        assert_eq!(
            session().messages(),
            ["field `value` is given a value twice"]
        );
    }

    #[test]
    fn an_untrusted_intrinsic_explains_the_library_restriction() {
        session().clear_diagnostics();

        report_untrusted_intrinsic(session(), SrcSpan::new(0, 1));

        assert_eq!(
            session().messages(),
            ["this intrinsic may only be called from the standard library"]
        );
    }
}
