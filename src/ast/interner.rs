use std::cell::RefCell;
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

    pub fn resolve(sym: Symbol) -> &'static str {
        INTERNER.with(|interner| interner.borrow().strings[sym.id() as usize])
    }

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
