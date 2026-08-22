use std::collections::HashMap;

use crate::mir::BasicBlock;

pub struct Lattice<SomeState> {
    pub entry: HashMap<BasicBlock, SomeState>,
    pub exit: HashMap<BasicBlock, SomeState>,
}

impl<SomeState> Lattice<SomeState> {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn set_entry(&mut self, block: BasicBlock, state: SomeState) -> Option<SomeState> {
        self.entry.insert(block, state)
    }

    pub fn set_exit(&mut self, block: BasicBlock, state: SomeState) -> Option<SomeState> {
        self.exit.insert(block, state)
    }

    pub fn entry(&self, block: BasicBlock) -> Option<&SomeState> {
        self.entry.get(&block)
    }

    pub fn exit(&self, block: BasicBlock) -> Option<&SomeState> {
        self.exit.get(&block)
    }
}

impl<SomeState> Default for Lattice<SomeState> {
    fn default() -> Self {
        Self {
            entry: HashMap::new(),
            exit: HashMap::new(),
        }
    }
}
