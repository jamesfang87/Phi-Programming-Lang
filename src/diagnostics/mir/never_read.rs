use crate::ast::Ident;
use crate::ast::interner::Interner;
use crate::diagnostics::{DiagCtx, Diagnostic};
use crate::driver::source::SrcSpan;

pub fn report_value_never_read(name: Ident, span: SrcSpan) {
    DiagCtx::emit(
        Diagnostic::warning(
            format!(
                "value assigned to `{}` is never read",
                Interner::resolve(name.text)
            ),
            span,
        )
        .with_label("this value is never read before it goes out of scope"),
    );
}
