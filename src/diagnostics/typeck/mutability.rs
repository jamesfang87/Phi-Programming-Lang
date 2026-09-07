use crate::ast::Ident;
use crate::ast::interner::Interner;
use crate::diagnostics::{DiagCtx, Diagnostic};
use crate::driver::source::SrcSpan;

pub fn report_not_mutable(name: Ident, span: SrcSpan) {
    DiagCtx::emit(
        Diagnostic::error(
            format!(
                "cannot assign to `{}`, which is not declared `mut`",
                Interner::resolve(name.text)
            ),
            span,
        )
        .with_label("not mutable")
        .with_help(format!(
            "declare it `let mut {}` to allow this",
            Interner::resolve(name.text)
        )),
    );
}
