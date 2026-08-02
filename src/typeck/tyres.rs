use std::collections::HashMap;

use crate::hir::{DefId, HirId, LocalId};
use crate::typeck::ty::Ty;

/// The output of type checking: every node whose type the checker worked out, mapped to that
/// type.
///
/// A definition's own type -- the type a `struct` names, the signature of a `fun` -- is in here
/// too, stored under its owner node, which is the node at [`LocalId::OWNER`] of its own arena.
/// Nothing else can be stored under that id, so definitions need no table of their own.
///
/// The [`Ty`] handles stored here are only meaningful paired with the
/// [`TyCtx`](crate::typeck::tyctx::TyCtx) that interned them, which is why
/// [`collect`](crate::typeck::collect) hands both back together.
///
/// [`LocalId::OWNER`]: crate::hir::LocalId::OWNER
#[derive(Default)]
pub struct TypeckResults {
    ty: HashMap<HirId, Ty>,
}

impl TypeckResults {
    pub fn new() -> TypeckResults {
        TypeckResults { ty: HashMap::new() }
    }

    pub fn add(&mut self, node: HirId, ty: Ty) {
        self.ty.insert(node, ty);
    }

    pub fn add_def(&mut self, node: DefId, ty: Ty) {
        let hir_id = HirId {
            owner: node,
            local_id: LocalId::OWNER,
        };

        self.add(hir_id, ty);
    }

    /// The type of `node`, or `None` if this pass never assigned one.
    pub fn get(&self, node: HirId) -> Option<Ty> {
        self.ty.get(&node).copied()
    }

    pub fn len(&self) -> usize {
        self.ty.len()
    }

    pub fn is_empty(&self) -> bool {
        self.ty.is_empty()
    }
}
