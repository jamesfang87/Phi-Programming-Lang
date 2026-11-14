use crate::diagnostics::Diagnostic;
use crate::diagnostics::codes;
use crate::diagnostics::display::DisplayCtx;
use crate::driver::source::SrcSpan;
use crate::session::Session;

pub fn report_captured_reference(cx: DisplayCtx<'_>, ty: crate::typeck::ty::Ty, span: SrcSpan) {
    cx.emit(
        Diagnostic::error(
            format!(
                "a closure cannot capture a reference, found `{}`",
                cx.show(ty)
            ),
            span,
        )
        .with_code(codes::CAPTURED_REFERENCE)
        .with_label("this capture stores a reference"),
    );
}

pub fn report_move_out_of_environment(session: &Session, span: SrcSpan) {
    session.emit(
        Diagnostic::error(
            "cannot move a captured value out of a closure's environment",
            span,
        )
        .with_code(codes::MOVE_OUT_OF_ENVIRONMENT),
    );
}
