//! AST -> HIR lowering.
//!
//! Split across submodules by what's being lowered: [`ctx`] drives the pass and assembles
//! modules, [`owner`] holds the per-owner arena builder and its generic node-building helpers,
//! and [`items`], [`ty`], [`block`], [`expr`], [`pat`], and [`desugar`] each lower one corner of
//! the AST against it.

mod block;
mod ctx;
mod desugar;
mod expr;
mod items;
mod owner;
mod pat;
#[cfg(test)]
mod tests;
mod ty;

use std::collections::HashMap;

use crate::ast::{Ast, NodeId};
use crate::hir::Hir;
use crate::hir::ids::DefId;
use ctx::LoweringCtx;

/// Lowers a build's whole [`Ast`] into one `Hir`.
///
/// Every module gets its `DefId` before any of them is lowered. That is what lets a module's
/// `Module` node list the submodules nested in it while they are still unlowered, and it is sound
/// because [`Ast::mod_ids`] visits a module after the module above it, so a parent's `DefId` is
/// always allocated before its children ask for it.
pub fn lower_unit(ast: &Ast) -> Hir {
    let mut cx = LoweringCtx::new();

    // Keyed by `NodeId` rather than indexed positionally: a module's `NodeId` is allocated from
    // the same global counter as every other AST node, so it isn't a dense index the way the old
    // `ModId` was.
    let mut module_defs: HashMap<NodeId, DefId> = HashMap::new();
    for mod_id in ast.mod_ids() {
        // `ast.mod_ids()` visits parents before children, so the parent's `DefId` is already in
        // `module_defs` by the time this reaches for it.
        let parent_def = ast.parent(mod_id).map(|id| module_defs[&id]);
        module_defs.insert(mod_id, cx.items.alloc(parent_def));
    }

    for mod_id in ast.mod_ids() {
        let module = ast.module(mod_id);
        let child_defs = module
            .children
            .iter()
            .map(|child| module_defs[child])
            .collect();
        cx.lower_module(module_defs[&mod_id], module, child_defs);
    }

    cx.finish(module_defs[&ast.root_id()])
}
