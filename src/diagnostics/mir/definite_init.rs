use crate::ast::Ident;
use crate::ast::interner::Interner;
use crate::diagnostics::{DiagCtx, Diagnostic};
use crate::driver::source::SrcSpan;

pub fn report_use_of_moved_value(name: Ident, span: SrcSpan) {
    DiagCtx::emit(
        Diagnostic::error(
            format!("use of moved value `{}`", Interner::resolve(name.text)),
            span,
        )
        .with_label("value used here after being moved"),
    );
}
