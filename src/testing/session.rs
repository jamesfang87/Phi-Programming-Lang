use std::cell::OnceCell;

use crate::ast::Symbol;
use crate::diagnostics::Diagnostic;
use crate::driver::source::{FileOrigin, SrcFile, SrcSpan};
use crate::session::Session;

thread_local! {
    /// The [`Session`] every test helper on this thread shares.
    ///
    /// Each test thread gets its own, so tests stay isolated without a process-wide singleton.
    /// The session is leaked because test helpers hand out references to it (`Session` is not
    /// `Copy`), and a test thread's session lives as long as the thread anyway.
    static SESSION: OnceCell<&'static Session> = const { OnceCell::new() };
}

/// The session every test helper on the current thread shares.
pub fn session() -> &'static Session {
    SESSION.with(|cell| *cell.get_or_init(|| Box::leak(Box::new(Session::new()))))
}

pub fn intern(text: &str) -> Symbol {
    session().intern(text)
}

pub fn resolve(symbol: Symbol) -> &'static str {
    session().resolve(symbol)
}

pub fn clear_interner() {}

pub fn clear_diagnostics() {
    session().clear_diagnostics();
}

pub fn diagnostics() -> Vec<Diagnostic> {
    session().diagnostics()
}

pub fn messages() -> Vec<String> {
    session().messages()
}

pub fn add_file(name: String, content: Vec<char>, origin: FileOrigin) -> usize {
    session().add_file(name, content, origin)
}

pub fn text_of(span: SrcSpan) -> Option<String> {
    session().text_of(span)
}

pub fn collect_core() -> Vec<&'static SrcFile> {
    session().collect_core()
}
