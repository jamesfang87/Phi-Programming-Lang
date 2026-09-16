mod block;
mod call;
mod closure;
mod ctx;
mod expr;
mod item;
mod pat;
#[cfg(test)]
mod tests;

use std::collections::{HashMap, HashSet};

use crate::hir::{DefId, Hir, Node, OwnerNode, StmtKind};
use crate::mir::lower::ctx::BodyLowerCtx;
use crate::mir::vtables::{VtableInfo, collect_vtables};
use crate::mir::{AnyMode, Body};
use crate::options::Mode;
use crate::session::Session;
use crate::typeck::results::TypeResolutions;
use crate::typeck::ty::ctx::TyCtx;
use crate::typeck::ty::{Ty, TyKind};

/// One unit of lowering work. `Ordinary` is a definition with no `any` anywhere in its
/// signature, lowered exactly once. `AnySpecialized` is a definition whose return type is
/// `any T`, lowered once per mode some call site actually demands -- see the module docs.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub(crate) enum Task {
    Ordinary(DefId),
    AnySpecialized(DefId, AnyMode),
}

impl Task {
    fn def_id(self) -> DefId {
        match self {
            Task::Ordinary(def_id) | Task::AnySpecialized(def_id, _) => def_id,
        }
    }

    fn any_mode(self) -> Option<AnyMode> {
        match self {
            Task::Ordinary(_) => None,
            Task::AnySpecialized(_, mode) => Some(mode),
        }
    }
}

pub struct Mir {
    pub bodies: HashMap<(DefId, Option<AnyMode>), Body>,
    pub vtables: HashMap<(Ty, DefId), VtableInfo>,
}

pub(super) fn is_any_specialized(tcx: &TyCtx, types: &TypeResolutions, def_id: DefId) -> bool {
    let Some(sig) = types.ty_of_def(def_id) else {
        return false;
    };
    let TyKind::Fun { ret: Some(ret), .. } = tcx.kind(sig) else {
        return false;
    };
    matches!(tcx.kind(*ret), TyKind::Any(_))
}

fn item_has_errors(hir: &Hir, tcx: &TyCtx, types: &TypeResolutions, def_id: DefId) -> bool {
    hir.arena(def_id).nodes.iter().any(|node| {
        if matches!(node, Node::Stmt(stmt) if matches!(stmt.kind, StmtKind::Error)) {
            return true;
        }
        types
            .ty(node.hir_id())
            .is_some_and(|ty| matches!(tcx.kind(ty), TyKind::Error))
    })
}

pub fn lower(
    session: &Session,
    hir: &Hir,
    tcx: &mut TyCtx,
    types: &TypeResolutions,
    mode: Mode,
) -> Mir {
    let erroneous: HashSet<DefId> = hir
        .def_ids()
        .filter(|&def_id| item_has_errors(hir, tcx, types, def_id))
        .collect();

    let mut bodies = HashMap::new();
    let mut worklist: Vec<Task> = Vec::new();

    for def_id in hir.def_ids() {
        if erroneous.contains(&def_id) {
            continue;
        }
        match hir.def(def_id) {
            OwnerNode::Function(function) if function.block.is_some() => {
                if !is_any_specialized(tcx, types, def_id) {
                    worklist.push(Task::Ordinary(def_id));
                }
            }
            OwnerNode::Closure(_) => worklist.push(Task::Ordinary(def_id)),
            _ => {}
        }
    }

    while let Some(task) = worklist.pop() {
        let key = (task.def_id(), task.any_mode());
        if bodies.contains_key(&key) || erroneous.contains(&task.def_id()) {
            continue;
        }
        let mut ctx = BodyLowerCtx::new(
            session,
            hir,
            tcx,
            types,
            mode,
            task.def_id(),
            task.any_mode(),
        );
        let body = ctx.lower_item(task);
        worklist.append(&mut ctx.discovered);
        bodies.insert(key, body);
    }

    Mir {
        bodies,
        vtables: collect_vtables(hir, types),
    }
}
