/// `Local` addresses one slot of a [`Body`](crate::mir::Body)'s `local_decls`. Slot `0` is
/// always the return place, and slots `1..=arg_count` are always the parameters, by the
/// convention `local_decls` itself documents. Every later slot is a `let` binding or a
/// compiler-introduced temporary.
#[derive(Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Debug)]
pub struct Local(u32);

impl Local {
    /// This is the slot every `Body` reserves for its return value.
    pub const RETURN_PLACE: Local = Local(0);

    pub const ENVIRONMENT: Local = Local(1);

    pub(crate) fn from_usize(index: usize) -> Self {
        Local(index as u32)
    }

    pub fn index(self) -> usize {
        self.0 as usize
    }
}

/// `BasicBlock` addresses one slot of a [`Body`](crate::mir::Body)'s `basic_blocks`.
#[derive(Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Debug)]
pub struct BasicBlock(u32);

impl BasicBlock {
    /// Every `Body` begins executing at this block.
    pub const START_BLOCK: BasicBlock = BasicBlock(0);

    pub(crate) fn from_usize(index: usize) -> Self {
        BasicBlock(index as u32)
    }

    pub fn index(self) -> usize {
        self.0 as usize
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Debug)]
pub struct VariantIdx(u32);

impl VariantIdx {
    pub(crate) fn from_usize(index: usize) -> Self {
        VariantIdx(index as u32)
    }

    pub fn index(self) -> usize {
        self.0 as usize
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Debug)]
pub struct StatementId(u32);

impl StatementId {
    pub(crate) fn from_usize(index: usize) -> Self {
        StatementId(index as u32)
    }

    pub fn index(self) -> usize {
        self.0 as usize
    }
}
