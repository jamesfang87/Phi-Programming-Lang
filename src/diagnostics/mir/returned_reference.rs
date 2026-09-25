use crate::ast::Ident;
use crate::diagnostics::Diagnostic;
use crate::diagnostics::codes;
use crate::driver::source::SrcSpan;
use crate::session::Session;

pub fn report_returned_local_reference(session: &Session, name: Option<Ident>, span: SrcSpan) {
    let subject = match name {
        Some(name) => format!("`{}`", session.resolve(name.text)),
        None => "a local variable".to_string(),
    };
    session.emit(
        Diagnostic::error(format!("cannot return a reference to {subject}"), span)
            .with_code(codes::RETURNED_LOCAL_REFERENCE)
            .with_label("this reference would outlive the storage it points to")
            .with_help(
                "a returned reference must be a projection of a parameter; return the value \
                 itself instead of a reference to it",
            ),
    );
}
