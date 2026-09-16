use crate::diagnostics::Diagnostic;
use crate::diagnostics::codes;
use crate::driver::source::SrcSpan;
use crate::hir::{DefId, Hir};
use crate::session::Session;

/// A warning, not an error: a crate with no entry point still compiles and links. The binary is
/// simply one that does nothing, which is what a caller building a crate for its definitions
/// alone wants.
pub fn report_missing_main(session: &Session) {
    session.emit(
        Diagnostic::warning_global(
            "no `main` function found; the built executable will do nothing",
        )
        .with_code(codes::MISSING_MAIN)
        .with_help("add a `fun main()` at the crate root to give the program an entry point"),
    );
}

pub fn report_ambiguous_main(session: &Session, hir: &Hir, candidates: &[DefId]) {
    let mut diagnostic = Diagnostic::error(
        format!(
            "found {} `main` functions at the crate root; the entry point is ambiguous",
            candidates.len()
        ),
        hir.function(candidates[0]).name.span,
    )
    .with_code(codes::AMBIGUOUS_MAIN)
    .with_label("the entry point would be this `main`");
    for &other in &candidates[1..] {
        diagnostic = diagnostic.with_secondary(hir.function(other).name.span, "also named `main`");
    }
    session.emit(diagnostic.with_help("keep one `main` at the crate root and rename the others"));
}

pub fn report_main_takes_parameters(session: &Session, span: SrcSpan, param_count: usize) {
    session.emit(
        Diagnostic::error("the `main` function cannot take parameters", span)
            .with_code(codes::MAIN_TAKES_PARAMETERS)
            .with_label(format!("this `main` takes {param_count} parameter(s)"))
            .with_help("the entry point is called with no arguments; remove main's parameters"),
    );
}

pub fn report_main_returns_a_value(session: &Session, span: SrcSpan) {
    session.emit(
        Diagnostic::error("the `main` function cannot return a value", span)
            .with_code(codes::MAIN_RETURNS_A_VALUE)
            .with_label("this declared return type must be removed")
            .with_help("main's return type is `()`; the entry point's return value is discarded"),
    );
}

pub fn report_main_is_generic(session: &Session, span: SrcSpan) {
    session.emit(
        Diagnostic::error("the `main` function cannot be generic", span)
            .with_code(codes::MAIN_IS_GENERIC)
            .with_label("this `main` has generic parameters")
            .with_help("the entry point must be a single concrete function; remove main's generic parameters"),
    );
}
