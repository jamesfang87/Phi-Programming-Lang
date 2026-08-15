pub mod display;
pub mod expr;
pub mod lower_ty;
pub mod pat;
pub mod traits;

use crate::ast::interner::{Interner, Symbol};
use crate::diag::{DiagCtx, Diagnostic};
use crate::diagnostics::typeck::display::DisplayCx;
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

// TODO: Remove when std library String is programmed
pub fn report_str_literal_untyped(span: SrcSpan) {
    DiagCtx::emit(
        Diagnostic::error("a string literal has no type yet", span)
            .with_label("`str` is not a type the core library declares")
            .with_help(
                "the core library declares no string type and no lang item names one, \
                             so there is nothing for this literal to be",
            ),
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
