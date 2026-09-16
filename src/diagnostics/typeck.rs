pub mod expr;
pub mod lower_ty;
pub mod pat;
pub mod traits;

use crate::ast::interner::Symbol;
use crate::ast::{Ident, SelfMode};
use crate::diagnostics::Diagnostic;
use crate::diagnostics::codes;
use crate::diagnostics::display::DisplayCtx;
use crate::driver::source::SrcSpan;
use crate::session::Session;
use crate::typeck::ty::Ty;
use crate::typeck::ty::unify::UnifyError;

pub fn show_self_mode(mode: SelfMode) -> &'static str {
    match mode {
        SelfMode::Immutable => "`&self`",
        SelfMode::Mutable => "`&mut self`",
        SelfMode::Move => "`self`",
        SelfMode::Any => "`any self`",
    }
}

pub fn show_optional_self_mode(mode: Option<SelfMode>) -> &'static str {
    mode.map_or("no receiver", show_self_mode)
}

pub fn report_private_field(session: &Session, member: Ident) {
    session.emit(
        Diagnostic::error(
            format!("field `{}` is private", session.resolve(member.text)),
            member.span,
        )
        .with_code(codes::PRIVATE_FIELD)
        .with_label("not visible from here")
        .with_help("mark the field `public` to use it outside its declaring module"),
    );
}

pub fn report_duplicate_field(cx: DisplayCtx<'_>, field: Ident) {
    cx.emit(
        Diagnostic::error(
            format!(
                "field `{}` is declared more than once",
                cx.resolve(field.text)
            ),
            field.span,
        )
        .with_code(codes::DUPLICATE_FIELD)
        .with_label("duplicate field")
        .with_help("give each field a distinct name"),
    );
}

pub fn report_recursive_type(cx: DisplayCtx<'_>, name: Ident) {
    cx.emit(
        Diagnostic::error(
            format!(
                "recursive type `{}` has infinite size",
                cx.resolve(name.text)
            ),
            name.span,
        )
        .with_code(codes::RECURSIVE_TYPE)
        .with_label("this type contains itself by value")
        .with_help("put the recursive field behind `iso` to give it a fixed size"),
    );
}

pub fn report_no_field(cx: DisplayCtx<'_>, member: Ident, base: Ty) {
    cx.emit(
        Diagnostic::error(
            format!(
                "no field `{}` on `{}`",
                cx.resolve(member.text),
                cx.show(base)
            ),
            member.span,
        )
        .with_code(codes::NO_FIELD)
        .with_label("not a field of this type"),
    );
}

pub fn report_no_variant(cx: DisplayCtx<'_>, variant: Ident, ty: Ty) {
    cx.emit(
        Diagnostic::error(
            format!(
                "no variant `{}` on `{}`",
                cx.resolve(variant.text),
                cx.show(ty)
            ),
            variant.span,
        )
        .with_code(codes::NO_VARIANT)
        .with_label("not a variant of this type"),
    );
}

pub fn report_return_mismatch(cx: DisplayCtx<'_>, err: UnifyError, span: SrcSpan) {
    cx.emit_unify(
        err,
        span,
        "returned value does not match this function's return type",
    );
}

pub fn report_bodiless_function(session: &Session, span: SrcSpan) {
    session.emit(
        Diagnostic::error("this function has no body", span)
            .with_code(codes::BODILESS_FUNCTION)
            .with_label("a function declared here must have a body")
            .with_help(
                "only the compiler's own intrinsics may omit one, and only when declared in \
                 the core library",
            ),
    );
}

pub fn report_operand_has_unknown_type(session: &Session, span: SrcSpan) {
    session.emit(
        Diagnostic::error(
            "type annotations needed: the type this operator is applied to is still unknown",
            span,
        )
        .with_code(codes::OPERAND_UNKNOWN_TYPE)
        .with_label("the type here is still unknown")
        .with_help(
            "which `extend .. with` block this operator would dispatch to depends on the \
                 type it is applied to, and unlike a trait bound that cannot wait for a later \
                 pass -- write the type out",
        ),
    );
}

pub fn report_binary_operand_mismatch(
    cx: DisplayCtx<'_>,
    err: UnifyError,
    lhs: Ty,
    rhs: Ty,
    span: SrcSpan,
) {
    cx.emit_unify(
        err,
        span,
        format!(
            "cannot use incompatible types {} and {} in binary operation",
            cx.show(lhs),
            cx.show(rhs)
        ),
    );
}

pub fn report_logic_op_needs_bool_operands(
    cx: DisplayCtx<'_>,
    err: UnifyError,
    operand: Ty,
    span: SrcSpan,
) {
    cx.emit_unify(
        err,
        span,
        format!("`&&`/`||` need bool operands, found {}", cx.show(operand)),
    );
}

pub fn report_unknown_literal_suffix(session: &Session, suffix: Symbol, span: SrcSpan) {
    session.emit(
        Diagnostic::error(
            format!("invalid literal suffix `{}`", session.resolve(suffix)),
            span,
        )
        .with_code(codes::UNKNOWN_LITERAL_SUFFIX)
        .with_label("not the name of a numeric type")
        .with_help(
            "a literal suffix must name a numeric type: `i8`, `i16`, `i32`, `i64`, `u8`, \
                 `u16`, `u32`, `u64`, `f32`, or `f64`",
        ),
    );
}

pub fn report_int_suffix_on_float_literal(session: &Session, suffix: Symbol, span: SrcSpan) {
    session.emit(
        Diagnostic::error(
            format!(
                "literal has a fractional part but is suffixed `{}`",
                session.resolve(suffix)
            ),
            span,
        )
        .with_code(codes::INT_SUFFIX_ON_FLOAT_LITERAL)
        .with_label(format!(
            "`{}` cannot hold a fractional value",
            session.resolve(suffix)
        ))
        .with_help("use a float suffix instead (`f32` or `f64`), or drop the fractional part"),
    );
}

pub fn report_integer_literal_out_of_range(
    cx: DisplayCtx<'_>,
    literal: &str,
    ty: Ty,
    span: SrcSpan,
) {
    cx.emit(
        Diagnostic::error(
            format!(
                "integer literal `{literal}` is out of range for `{}`",
                cx.show(ty)
            ),
            span,
        )
        .with_code(codes::INTEGER_LITERAL_OUT_OF_RANGE)
        .with_label("this value does not fit its type"),
    );
}

pub fn report_negative_literal_for_unsigned_type(cx: DisplayCtx<'_>, ty: Ty, span: SrcSpan) {
    cx.emit(
        Diagnostic::error(
            format!(
                "`{}` is unsigned, so it has no negative values",
                cx.show(ty)
            ),
            span,
        )
        .with_code(codes::NEGATIVE_LITERAL_FOR_UNSIGNED)
        .with_label("this negation cannot be represented"),
    );
}

pub fn report_binding_type_mismatch(cx: DisplayCtx<'_>, err: UnifyError, span: SrcSpan) {
    cx.emit_unify(
        err,
        span,
        "the value this binding is given does not match its declared type",
    );
}

pub fn report_body_return_mismatch(cx: DisplayCtx<'_>, err: UnifyError, span: SrcSpan) {
    cx.emit_unify(
        err,
        span,
        "this function does not return its declared return type on every path",
    );
}

pub fn report_any_outside_signature(cx: DisplayCtx<'_>, ty: Ty, span: SrcSpan) {
    cx.emit(
        Diagnostic::error(
            format!(
                "`any` may only appear in a parameter or return type, found `{}`",
                cx.show(ty)
            ),
            span,
        )
        .with_code(codes::ANY_OUTSIDE_SIGNATURE)
        .with_label("this type carries `any` outside a function signature")
        .with_help(
            "`any` describes how a function accepts or hands back a value, not a type a \
                 field, binding, or generic argument can hold; give the field or binding a \
                 concrete type instead",
        ),
    );
}

pub fn report_reference_field(cx: DisplayCtx<'_>, ty: Ty, span: SrcSpan) {
    cx.emit(
        Diagnostic::error(
            format!("a field cannot hold a reference, found `{}`", cx.show(ty)),
            span,
        )
        .with_code(codes::REFERENCE_FIELD)
        .with_label("this type stores a reference")
        .with_help(
            "a struct or enum field has to own the value it holds; take the field by value \
                 instead of by reference",
        ),
    );
}
