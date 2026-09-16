use crate::ast::Ident;
use crate::diagnostics::Diagnostic;
use crate::diagnostics::codes;
use crate::driver::source::SrcSpan;
use crate::session::Session;

pub fn report_use_of_moved_value(session: &Session, name: Ident, span: SrcSpan) {
    session.emit(
        Diagnostic::error(
            format!("use of moved value `{}`", session.resolve(name.text)),
            span,
        )
        .with_code(codes::USE_OF_MOVED_VALUE)
        .with_label("value used here after being moved"),
    );
}

pub fn report_move_out_of_array(session: &Session, name: Ident, span: SrcSpan) {
    session.emit(
        Diagnostic::error(
            format!(
                "cannot move a value out of `{}`, which is an array",
                session.resolve(name.text)
            ),
            span,
        )
        .with_code(codes::MOVE_OUT_OF_ARRAY)
        .with_label("this element would be moved out, leaving a hole in the array")
        .with_help(
            "an array is dropped as a whole, so it has nowhere to record that one element is \
             gone; borrow the element instead, or move the whole array",
        ),
    );
}
