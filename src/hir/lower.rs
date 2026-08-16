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

pub fn lower_program(ast: &Ast, res: &NameResolutions) -> Hir {
    let mut cx = LoweringCtx::new(res);

    for mod_id in ast.mod_ids() {
        let parent_def = ast.parent(mod_id).map(|id| cx.def_ids[&id]);
        let def_id = cx.def_id_allocator.alloc(parent_def);
        cx.def_ids.insert(mod_id, def_id);

        for item in &ast.module(mod_id).items {
            cx.prealloc_item(def_id, item);
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
