use crate::diagnostics::typeck::display::DisplayCx;
use crate::diagnostics::{DiagCtx, Diagnostic};
use crate::driver::source::SrcSpan;
use crate::typeck::ty::Ty;
use crate::typeck::unify::UnifyError;

pub fn report_unexpected_generic_args(kind: &str, span: SrcSpan) {
    DiagCtx::emit(
        Diagnostic::error(format!("{kind} takes no generic arguments"), span)
            .with_label("unexpected generic arguments"),
    );
}

pub fn report_arg_count(span: SrcSpan, declared: usize, found: usize) {
    let plural = if declared == 1 { "" } else { "s" };
    DiagCtx::emit(
        Diagnostic::error(
            format!(
                "this type takes {declared} generic argument{plural} but {found} \
                     {} supplied",
                if found == 1 { "was" } else { "were" }
            ),
            span,
        )
        .with_label(format!("expected {declared} argument{plural}")),
    );
}

pub fn report_trait_as_ty(span: SrcSpan) {
    DiagCtx::emit(
        Diagnostic::error("a trait cannot be used as a type on its own", span)
            .with_label("not a type")
            .with_help(
                "a trait names every type that implements it, not one type; write \
                     `dyn Trait` for a value whose type is only known at run time, or take a \
                     generic parameter bounded by the trait",
            ),
    );
}

pub fn report_dyn_not_a_trait(span: SrcSpan) {
    DiagCtx::emit(
        Diagnostic::error("`dyn` must be applied to a trait", span)
            .with_label("not a trait")
            .with_help("only a trait describes a set of types that a `dyn` value can hold"),
    );
}

pub fn report_self_outside_item(span: SrcSpan) {
    DiagCtx::emit(
        Diagnostic::error("`Self` is not available here", span)
            .with_label("no enclosing type")
            .with_help(
                "`Self` names the type being defined, so it only means something inside a \
                     `struct`, `enum`, `trait`, or `extend` body",
            ),
    );
}

pub fn report_unsized_dyn(cx: DisplayCx<'_>, ty: Ty, span: SrcSpan) {
    DiagCtx::emit(
        Diagnostic::error(
            format!(
                "`dyn` has no size known at compile time, found `{}`",
                cx.show(ty)
            ),
            span,
        )
        .with_label("this type is not sized")
        .with_help(
            "a `dyn` value can only be used through a fixed-size indirection; write \
                 `&dyn Trait` (or `&mut dyn Trait`) to borrow it, or `iso dyn Trait` to own it \
                 behind a pointer",
        ),
    );
}

pub fn report_array_len_not_usize(cx: DisplayCx<'_>, err: UnifyError, span: SrcSpan) {
    DiagCtx::emit(
        Diagnostic::error(cx.show(err).to_string(), span)
            .with_label("the length in `[T; N]` is a `usize`"),
    );
}

pub fn report_array_len_not_constant(span: SrcSpan) {
    DiagCtx::emit(
        Diagnostic::error("array length must be a constant", span)
            .with_label("not a constant")
            .with_help(
                "the length in `[T; N]` is part of the type, so it has to be known at \
                 compile time; write an integer literal or an expression built from them",
            ),
    );
}

pub fn report_array_len_negative(span: SrcSpan) {
    DiagCtx::emit(
        Diagnostic::error("array length cannot be negative", span).with_label("negative length"),
    );
}

pub fn report_array_len_overflow(span: SrcSpan) {
    DiagCtx::emit(
        Diagnostic::error("array length overflowed while being evaluated", span)
            .with_label("does not fit in a `usize`"),
    );
}

pub fn report_array_len_division_by_zero(span: SrcSpan) {
    DiagCtx::emit(
        Diagnostic::error("array length divides by zero", span).with_label("divisor is zero"),
    );
}

pub fn report_reference_generic_arg(cx: DisplayCx<'_>, ty: Ty, span: SrcSpan) {
    DiagCtx::emit(
        Diagnostic::error(
            format!(
                "a struct or enum cannot be instantiated with a reference, found `{}`",
                cx.show(ty)
            ),
            span,
        )
        .with_label("this generic argument stores a reference")
        .with_help(
            "a struct or enum field has to own the value it holds, so none of its generic \
                 arguments may store a reference either",
        ),
    );
}

pub fn report_self_cycle(span: SrcSpan) {
    DiagCtx::emit(
        Diagnostic::error("`Self` is defined in terms of itself", span)
            .with_label("cycle here")
            .with_help(
                "the type this `Self` stands for cannot be worked out without already \
                     knowing it",
            ),
    );
}
