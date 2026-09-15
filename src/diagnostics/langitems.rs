use crate::diagnostics::Diagnostic;
use crate::diagnostics::codes;
use crate::langitems::LangItem;
use crate::session::Session;

/// Reports a lang item the core library doesn't declare.
pub fn report_missing(session: &Session, item: LangItem) {
    session.emit(
        Diagnostic::error_global(format!("missing lang item `{}`", item.display_path()))
            .with_code(codes::MISSING_LANG_ITEM)
            .with_help(
            "the core library must declare this item; it is embedded in the compiler, so this \
             is a compiler bug rather than a problem with the program being compiled",
        ),
    );
}
