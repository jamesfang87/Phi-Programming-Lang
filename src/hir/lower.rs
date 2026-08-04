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

use crate::ast::{Ast, ModId};
use crate::hir::Hir;
use crate::hir::ids::DefId;
use ctx::LoweringCtx;

/// Lowers a build's whole [`Ast`] into one `Hir`.
///
/// Every module gets its `DefId` before any of them is lowered. That is what lets a module's
/// `Module` node list the submodules nested in it while they are still unlowered, and it is sound
/// because [`Ast`] numbers a module after the module above it, so a parent's id is always
/// allocated before its children ask for it.
pub fn lower_unit(ast: &Ast) -> Hir {
    let mut cx = LoweringCtx::new();

    let mut module_defs: Vec<DefId> = Vec::new();
    for mod_id in ast.mod_ids() {
        // The parent is a lower `ModId` than the module itself, so its `DefId` is already in
        // `module_defs` by the time this reaches for it. The annotation is there because this is
        // the one place the two id spaces meet: `module_defs` is indexed by `ModId` and holds
        // `DefId`s.
        let parent: Option<ModId> = ast.parent(mod_id);
        module_defs.push(cx.items.alloc(parent.map(|id| module_defs[id.index()])));
    }

    for mod_id in ast.mod_ids() {
        let module = ast.module(mod_id);
        let child_defs = module
            .children
            .iter()
            .map(|&child| module_defs[child.index()])
            .collect();
        cx.lower_module(module_defs[mod_id.index()], module, child_defs);
    }

    cx.finish(module_defs[ast.root_id().index()])
}
