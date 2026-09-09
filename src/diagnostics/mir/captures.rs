use crate::diagnostics::typeck::display::DisplayCx;
use crate::diagnostics::{DiagCtx, Diagnostic};
use crate::driver::source::SrcSpan;

pub fn report_captured_reference(cx: DisplayCx<'_>, ty: crate::typeck::ty::Ty, span: SrcSpan) {
    DiagCtx::emit(
        Diagnostic::error(
            format!(
                "a closure cannot capture a reference, found `{}`",
                cx.show(ty)
            ),
            span,
        )
        .with_label("this capture stores a reference")
        .with_help(
            "a closure value can outlive the expression that built it -- it can be returned, \
             stored, or passed on -- so what it captures has to be owned, the same way a struct \
             field does; capture the value itself, or take the reference as a parameter instead",
        ),
    );
}

pub fn report_move_out_of_environment(span: SrcSpan) {
    DiagCtx::emit(
        Diagnostic::error(
            "cannot move a captured value out of a closure's environment",
            span,
        )
        .with_label("this would take the value out of the closure that owns it")
        .with_help(
            "a closure's captures belong to the closure value, which stays callable afterwards \
             and releases them when it is dropped; read the capture instead, or move the value \
             into the closure's own body by other means",
        ),
    );
}
