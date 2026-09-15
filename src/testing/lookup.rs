use crate::hir::{DefId, Hir, HirId, OwnerNode, StmtKind};

use super::session::session;

fn first_item(hir: &Hir, what: &str, pred: impl Fn(&OwnerNode) -> bool) -> DefId {
    hir.root()
        .items
        .iter()
        .copied()
        .find(|&item| pred(hir.def(item)))
        .unwrap_or_else(|| panic!("fixture declares no top-level {what}"))
}

pub fn named_def(hir: &Hir, name: &str) -> DefId {
    hir.root()
        .items
        .iter()
        .copied()
        .find(|&id| {
            let text = match hir.def(id) {
                OwnerNode::Struct(s) => s.name.text,
                OwnerNode::Enum(e) => e.name.text,
                OwnerNode::Trait(t) => t.name.text,
                OwnerNode::Function(f) => f.name.text,
                _ => return false,
            };
            session().resolve(text) == name
        })
        .unwrap_or_else(|| panic!("no definition named {name:?}"))
}

pub fn first_function(hir: &Hir) -> DefId {
    first_item(hir, "function", |def| matches!(def, OwnerNode::Function(_)))
}

pub fn first_struct(hir: &Hir) -> DefId {
    first_item(hir, "struct", |def| matches!(def, OwnerNode::Struct(_)))
}

pub fn first_trait(hir: &Hir) -> DefId {
    first_item(hir, "trait", |def| matches!(def, OwnerNode::Trait(_)))
}

pub fn first_extend(hir: &Hir) -> DefId {
    first_item(hir, "extend block", |def| {
        matches!(def, OwnerNode::Extend(_))
    })
}

pub fn first_extend_method(hir: &Hir) -> DefId {
    hir.extend(first_extend(hir)).methods[0]
}

pub fn find_return(hir: &Hir, def: DefId) -> (HirId, HirId) {
    let function = hir.function(def);
    let block_id = function.block.expect("fixture function has a body");
    let block = hir.block(block_id);

    for &stmt_id in &block.stmts {
        let stmt = hir.stmt(stmt_id);
        if let StmtKind::Return(Some(expr_id)) = stmt.kind {
            return (stmt_id.into(), expr_id.into());
        }
    }
    panic!("fixture function has no `return <expr>;` statement");
}
