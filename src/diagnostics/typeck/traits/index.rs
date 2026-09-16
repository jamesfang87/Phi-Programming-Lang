use crate::diagnostics::Diagnostic;
use crate::diagnostics::codes;
use crate::driver::source::SrcSpan;
use crate::session::Session;

pub fn report_extend_trait(session: &Session, span: SrcSpan) {
    session.emit(
        Diagnostic::error("a trait cannot be extended", span)
            .with_code(codes::EXTEND_TRAIT)
            .with_label("not a struct or enum")
            .with_help(
                "a trait names every type that implements it, not one type; extend the \
                     `struct` or `enum` that should implement it instead",
            ),
    );
}

pub fn report_extend_generic(session: &Session, span: SrcSpan) {
    session.emit(
        Diagnostic::error("a generic type parameter cannot be extended", span)
            .with_code(codes::EXTEND_GENERIC)
            .with_label("not a struct or enum")
            .with_help(
                "an implementation has to name the type it applies to; extending a parameter \
                     would implement the trait for every type at once",
            ),
    );
}

pub fn report_extend_any(session: &Session, span: SrcSpan) {
    session.emit(
        Diagnostic::error("`any` cannot be extended", span)
            .with_code(codes::EXTEND_ANY)
            .with_label("not a type in its own right")
            .with_help(
                "`any` describes how a value crosses a function boundary, not a type of its \
                     own; extend the type underneath it instead",
            ),
    );
}

pub fn report_extend_dyn(session: &Session, span: SrcSpan) {
    session.emit(
        Diagnostic::error("`dyn Trait` cannot be extended", span)
            .with_code(codes::EXTEND_DYN)
            .with_label("already satisfies its own trait")
            .with_help(
                "a `dyn Trait` value already satisfies `Trait` by construction; extend the \
                     concrete type stored behind it instead",
            ),
    );
}

pub fn report_extend_unsized(session: &Session, span: SrcSpan) {
    session.emit(
        Diagnostic::error("an unsized array cannot be extended", span)
            .with_code(codes::EXTEND_UNSIZED)
            .with_label("has no fixed size")
            .with_help(
                "`[T]` has no fixed size, so there is nowhere to store a value of it to extend; \
                     write a fixed length, as in `[T; N]`",
            ),
    );
}

pub fn report_extend_bare_self(session: &Session, span: SrcSpan) {
    session.emit(
        Diagnostic::error("`Self` cannot be extended", span)
            .with_code(codes::EXTEND_BARE_SELF)
            .with_label("names whatever this block already extends")
            .with_help("an `extend` block has to name a concrete type, not `Self`"),
    );
}

pub fn report_attempt_to_extend_with_non_trait(session: &Session, span: SrcSpan) {
    session.emit(
        Diagnostic::error("`with` must name a trait", span)
            .with_code(codes::EXTEND_WITH_NON_TRAIT)
            .with_label("not a trait")
            .with_help("only a trait declares methods for an `extend` block to implement"),
    );
}
