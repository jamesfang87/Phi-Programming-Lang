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
/// [`check`](crate::typeck::check) hands both back together.
///
/// [`LocalId::OWNER`]: crate::hir::LocalId::OWNER
#[derive(Default)]
pub struct TypeResolutions {
    ty: HashMap<HirId, Ty>,
}

impl TypeResolutions {
    pub fn new() -> TypeResolutions {
        TypeResolutions { ty: HashMap::new() }
    }

    pub fn record(&mut self, id: HirId, ty: Ty) {
        self.ty.insert(id, ty);
    }

    /// Records the type a definition itself has: what a `struct` names, what a `fun`'s signature
    /// is.
    pub fn record_def(&mut self, def: DefId, ty: Ty) {
        self.record(
            HirId {
                owner: def,
                local_id: LocalId::OWNER,
            },
            ty,
        );
    }

    /// The type of `id`, or `None` if this pass never assigned one.
    pub fn ty(&self, id: HirId) -> Option<Ty> {
        self.ty.get(&id).copied()
    }

    /// The type of the definition `def` itself, the counterpart to
    /// [`record_def`](TypeResolutions::record_def).
    pub fn ty_of_def(&self, def: DefId) -> Option<Ty> {
        self.ty(HirId {
            owner: def,
            local_id: LocalId::OWNER,
        })
    }

    /// Iterates every node recorded so far, alongside the type assigned to it. Used by the
    /// `--debug` dump; see [`crate::driver::emit_debug::print_typeck`].
    pub fn iter(&self) -> impl Iterator<Item = (HirId, Ty)> + '_ {
        self.ty.iter().map(|(&id, &ty)| (id, ty))
    }
}
