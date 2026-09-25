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

use crate::driver::cli::Mode;
use crate::hir::{DefId, Hir, Node, OwnerNode, StmtKind};
use crate::mir::lower::ctx::BodyLowerCtx;
use crate::mir::vtables::{VtableInfo, collect_vtables};
use crate::mir::{AnyMode, Body};
use crate::session::Session;
use crate::typeck::results::TypeResolutions;
use crate::typeck::ty::ctx::TyCtx;
use crate::typeck::ty::{Ty, TyKind};

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
    let erroneous = collect_erroneous_defs(hir, tcx, types);
    let mut worklist = seed_worklist(hir, tcx, types, &erroneous);
    let mut bodies = HashMap::new();
    lower_worklist(
        session,
        hir,
        tcx,
        types,
        mode,
        &erroneous,
        &mut worklist,
        &mut bodies,
    );

    Mir {
        bodies,
        vtables: collect_vtables(hir, types),
    }
}

fn collect_erroneous_defs(hir: &Hir, tcx: &TyCtx, types: &TypeResolutions) -> HashSet<DefId> {
    hir.def_ids()
        .filter(|&def_id| item_has_errors(hir, tcx, types, def_id))
        .collect()
}

fn seed_worklist(
    hir: &Hir,
    tcx: &TyCtx,
    types: &TypeResolutions,
    erroneous: &HashSet<DefId>,
) -> Vec<Task> {
    let mut worklist = Vec::new();
    for def_id in hir.def_ids() {
        if erroneous.contains(&def_id) {
            continue;
        }
        if let Some(task) = body_task(hir, tcx, types, def_id) {
            worklist.push(task);
        }
    }
    worklist
}

/// Returns the task that lowers `def_id`'s body, or `None` when the def has no body to lower.
fn body_task(hir: &Hir, tcx: &TyCtx, types: &TypeResolutions, def_id: DefId) -> Option<Task> {
    match hir.def(def_id) {
        OwnerNode::Function(function)
            if function.block.is_some() && !is_any_specialized(tcx, types, def_id) =>
        {
            Some(Task::Ordinary(def_id))
        }
        OwnerNode::Closure(_) => Some(Task::Ordinary(def_id)),
        _ => None,
    }
}

#[allow(clippy::too_many_arguments)]
fn lower_worklist(
    session: &Session,
    hir: &Hir,
    tcx: &mut TyCtx,
    types: &TypeResolutions,
    mode: Mode,
    erroneous: &HashSet<DefId>,
    worklist: &mut Vec<Task>,
    bodies: &mut HashMap<(DefId, Option<AnyMode>), Body>,
) {
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
}
