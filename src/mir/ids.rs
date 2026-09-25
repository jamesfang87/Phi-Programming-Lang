#[derive(Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Debug)]
pub struct Local(u32);

impl Local {

    pub const RETURN_PLACE: Local = Local(0);

    pub const ENVIRONMENT: Local = Local(1);

    pub(crate) fn from_usize(index: usize) -> Self {
        Local(index as u32)
    }

    pub fn index(self) -> usize {
        self.0 as usize
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Debug)]
pub struct BasicBlock(u32);

impl BasicBlock {

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
