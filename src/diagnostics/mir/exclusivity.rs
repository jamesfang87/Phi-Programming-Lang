use crate::ast::Ident;
use crate::ast::interner::Interner;
use crate::diagnostics::{DiagCtx, Diagnostic};
use crate::driver::source::SrcSpan;

pub fn report_exclusivity_violation(name: Ident, span: SrcSpan) {
    DiagCtx::emit(
        Diagnostic::error(
            format!(
                "cannot use `{}` while it is borrowed",
                Interner::resolve(name.text)
            ),
            span,
        )
        .with_label("used here while a conflicting borrow is still alive"),
    );
}
