//! Diagnostics for `typeck`, mirroring its module tree file-for-file: a `report_*` here for
//! every mistake the type checker itself can find, plus (in [`display`]) the machinery that
//! renders a `Ty`/`UnifyError` as the user-facing text those reports quote.

pub mod display;
pub mod expr;
pub mod lower_ty;
pub mod pat;
pub mod traits;

use crate::diag::{DiagCtx, Diagnostic};
use crate::diagnostics::typeck::display::DisplayCx;
use crate::driver::source::SrcSpan;
use crate::typeck::ty::Ty;
use crate::typeck::unify::UnifyError;

/// Reports why a `return`'s expression didn't unify with the enclosing function's return
/// type, at the `return` statement's span.
pub fn report_return_mismatch(cx: DisplayCx<'_>, err: UnifyError, span: SrcSpan) {
    DiagCtx::emit(
        Diagnostic::error(cx.show(err).to_string(), span).with_label(
            "returned value does not match this \
                function's return type",
        ),
    );
}

/// Reported by [`Typeck::operator_holds`](crate::typeck::Typeck::operator_holds) when an
/// operator's operand is still a wholly unresolved variable -- not merely
/// unconstrained-but-numeric, which
/// [`Typeck::is_builtin_operand`](crate::typeck::Typeck::is_builtin_operand) already lets through
/// without reaching here.
pub fn report_operand_unknown(span: SrcSpan) {
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

/// A binary operator's two operands don't unify with each other, at the whole binary expression's
/// span.
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

/// `&&`/`||`'s operand didn't unify with `bool`. Not overloadable -- the core lib has no logic
/// trait, so these only ever mean the primitive short-circuit operators.
pub fn report_logic_op_needs_bool(cx: DisplayCx<'_>, err: UnifyError, operand: Ty, span: SrcSpan) {
    DiagCtx::emit(
        Diagnostic::error(cx.show(err).to_string(), span).with_label(format!(
            "`&&`/`||` need bool operands, found {}",
            cx.show(operand)
        )),
    );
}

/// A string literal is a value of some string type, and there is nothing here for it to be: the
/// core library declares no `String`, and `LangItem` names none, so there is no definition to
/// resolve one to. Reported rather than given a stand-in type, which would make every use of it
/// check against something no later pass could lower.
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

/// A `let`'s (or a `with` lend's) initializer didn't unify with its declared type.
pub fn report_binding_type_mismatch(cx: DisplayCx<'_>, err: UnifyError, span: SrcSpan) {
    DiagCtx::emit(
        Diagnostic::error(cx.show(err).to_string(), span)
            .with_label("the value this binding is given does not match its declared type"),
    );
}

/// A function's body doesn't unify with its declared return type on every path.
pub fn report_body_return_mismatch(cx: DisplayCx<'_>, err: UnifyError, span: SrcSpan) {
    DiagCtx::emit(
        Diagnostic::error(cx.show(err).to_string(), span)
            .with_label("this function does not return its declared return type on every path"),
    );
}
