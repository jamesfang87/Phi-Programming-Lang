//! This module defines the index types the MIR uses to address its own tables. [`Local`]
//! addresses one slot of a [`Body`](crate::mir::Body)'s `local_decls`, [`BasicBlock`] addresses
//! one slot of a `Body`'s `basic_blocks`, and [`VariantIdx`] names one variant of an enum by its
//! declaration order, the position of its `HirId` within `Enum::variants`, independent of both.

/// `Local` addresses one slot of a [`Body`](crate::mir::Body)'s `local_decls`. Slot `0` is
/// always the return place, and slots `1..=arg_count` are always the parameters, by the
/// convention `local_decls` itself documents. Every later slot is a `let` binding or a
/// compiler-introduced temporary.
#[derive(Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Debug)]
pub struct Local(u32);

impl Local {
    /// This is the slot every `Body` reserves for its return value.
    pub const RETURN_PLACE: Local = Local(0);

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

/// `VariantIdx` names one variant of an enum by its position in that enum's declared variant
/// order, matching `hir::Enum::variants`' own order. It is used by
/// [`PlaceElem::Downcast`](crate::mir::PlaceElem),
/// [`AggregateKind::Adt`](crate::mir::AggregateKind), and
/// [`StatementKind::SetDiscriminant`](crate::mir::StatementKind), each of which narrows or builds
/// one variant of an enum named separately by a `DefId`.
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
