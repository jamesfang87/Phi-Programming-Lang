use crate::diagnostics::{DiagCtx, Diagnostic};
use crate::driver::source::SrcSpan;

pub fn report_not_all_paths_return(span: SrcSpan) {
    DiagCtx::emit(
        Diagnostic::error("not all control flow paths return a value", span)
            .with_label("this function does not always return"),
    );
}
