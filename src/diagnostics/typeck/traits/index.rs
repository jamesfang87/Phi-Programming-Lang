use crate::diagnostics::{DiagCtx, Diagnostic};
use crate::driver::source::SrcSpan;

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

pub fn report_extend_any(span: SrcSpan) {
    DiagCtx::emit(
        Diagnostic::error("`any` cannot be extended", span)
            .with_label("not a type in its own right")
            .with_help(
                "`any` describes how a value crosses a function boundary, not a type of its \
                     own; extend the type underneath it instead",
            ),
    );
}

pub fn report_extend_dyn(span: SrcSpan) {
    DiagCtx::emit(
        Diagnostic::error("`dyn Trait` cannot be extended", span)
            .with_label("already satisfies its own trait")
            .with_help(
                "a `dyn Trait` value already satisfies `Trait` by construction; extend the \
                     concrete type stored behind it instead",
            ),
    );
}

pub fn report_extend_unsized(span: SrcSpan) {
    DiagCtx::emit(
        Diagnostic::error("an unsized array cannot be extended", span)
            .with_label("has no fixed size")
            .with_help(
                "`[T]` has no fixed size, so there is nowhere to store a value of it to extend; \
                     write a fixed length, as in `[T; N]`",
            ),
    );
}

pub fn report_extend_bare_self(span: SrcSpan) {
    DiagCtx::emit(
        Diagnostic::error("`Self` cannot be extended", span)
            .with_label("names whatever this block already extends")
            .with_help("an `extend` block has to name a concrete type, not `Self`"),
    );
}

pub fn report_attempt_to_extend_with_non_trait(span: SrcSpan) {
    DiagCtx::emit(
        Diagnostic::error("`with` must name a trait", span)
            .with_label("not a trait")
            .with_help("only a trait declares methods for an `extend` block to implement"),
    );
}
