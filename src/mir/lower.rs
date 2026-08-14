//! Lowering #2: builds one [`Body`] per function, method, and closure out of a fully
//! type-checked [`Hir`].
//!
//! [`lower_program`] is the entry point. It seeds a worklist with every ordinary (non-`any`)
//! function, method, and closure the `Hir` declares -- found by a flat scan of
//! [`Hir::def_ids`], since every one of those already has its own arena and its own `DefId`
//! regardless of whether it is a free function, a trait/`extend` method, or a closure nested
//! inside another body. A function or method whose return type is `any T` is not seeded
//! directly: [`AnyMode`] specialization is a structural choice (it changes whether a parameter's
//! `Place` needs a `Deref` projection at all), so it can only be decided once some call site
//! demands a specific mode. Lowering that call site pushes the `(DefId, AnyMode)` pair it needs
//! onto the same worklist, `mir::lower::call`'s job; see [`Task`].
//!
//! Ordinary generic substitution needs none of this: a generic body lowers once, with
//! `TyKind::Generic`/`SelfTy` left exactly as `TypeResolutions` already recorded them, and
//! substituting those into a concrete `Body` per instantiation is `mir::monomorphize`'s job, a
//! separate pass over this one's output.

mod block;
mod call;
mod closure;
mod ctx;
mod expr;
mod item;
mod pat;
#[cfg(test)]
mod tests;

use std::collections::HashMap;

use crate::driver::cli::Mode;
use crate::hir::{DefId, Hir, OwnerNode};
use crate::mir::lower::ctx::BodyLowerCtx;
use crate::mir::{AnyMode, Body};
use crate::typeck::results::TypeResolutions;
use crate::typeck::ty::TyKind;
use crate::typeck::tyctx::TyCtx;

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

/// The generic (pre-monomorphization) [`Body`] lowering produced for every [`Task`] actually
/// demanded, keyed by the same `(DefId, Option<AnyMode>)` pair `Task` carries.
/// [`mir::monomorphize`](crate::mir::monomorphize) substitutes each one's own remaining
/// `TyKind::Generic`/`SelfTy` per calling context, keyed by the same pair plus a generic
/// argument list.
pub struct LoweredProgram {
    pub bodies: HashMap<(DefId, Option<AnyMode>), Body>,
}

/// Whether `def_id`'s return type is itself `any T`, the one condition the README ties `any`
/// specialization to (rule 3: "`any` is only meaningful in a function whose return type is `&T`,
/// `&mut T`, or `any T`. It has no effect on a function returning an owned type"). An `any`
/// parameter or `any self` on a definition that does not meet this has, per that same rule, no
/// effect at all: `mir::lower::item` resolves it as a plain owned value, the same as if `any`
/// had not been written, and this definition is lowered once, ordinarily.
pub(super) fn is_any_specialized(tcx: &TyCtx, types: &TypeResolutions, def_id: DefId) -> bool {
    let Some(sig) = types.ty_of_def(def_id) else {
        return false;
    };
    let TyKind::Fun { ret: Some(ret), .. } = tcx.kind(sig) else {
        return false;
    };
    matches!(tcx.kind(*ret), TyKind::Any(_))
}

/// Lowers every function, method, and closure `hir` declares into a [`LoweredProgram`]. `mode`
/// is the project's debug/release profile, which decides whether integer arithmetic gets a
/// [`crate::mir::CheckedBinaryOp`] and an overflow [`crate::mir::Assert`] or wraps silently.
pub fn lower_program(
    hir: &Hir,
    tcx: &mut TyCtx,
    types: &TypeResolutions,
    mode: Mode,
) -> LoweredProgram {
    let mut bodies = HashMap::new();
    let mut worklist: Vec<Task> = Vec::new();

    for def_id in hir.def_ids() {
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
        if bodies.contains_key(&key) {
            continue;
        }
        let mut ctx = BodyLowerCtx::new(hir, tcx, types, mode, task.def_id(), task.any_mode());
        let body = ctx.lower_item(task);
        worklist.append(&mut ctx.discovered);
        bodies.insert(key, body);
    }

    LoweredProgram { bodies }
}
