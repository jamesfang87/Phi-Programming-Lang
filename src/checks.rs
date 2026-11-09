mod check_main;
mod loop_control;
mod mutability;

use crate::hir::{DefId, Hir, HirId, OwnerNode};

pub use check_main::check as check_entry_point;
pub(crate) use check_main::crate_root_main_candidates;
pub use loop_control::check as check_loop_control;
pub use mutability::check as check_mutability;

pub(crate) fn body_block(hir: &Hir, def_id: DefId) -> Option<HirId> {
    match hir.def(def_id) {
        OwnerNode::Function(function) => function.block.map(HirId::from),
        OwnerNode::Closure(closure) => Some(closure.block.into()),
        _ => None,
    }
}
