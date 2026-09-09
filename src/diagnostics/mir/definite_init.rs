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

pub fn report_move_out_of_array(name: Ident, span: SrcSpan) {
    DiagCtx::emit(
        Diagnostic::error(
            format!(
                "cannot move a value out of `{}`, which is an array",
                Interner::resolve(name.text)
            ),
            span,
        )
        .with_label("this element would be moved out, leaving a hole in the array")
        .with_help(
            "an array is dropped as a whole, so it has nowhere to record that one element is \
             gone; borrow the element instead, or move the whole array",
        ),
    );
}
