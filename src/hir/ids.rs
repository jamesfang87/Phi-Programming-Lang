#[derive(Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Debug)]
pub struct DefId(u32);

impl DefId {
    pub(crate) fn from_usize(index: usize) -> Self {
        DefId(index as u32)
    }

    pub fn index(self) -> usize {
        self.0 as usize
    }

    /// The [`HirId`] of this definition's own declaration node, which every arena stores at
    /// [`LocalId::OWNER`].
    ///
    /// This is the bridge between the two ways the compiler addresses a definition: by `DefId`
    /// when naming the definition itself, and by `HirId` when it is one node among the rest --
    /// which is how [`TypeResolutions`](crate::typeck::results::TypeResolutions) manages to key
    /// a definition's own type in the same table as every other node's.
    pub fn owner_id(self) -> HirId {
        HirId::new(self, LocalId::OWNER)
    }
}

/// A [`LocalId`] identifies one node inside a single owner's arena. `LocalId`s start at zero
/// within each arena and count up densely as nodes are added to it.
///
/// Unlike a [`DefId`], which works globally, a [`LocalId`] only means something relative to the
/// arena it's contained in. For example, `LocalId(3)` in one function's arena and `LocalId(3)`
/// in a different function's arena are two unrelated nodes that happen to share a number.
///
/// A definition identified by a [`DefId`] also has a [`LocalId`] within its own arena. Its own
/// declaration node always sits at [`LocalId::OWNER`]. A function parameter or a struct field,
/// on the other hand, only ever gets a [`LocalId`] and never a [`DefId`] of its own.
#[derive(Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Debug)]
pub struct LocalId(u32);

impl LocalId {
    /// Every arena's own owner node (a `Function`, a `Struct`, a `Module`, and so on) is stored
    /// at this id, index zero, within that arena.
    pub const OWNER: LocalId = LocalId(0);

    pub(crate) fn from_usize(index: usize) -> Self {
        LocalId(index as u32)
    }

    pub fn index(self) -> usize {
        self.0 as usize
    }
}

/// A [`HirId`] is the full address of one HIR node. It holds both the [`DefId`] of the arena
/// the node lives in and the node's own [`LocalId`] within that arena.
///
/// Every node in the program is fully identifiable by its unique [`HirId`].
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub struct HirId {
    pub owner: DefId,
    pub local_id: LocalId,
}

impl HirId {
    pub fn new(owner: DefId, local_id: LocalId) -> Self {
        HirId { owner, local_id }
    }
}
