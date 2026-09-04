use std::{collections::HashMap, hash::Hash};

pub struct Lattice<Key: Hash + Eq, SomeState> {
    pub entry: HashMap<Key, SomeState>,
    pub exit: HashMap<Key, SomeState>,
}

impl<Key: Hash + Eq, SomeState> Lattice<Key, SomeState> {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn set_entry(&mut self, key: Key, state: SomeState) -> Option<SomeState> {
        self.entry.insert(key, state)
    }

    pub fn set_exit(&mut self, key: Key, state: SomeState) -> Option<SomeState> {
        self.exit.insert(key, state)
    }

    pub fn entry(&self, key: Key) -> Option<&SomeState> {
        self.entry.get(&key)
    }

    pub fn exit(&self, key: Key) -> Option<&SomeState> {
        self.exit.get(&key)
    }
}

impl<Key: Hash + Eq, SomeState> Default for Lattice<Key, SomeState> {
    fn default() -> Self {
        Self {
            entry: HashMap::new(),
            exit: HashMap::new(),
        }
    }
}
