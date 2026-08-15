use crate::diag::{DiagCtx, Diagnostic};
use crate::langitems::LangItem;

/// Reports a lang item the core library doesn't declare.
pub fn report_missing(item: LangItem) {
    DiagCtx::emit(
        Diagnostic::error_global(format!("missing lang item `{}`", item.display_path())).with_help(
            "the core library must declare this item; it is embedded in the compiler, so this \
             is a compiler bug rather than a problem with the program being compiled",
        ),
    );
}
