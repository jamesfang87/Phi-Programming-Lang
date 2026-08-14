use crate::diag::{DiagCtx, Diagnostic};
use crate::langitems::LangItem;

/// Reports a lang item the core library doesn't declare.
///
/// This names no span. There is no source location that could be to blame: the core library is
/// embedded in the compiler binary, and the path being looked up is the compiler's own, so
/// nothing the user wrote is at fault. See [`Diagnostic::error_global`].
pub fn report_missing(item: LangItem) {
    DiagCtx::emit(
        Diagnostic::error_global(format!("missing lang item `{}`", item.display_path())).with_help(
            "the core library must declare this item; it is embedded in the compiler, so this \
             is a compiler bug rather than a problem with the program being compiled",
        ),
    );
}
