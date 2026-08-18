use crate::diagnostics::{DiagCtx, Diagnostic};
use crate::driver::source::SrcSpan;

pub fn report_extend_primitive(span: SrcSpan) {
    DiagCtx::emit(
        Diagnostic::error("a primitive type cannot be extended", span)
            .with_label("not a struct or enum")
            .with_help(
                "only a `struct` or an `enum` can be extended, so that the type being \
                     implemented has a definition to attach the implementation to",
            ),
    );
}

pub fn report_extend_trait(span: SrcSpan) {
    DiagCtx::emit(
        Diagnostic::error("a trait cannot be extended", span)
            .with_label("not a struct or enum")
            .with_help(
                "a trait names every type that implements it, not one type; extend the \
                     `struct` or `enum` that should implement it instead",
            ),
    );
}

pub fn report_extend_generic(span: SrcSpan) {
    DiagCtx::emit(
        Diagnostic::error("a generic type parameter cannot be extended", span)
            .with_label("not a struct or enum")
            .with_help(
                "an implementation has to name the type it applies to; extending a parameter \
                     would implement the trait for every type at once",
            ),
    );
}

pub fn report_attempt_to_extend_with_non_trait(span: SrcSpan) {
    DiagCtx::emit(
        Diagnostic::error("`with` must name a trait", span)
            .with_label("not a trait")
            .with_help("only a trait declares methods for an `extend` block to implement"),
    );
}
