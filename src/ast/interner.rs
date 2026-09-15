use std::collections::HashMap;

#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub struct Symbol(u32);

impl Symbol {
    pub(crate) fn from_id(id: u32) -> Symbol {
        Symbol(id)
    }

    pub(crate) fn id(self) -> u32 {
        self.0
    }
}

/// A string interner: maps source text to compact [`Symbol`] handles and back.
///
/// Owned by [`Session`](crate::session::Session). Interned text is leaked so a resolved name is
/// a `&'static str` that outlives the interner that produced it, which lets symbol resolution
/// happen anywhere a [`Symbol`] is available without carrying an interner reference.
#[derive(Default)]
pub struct Interner {
    strings: Vec<&'static str>,
    lookup: HashMap<&'static str, Symbol>,
}

impl Interner {
    pub fn new() -> Self {
        Interner::default()
    }

    pub fn intern(&mut self, text: &str) -> Symbol {
        if let Some(&sym) = self.lookup.get(text) {
            return sym;
        }
        let text: &'static str = Box::leak(text.to_string().into_boxed_str());
        let sym = Symbol::from_id(self.strings.len() as u32);
        self.strings.push(text);
        self.lookup.insert(text, sym);
        sym
    }

    pub fn resolve(&self, sym: Symbol) -> &'static str {
        self.strings[sym.id() as usize]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn interning_same_text_returns_same_symbol() {
        let mut interner = Interner::new();
        let a = interner.intern("foo");
        let b = interner.intern("foo");
        assert_eq!(a, b);
    }

    #[test]
    fn interning_different_text_returns_different_symbols() {
        let mut interner = Interner::new();
        let a = interner.intern("foo");
        let b = interner.intern("bar");
        assert_ne!(a, b);
    }

    #[test]
    fn resolve_round_trips() {
        let mut interner = Interner::new();
        let sym = interner.intern("hello");
        assert_eq!(interner.resolve(sym), "hello");
    }
}
