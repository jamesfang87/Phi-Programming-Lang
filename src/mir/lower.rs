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
use crate::hir::{DefId, Hir, HirId, Node, OwnerNode, StmtKind};
use crate::langitems::hir::LangItems;
use crate::mir::adt::{collect_adt_defs, AdtDef};
use crate::mir::def_names::{collect_def_names, DefNames};
use crate::mir::lower::ctx::BodyLowerCtx;
use crate::mir::vtables::{collect_vtables, VtableInfo};
use crate::mir::{AnyMode, Body};
use crate::typeck::results::TypeResolutions;
use crate::typeck::ty::{Ty, TyKind};
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
pub struct Mir {
    pub bodies: HashMap<(DefId, Option<AnyMode>), Body>,
    pub adts: HashMap<DefId, AdtDef>,
    pub vtables: HashMap<(Ty, DefId), VtableInfo>,
    pub array_lens: HashMap<HirId, u64>,
    pub def_names: DefNames,
    pub lang_items: LangItems,
    pub main: Option<DefId>,
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

pub fn lower(hir: &Hir, tcx: &mut TyCtx, types: &TypeResolutions, mode: Mode) -> Mir {
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
        let mut ctx = BodyLowerCtx::new(hir, tcx, types, mode, task.def_id(), task.any_mode());
        let body = ctx.lower_item(task);
        worklist.append(&mut ctx.discovered);
        bodies.insert(key, body);
    }

    Mir {
        bodies,
        adts: collect_adt_defs(hir, types),
        vtables: collect_vtables(hir, types),
        array_lens: collect_array_lens(tcx, hir),
        def_names: collect_def_names(hir),
        lang_items: hir.lang_items().clone(),
        main: find_crate_root_main(hir),
    }
}

fn collect_array_lens(tcx: &TyCtx, hir: &Hir) -> HashMap<HirId, u64> {
    let mut out = HashMap::new();
    for ty in tcx.all_tys() {
        if let TyKind::Array { len: Some(len_id), .. } = tcx.kind(ty) {
            out.entry(*len_id)
                .or_insert_with(|| array_len_from_hir(hir, *len_id));
        }
    }
    out
}

fn array_len_from_hir(hir: &Hir, len_id: HirId) -> u64 {
    use crate::ast::Literal;
    use crate::ast::interner::Interner;
    use crate::hir::ExprKind;

    let expr = hir.expr(len_id);
    match &expr.kind {
        ExprKind::Literal(Literal::Int { value, .. }) => Interner::resolve(*value)
            .parse::<u64>()
            .unwrap_or_else(|_| panic!("array length literal {value:?} does not parse as u64")),
        _ => panic!("array length must be an integer literal in v1 codegen"),
    }
}

fn is_named_main(hir: &Hir, def: DefId) -> bool {
    matches!(
        hir.def(def),
        OwnerNode::Function(f) if crate::ast::interner::Interner::resolve(f.name.text) == "main"
    )
}

fn find_crate_root_main(hir: &Hir) -> Option<DefId> {
    let root = hir.root();
    let mut candidates: Vec<DefId> = root
        .items
        .iter()
        .copied()
        .filter(|&def| is_named_main(hir, def))
        .collect();

    for &item in &root.items {
        if let OwnerNode::Module(child) = hir.def(item) {
            candidates.extend(
                child
                    .items
                    .iter()
                    .copied()
                    .filter(|&def| is_named_main(hir, def)),
            );
        }
    }

    match candidates.as_slice() {
        [] => None,
        [one] => Some(*one),
        _ => panic!(
            "found {} `main` functions at the crate root; the OS entry point is ambiguous: {:?}",
            candidates.len(),
            candidates
        ),
    }
}
