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

use crate::ast::Ast;
use crate::hir::Hir;
use crate::nameres::NameResolutions;
use ctx::LoweringCtx;

/// Lowers a build's whole [`Ast`] into one `Hir`.
///
/// Every definition in the program -- every module, and every function, struct, enum, trait, and
/// `extend` block declared in one -- gets its `DefId` before any of them is lowered, recorded in
/// [`LoweringCtx::def_ids`] keyed by its `NodeId`. That is what lets a node reference a
/// definition lowered later, or in another module: by the time lowering reaches the reference,
/// the target's id already exists to write down, even though the target's own arena does not
/// exist yet. It is also what lets a module's `Module` node list the submodules nested in it
/// while they are still unlowered.
///
/// Getting this right takes three passes, each depending on the last having finished everywhere
/// before it starts anywhere:
///
///  1. Every module gets its `DefId`, in [`Ast::mod_ids`] order. That order visits a module after
///     the module above it, so a parent's `DefId` is always allocated before its children ask for
///     it.
///  2. Every module's own items -- functions, structs, enums, traits, `extend` blocks -- get
///     theirs, parented to that module. This is sound only because every module already has an
///     id from pass 1.
///  3. A trait's functions and an `extend` block's methods get theirs too, parented to the trait
///     or `extend` item pass 2 just allocated rather than to the module it sits in -- which is
///     why they need their own pass instead of folding into pass 2. See
///     [`LoweringCtx::prealloc_item`], which does passes 2 and 3 together for one item.
///
/// Only pass 4, the lowering proper, actually builds arenas -- passes 1 through 3 only hand out
/// ids.
///
/// `res` is the AST-level name resolution ([`crate::nameres::resolve`]), already run over `ast`
/// by the caller. Lowering writes each of its answers into the [`crate::hir::Path`] of the
/// node it belongs to as that node is built, rather than into a side table keyed by `HirId`.
pub fn lower_unit(ast: &Ast, res: &NameResolutions) -> Hir {
    let mut cx = LoweringCtx::new(res);

    for mod_id in ast.mod_ids() {
        // `ast.mod_ids()` visits parents before children, so the parent's `DefId` is already in
        // `cx.def_ids` by the time this reaches for it.
        let parent_def = ast.parent(mod_id).map(|id| cx.def_ids[&id]);
        let def_id = cx.items.alloc(parent_def);
        cx.def_ids.insert(mod_id, def_id);
    }

    for mod_id in ast.mod_ids() {
        let module_def = cx.def_ids[&mod_id];
        for item in &ast.module(mod_id).items {
            cx.prealloc_item(module_def, item);
        }
    }

    for mod_id in ast.mod_ids() {
        let module = ast.module(mod_id);
        let child_defs = module
            .children
            .iter()
            .map(|child| cx.def_ids[child])
            .collect();
        cx.lower_module(cx.def_ids[&mod_id], module, child_defs);
    }

    let root = cx.def_ids[&ast.root_id()];
    cx.finish(root)
}
