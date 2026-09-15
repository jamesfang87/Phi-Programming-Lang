use crate::diagnostics::Diagnostic;
use crate::diagnostics::codes;
use crate::diagnostics::display::DisplayCtx;
use crate::driver::source::SrcSpan;
use crate::session::Session;
use crate::typeck::ty::Ty;
use crate::typeck::unify::UnifyError;

pub fn report_unexpected_generic_args(session: &Session, kind: &str, span: SrcSpan) {
    session.emit(
        Diagnostic::error(format!("{kind} takes no generic arguments"), span)
            .with_code(codes::UNEXPECTED_GENERIC_ARGS)
            .with_label("unexpected generic arguments"),
    );
}

pub fn report_arg_count(session: &Session, span: SrcSpan, declared: usize, found: usize) {
    let plural = if declared == 1 { "" } else { "s" };
    session.emit(
        Diagnostic::error(
            format!(
                "this type takes {declared} generic argument{plural} but {found} \
                     {} supplied",
                if found == 1 { "was" } else { "were" }
            ),
            span,
        )
        .with_code(codes::GENERIC_ARG_COUNT)
        .with_label(format!("expected {declared} argument{plural}")),
    );
}

pub fn report_trait_as_ty(session: &Session, span: SrcSpan) {
    session.emit(
        Diagnostic::error("a trait cannot be used as a type on its own", span)
            .with_code(codes::TRAIT_AS_TYPE)
            .with_label("not a type")
            .with_help(
                "a trait names every type that implements it, not one type; write \
                     `dyn Trait` for a value whose type is only known at run time, or take a \
                     generic parameter bounded by the trait",
            ),
    );
}

pub fn report_unsized_dyn(cx: DisplayCtx<'_>, ty: Ty, span: SrcSpan) {
    cx.emit(
        Diagnostic::error(
            format!(
                "`dyn` has no size known at compile time, found `{}`",
                cx.show(ty)
            ),
            span,
        )
        .with_code(codes::UNSIZED_DYN)
        .with_label("this type is not sized")
        .with_help(
            "a `dyn` value can only be used through a fixed-size indirection; write \
                 `&dyn Trait` (or `&mut dyn Trait`) to borrow it, or `iso dyn Trait` to own it \
                 behind a pointer",
        ),
    );
}

pub fn report_array_len_not_usize(cx: DisplayCtx<'_>, err: UnifyError, span: SrcSpan) {
    cx.emit_unify(err, span, "the length in `[T; N]` is a `usize`");
}

pub fn report_array_len_not_constant(session: &Session, span: SrcSpan) {
    session.emit(
        Diagnostic::error("array length must be a constant", span)
            .with_code(codes::ARRAY_LEN_NOT_CONSTANT)
            .with_label("not a constant")
            .with_help(
                "the length in `[T; N]` is part of the type, so it has to be known at \
                 compile time; write an integer literal or an expression built from them",
            ),
    );
}

pub fn report_array_len_negative(session: &Session, span: SrcSpan) {
    session.emit(
        Diagnostic::error("array length cannot be negative", span)
            .with_code(codes::ARRAY_LEN_NEGATIVE)
            .with_label("negative length"),
    );
}

pub fn report_array_len_overflow(session: &Session, span: SrcSpan) {
    session.emit(
        Diagnostic::error("array length overflowed while being evaluated", span)
            .with_code(codes::ARRAY_LEN_OVERFLOW)
            .with_label("does not fit in a `usize`"),
    );
}

pub fn report_array_len_division_by_zero(session: &Session, span: SrcSpan) {
    session.emit(
        Diagnostic::error("array length divides by zero", span)
            .with_code(codes::ARRAY_LEN_DIVISION_BY_ZERO)
            .with_label("divisor is zero"),
    );
}

pub fn report_reference_generic_arg(cx: DisplayCtx<'_>, ty: Ty, span: SrcSpan) {
    cx.emit(
        Diagnostic::error(
            format!(
                "a struct or enum cannot be instantiated with a reference, found `{}`",
                cx.show(ty)
            ),
            span,
        )
        .with_code(codes::REFERENCE_GENERIC_ARG)
        .with_label("this generic argument stores a reference")
        .with_help(
            "a struct or enum field has to own the value it holds, so none of its generic \
                 arguments may store a reference either",
        ),
    );
}

pub fn report_self_cycle(session: &Session, span: SrcSpan) {
    session.emit(
        Diagnostic::error("`Self` is defined in terms of itself", span)
            .with_code(codes::SELF_CYCLE)
            .with_label("cycle here")
            .with_help(
                "the type this `Self` stands for cannot be worked out without already \
                     knowing it",
            ),
    );
}
