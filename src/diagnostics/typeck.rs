pub mod display;
pub mod expr;
pub mod lower_ty;
pub mod pat;
pub mod traits;

use crate::ast::interner::{Interner, Symbol};
use crate::diagnostics::typeck::display::DisplayCx;
use crate::diagnostics::{DiagCtx, Diagnostic};
use crate::driver::source::SrcSpan;
use crate::typeck::ty::Ty;
use crate::typeck::unify::UnifyError;

pub fn report_return_mismatch(cx: DisplayCx<'_>, err: UnifyError, span: SrcSpan) {
    DiagCtx::emit(
        Diagnostic::error(cx.show(err).to_string(), span).with_label(
            "returned value does not match this \
                function's return type",
        ),
    );
}

pub fn report_bodiless_function(span: SrcSpan) {
    DiagCtx::emit(
        Diagnostic::error("this function has no body", span)
            .with_label("a function declared here must have a body")
            .with_help(
                "only the compiler's own intrinsics may omit one, and only when declared in \
                 the core library",
            ),
    );
}

pub fn report_operand_has_unknown_type(span: SrcSpan) {
    DiagCtx::emit(
        Diagnostic::error(
            "type annotations needed: the type this operator is applied to is still unknown",
            span,
        )
        .with_label("the type here is still unknown")
        .with_help(
            "which `extend .. with` block this operator would dispatch to depends on the \
                 type it is applied to, and unlike a trait bound that cannot wait for a later \
                 pass -- write the type out",
        ),
    );
}

pub fn report_binary_operand_mismatch(
    cx: DisplayCx<'_>,
    err: UnifyError,
    lhs: Ty,
    rhs: Ty,
    span: SrcSpan,
) {
    DiagCtx::emit(
        Diagnostic::error(cx.show(err).to_string(), span).with_label(format!(
            "cannot use incompatible types {} and {} in binary operation",
            cx.show(lhs),
            cx.show(rhs)
        )),
    );
}

pub fn report_logic_op_needs_bool_operands(
    cx: DisplayCx<'_>,
    err: UnifyError,
    operand: Ty,
    span: SrcSpan,
) {
    DiagCtx::emit(
        Diagnostic::error(cx.show(err).to_string(), span).with_label(format!(
            "`&&`/`||` need bool operands, found {}",
            cx.show(operand)
        )),
    );
}


pub fn report_unknown_literal_suffix(suffix: Symbol, span: SrcSpan) {
    DiagCtx::emit(
        Diagnostic::error(
            format!("invalid literal suffix `{}`", Interner::resolve(suffix)),
            span,
        )
        .with_label("not the name of a numeric type")
        .with_help(
            "a literal suffix must name a numeric type: `i8`, `i16`, `i32`, `i64`, `u8`, \
                 `u16`, `u32`, `u64`, `f32`, or `f64`",
        ),
    );
}

pub fn report_int_suffix_on_float_literal(suffix: Symbol, span: SrcSpan) {
    DiagCtx::emit(
        Diagnostic::error(
            format!(
                "literal has a fractional part but is suffixed `{}`",
                Interner::resolve(suffix)
            ),
            span,
        )
        .with_label(format!(
            "`{}` cannot hold a fractional value",
            Interner::resolve(suffix)
        ))
        .with_help("use a float suffix instead (`f32` or `f64`), or drop the fractional part"),
    );
}

pub fn report_binding_type_mismatch(cx: DisplayCx<'_>, err: UnifyError, span: SrcSpan) {
    DiagCtx::emit(
        Diagnostic::error(cx.show(err).to_string(), span)
            .with_label("the value this binding is given does not match its declared type"),
    );
}

pub fn report_body_return_mismatch(cx: DisplayCx<'_>, err: UnifyError, span: SrcSpan) {
    DiagCtx::emit(
        Diagnostic::error(cx.show(err).to_string(), span)
            .with_label("this function does not return its declared return type on every path"),
    );
}

pub fn report_any_outside_signature(cx: DisplayCx<'_>, ty: Ty, span: SrcSpan) {
    DiagCtx::emit(
        Diagnostic::error(
            format!("`any` may only appear in a parameter or return type, found `{}`", cx.show(ty)),
            span,
        )
        .with_label("this type carries `any` outside a function signature")
        .with_help(
            "`any` describes how a function accepts or hands back a value, not a type a \
                 field, binding, or generic argument can hold; give the field or binding a \
                 concrete type instead",
        ),
    );
}

pub fn report_reference_field(cx: DisplayCx<'_>, ty: Ty, span: SrcSpan) {
    DiagCtx::emit(
        Diagnostic::error(
            format!("a field cannot hold a reference, found `{}`", cx.show(ty)),
            span,
        )
        .with_label("this type stores a reference")
        .with_help(
            "a struct or enum field has to own the value it holds; take the field by value \
                 instead of by reference",
        ),
    );
}
