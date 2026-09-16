use crate::ast::Ident;
use crate::diagnostics::Diagnostic;
use crate::diagnostics::codes;
use crate::driver::source::SrcSpan;
use crate::session::Session;

pub fn report_exclusivity_violation(session: &Session, name: Ident, span: SrcSpan) {
    session.emit(
        Diagnostic::error(
            format!(
                "cannot use `{}` while it is borrowed",
                session.resolve(name.text)
            ),
            span,
        )
        .with_code(codes::EXCLUSIVITY_VIOLATION)
        .with_label("used here while a conflicting borrow is still alive"),
    );
}
