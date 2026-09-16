use crate::hir::arena::{Arena, Node};
use crate::hir::ids::{DefId, HirId, LocalId};

pub struct DefIdAllocator {
    /// `parents[i]` is the definition lexically enclosing `DefId(i)`. The root module has no
    /// enclosing definition and is recorded as its own parent, which keeps this dense; see
    /// [`Hir::parent`], which turns that back into a `None`.
    parents: Vec<DefId>,
}

impl DefIdAllocator {
    pub fn new() -> Self {
        DefIdAllocator {
            parents: Vec::new(),
        }
    }

    /// Allocates the next, previously-unused `DefId`, recording `parent` as the definition it is
    /// declared inside. `parent` is `None` only for the root module, which is stored as its own
    /// parent so the table stays dense.
    pub fn alloc(&mut self, parent: Option<DefId>) -> DefId {
        let id = DefId::from_usize(self.parents.len());
        self.parents.push(parent.unwrap_or(id));
        id
    }

    /// Returns the number of `DefId`s allocated so far. Every allocated id is `< len()`.
    pub fn len(&self) -> usize {
        self.parents.len()
    }

    /// Consumes the allocator, yielding the parent of every `DefId` it handed out, indexed by
    /// [`DefId::index`].
    pub fn finish(self) -> Vec<DefId> {
        self.parents
    }
}

impl Default for DefIdAllocator {
    fn default() -> Self {
        Self::new()
    }
}

pub struct ArenaBuilder {
    /// The [`DefId`] of the definition whose arena this builder is building.
    def_id: DefId,

    /// `slots[i]` is the content of the node whose `LocalId` is `i`, or `None` if that id has
    /// been reserved but not yet filled in.
    slots: Vec<Option<Node>>,
}

impl ArenaBuilder {
    pub fn new(def_id: DefId) -> Self {
        ArenaBuilder {
            def_id,
            slots: Vec::new(),
        }
    }

    pub fn def_id(&self) -> DefId {
        self.def_id
    }

    pub fn reserve(&mut self) -> HirId {
        let local_id = LocalId::from_usize(self.slots.len());
        self.slots.push(None);
        HirId::new(self.def_id, local_id)
    }

    /// Writes the real content for a [`LocalId`] previously returned by
    /// [`ArenaBuilder::reserve()`]. Call this exactly once for every reserved id. Lowering must
    /// have already lowered that node's children, if it has any, before you call `fill`.
    pub fn fill(&mut self, id: HirId, node: impl Into<Node>) {
        debug_assert_eq!(
            id.owner, self.def_id,
            "node filled into another owner's arena"
        );
        debug_assert!(
            self.slots[id.local_id.index()].is_none(),
            "ArenaBuilder::fill called twice for the same LocalId"
        );
        self.slots[id.local_id.index()] = Some(node.into());
    }

    /// Consumes the builder once every reserved id has been filled in, producing the finished
    /// arena. Panics if any reserved id was never filled.
    pub fn finish(self) -> Arena {
        let nodes = self
            .slots
            .into_iter()
            .map(|node| node.expect("ArenaBuilder::finish: a reserved LocalId was never filled"))
            .collect();
        Arena { nodes }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::driver::source::SrcSpan;
    use crate::hir::block::Block;

    fn block(id: HirId) -> Node {
        Node::Block(Block {
            hir_id: id.into(),
            stmts: Vec::new(),
            expr: None,
            span: SrcSpan::new(0, 0),
        })
    }

    #[test]
    #[should_panic(expected = "node filled into another owner's arena")]
    fn filling_a_node_under_a_foreign_id_is_caught() {
        let mut builder = ArenaBuilder::new(DefId::from_usize(0));
        let id = builder.reserve();

        let foreign = HirId::new(DefId::from_usize(1), id.local_id);
        builder.fill(foreign, block(foreign));
    }

    #[test]
    fn a_filled_node_is_stored_under_its_own_id() {
        let mut builder = ArenaBuilder::new(DefId::from_usize(0));
        let id = builder.reserve();
        builder.fill(id, block(id));

        let arena = builder.finish();
        assert_eq!(arena.get(id).hir_id(), id);
    }
}
