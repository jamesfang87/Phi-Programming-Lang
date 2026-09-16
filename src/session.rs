use std::cell::RefCell;
use std::io;
use std::path::Path;

use crate::ast::interner::{Interner, Symbol};
use crate::diagnostics::{Diagnostic, Diagnostics};
use crate::driver::source::{FileOrigin, SrcFile, SrcMap, SrcSpan};

pub struct Session {
    sources: RefCell<SrcMap>,
    interner: RefCell<Interner>,
    diagnostics: RefCell<Diagnostics>,
}

impl Default for Session {
    fn default() -> Self {
        Session::new()
    }
}

impl Session {
    pub fn new() -> Self {
        Session {
            sources: RefCell::new(SrcMap::new()),
            interner: RefCell::new(Interner::new()),
            diagnostics: RefCell::new(Diagnostics::new()),
        }
    }

    // ---------------------------------------------------------------
    // Source map
    // ---------------------------------------------------------------

    /// Registers a new source file, returning the global offset its content starts at.
    pub fn add_file(&self, name: String, content: Vec<char>, origin: FileOrigin) -> usize {
        self.sources.borrow_mut().add_file(name, content, origin)
    }

    /// Returns every registered file, in the order it was added.
    pub fn files(&self) -> Vec<&'static SrcFile> {
        self.sources.borrow().files()
    }

    /// Returns the file whose global offset range contains `offset`.
    pub fn file_containing(&self, offset: usize) -> Option<&'static SrcFile> {
        self.sources.borrow().file_containing(offset)
    }

    /// Returns the source text covered by `span` as an owned `String`.
    pub fn text_of(&self, span: SrcSpan) -> Option<String> {
        self.sources.borrow().text_of(span)
    }

    /// Returns the chars covered by `span`.
    pub fn chars_of(&self, span: SrcSpan) -> Option<&'static [char]> {
        self.sources.borrow().chars_of(span)
    }

    /// Recursively finds all `.phi` files under `root` and registers them.
    pub fn collect(&self, root: &Path) -> io::Result<()> {
        crate::driver::source::collect(&mut self.sources.borrow_mut(), root)
    }

    /// Registers every core library file, returning exactly the files this call registered.
    pub fn collect_core(&self) -> Vec<&'static SrcFile> {
        crate::driver::source::collect_core(&mut self.sources.borrow_mut())
    }

    /// Registers every standard library file, returning exactly the files this call registered.
    pub fn collect_std(&self) -> Vec<&'static SrcFile> {
        crate::driver::source::collect_std(&mut self.sources.borrow_mut())
    }

    // ---------------------------------------------------------------
    // Interner
    // ---------------------------------------------------------------

    /// Interns `text`, returning its [`Symbol`] handle.
    pub fn intern(&self, text: &str) -> Symbol {
        self.interner.borrow_mut().intern(text)
    }

    /// Resolves `symbol` back to the text it was interned from.
    pub fn resolve(&self, symbol: Symbol) -> &'static str {
        self.interner.borrow().resolve(symbol)
    }

    // ---------------------------------------------------------------
    // Diagnostics
    // ---------------------------------------------------------------

    /// Records `diagnostic`. It isn't rendered until [`Session::report`] is called.
    pub fn emit(&self, diagnostic: Diagnostic) {
        self.diagnostics.borrow_mut().emit(diagnostic);
    }

    /// Records an error-severity diagnostic.
    pub fn error(&self, message: impl Into<String>, span: SrcSpan) {
        self.diagnostics.borrow_mut().error(message, span);
    }

    /// Records a warning-severity diagnostic.
    pub fn warning(&self, message: impl Into<String>, span: SrcSpan) {
        self.diagnostics.borrow_mut().warning(message, span);
    }

    /// Returns every diagnostic recorded so far, in the order it was recorded.
    pub fn diagnostics(&self) -> Vec<Diagnostic> {
        self.diagnostics.borrow().diagnostics()
    }

    /// Returns just the message text of every diagnostic recorded so far.
    pub fn messages(&self) -> Vec<String> {
        self.diagnostics.borrow().messages()
    }

    /// Discards every diagnostic collected so far.
    pub fn clear_diagnostics(&self) {
        self.diagnostics.borrow_mut().clear();
    }

    /// Renders every diagnostic collected so far to stderr in source order, takes them out of the
    /// collection, and returns whether any of them was error-severity.
    pub fn report(&self) -> bool {
        self.diagnostics.borrow_mut().report(&self.sources.borrow())
    }
}
