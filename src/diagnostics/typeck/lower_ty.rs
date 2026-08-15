use crate::diag::{DiagCtx, Diagnostic};
use crate::driver::source::SrcSpan;

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
