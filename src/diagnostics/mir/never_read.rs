use crate::ast::Ident;
use crate::diagnostics::Diagnostic;
use crate::diagnostics::codes;
use crate::driver::source::SrcSpan;
use crate::session::Session;

pub fn report_value_never_read(session: &Session, name: Ident, span: SrcSpan) {
    session.emit(
        Diagnostic::warning(
            format!(
                "value assigned to `{}` is never read",
                session.resolve(name.text)
            ),
            span,
        )
        .with_code(codes::NEVER_READ)
        .with_label("this value is never read before it goes out of scope"),
    );
}

pub fn report_parameter_never_read(session: &Session, name: Ident, span: SrcSpan) {
    session.emit(
        Diagnostic::warning(
            format!("parameter `{}` is never read", session.resolve(name.text)),
            span,
        )
        .with_code(codes::NEVER_READ)
        .with_label("this parameter is never read in the function body"),
    );
}
