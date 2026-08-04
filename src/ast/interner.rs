//! A trivial string interner for [`crate::ast::Symbol`].
//!
//! The interner is thread-local, mirroring [`crate::diag::DiagCtx`]. Pipeline
//! stages call its associated functions directly, rather than threading an interner
//! instance through every parser and lexer constructor.

use std::cell::RefCell;
use std::collections::HashMap;

use crate::ast::Symbol;

/// Interned text is leaked to `'static` rather than owned by this table, so that
/// [`Interner::resolve`] can hand back a borrow instead of a clone.
///
/// Leaking is what makes that sound in the presence of [`Interner::clear`]: resetting the table
/// drops the *index*, not the strings, so a `&'static str` handed out earlier stays valid even
/// though the `Symbol` that produced it no longer resolves to it. The cost is that cleared text
/// is never reclaimed, which is bounded by the total distinct text a process interns -- a
/// compiler run interns every identifier once and then exits, and the tests that clear are short.
struct InternerData {
    strings: Vec<&'static str>,
    lookup: HashMap<&'static str, Symbol>,
}

impl InternerData {
    fn new() -> Self {
        InternerData {
            strings: Vec::new(),
            lookup: HashMap::new(),
        }
    }
}

thread_local! {
    static INTERNER: RefCell<InternerData> = RefCell::new(InternerData::new());
}

pub struct Interner;

impl Interner {
    /// Interns `text`, returning the existing `Symbol` if it's been seen before on this thread.
    pub fn intern(text: &str) -> Symbol {
        INTERNER.with(|interner| {
            let mut interner = interner.borrow_mut();
            if let Some(&sym) = interner.lookup.get(text) {
                return sym;
            }
            let text: &'static str = Box::leak(text.to_string().into_boxed_str());
            let sym = Symbol::from_id(interner.strings.len() as u32);
            interner.strings.push(text);
            interner.lookup.insert(text, sym);
            sym
        })
    }

    /// Resolves a `Symbol` back to the text it was interned from.
    ///
    /// Borrowed rather than cloned: this is called wherever a name is compared or printed --
    /// every primitive-type check, every diagnostic, every debug dump -- and an owned `String`
    /// per call made each of those allocate to read a name the interner already held.
    pub fn resolve(sym: Symbol) -> &'static str {
        INTERNER.with(|interner| interner.borrow().strings[sym.id() as usize])
    }

    /// Discards every interned string on this thread.
    ///
    /// Tests use this for isolation, exactly like [`crate::diag::DiagCtx::clear`].
    pub fn clear() {
        INTERNER.with(|interner| *interner.borrow_mut() = InternerData::new());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn interning_same_text_returns_same_symbol() {
        Interner::clear();
        let a = Interner::intern("foo");
        let b = Interner::intern("foo");
        assert_eq!(a, b);
    }

    #[test]
    fn interning_different_text_returns_different_symbols() {
        Interner::clear();
        let a = Interner::intern("foo");
        let b = Interner::intern("bar");
        assert_ne!(a, b);
    }

    #[test]
    fn resolve_round_trips() {
        Interner::clear();
        let sym = Interner::intern("hello");
        assert_eq!(Interner::resolve(sym), "hello");
    }
}
