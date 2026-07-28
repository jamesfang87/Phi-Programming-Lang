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

use crate::ast;
use crate::hir::Hir;
use ctx::LoweringCtx;

/// Lowers every parsed file [`ast::ParsedSrcFile`] of a build into one `Hir`.
pub fn lower_unit(units: &[ast::ParsedSrcFile]) -> Hir {
    let mut ctx = LoweringCtx::new();
    for unit in units {
        let module = match &unit.module {
            Some(decl) => ctx.module_for_path(&decl.path.segments),
            None => ctx.root_module(),
        };
        ctx.lower_file(module, unit);
    }
    ctx.finish()
}
