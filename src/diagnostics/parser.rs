use chumsky::error::Rich;

use crate::diagnostics::{DiagCtx, Diagnostic};
use crate::driver::source::SrcSpan;
use crate::lexer::token::Token;

pub fn report_duplicate_module(span: SrcSpan) {
    DiagCtx::emit(
        Diagnostic::error("a file can only declare one module", span)
            .with_label("second `module` declaration")
            .with_help("every item in a file belongs to the module its first header names"),
    );
}

pub fn report_error(err: &Rich<Token>) {
    let span = err
        .found()
        .map(|t| t.span)
        .unwrap_or_else(|| SrcSpan::new(0, 0));
    DiagCtx::error(err.to_string(), span);
}
