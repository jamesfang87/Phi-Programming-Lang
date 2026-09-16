use crate::ast::Ident;
use crate::diagnostics::Diagnostic;
use crate::diagnostics::codes;
use crate::driver::source::SrcSpan;
use crate::session::Session;

pub fn report_not_mutable(session: &Session, name: Ident, span: SrcSpan) {
    session.emit(
        Diagnostic::error(
            format!(
                "cannot assign to `{}`, which is not declared `mut`",
                session.resolve(name.text)
            ),
            span,
        )
        .with_code(codes::NOT_MUTABLE)
        .with_label("not mutable")
        .with_help(format!(
            "declare it `let mut {}` to allow this",
            session.resolve(name.text)
        )),
    );
}
