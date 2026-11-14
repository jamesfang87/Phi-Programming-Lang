use crate::hir::{DefId, Hir, HirId, OwnerNode, StmtKind};

use super::session::session;

fn first_item(hir: &Hir, what: &str, pred: impl Fn(&OwnerNode) -> bool) -> DefId {
    for &item in &hir.root().items {
        if pred(hir.def(item)) {
            return item;
        }
    }
    panic!("fixture declares no top-level {what}")
}

pub fn named_def(hir: &Hir, name: &str) -> DefId {
    for &id in &hir.root().items {
        let text = match hir.def(id) {
            OwnerNode::Struct(s) => s.name.text,
            OwnerNode::Enum(e) => e.name.text,
            OwnerNode::Trait(t) => t.name.text,
            OwnerNode::Function(f) => f.name.text,
            _ => continue,
        };
        if session().resolve(text) == name {
            return id;
        }
    }
    panic!("no definition named {name:?}")
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testing::lower_to_hir;

    const EVERY_KIND: &str = "
        struct Point { x: i32 }
        enum Choice { some: i32, none }
        trait Show { fun show(&self) -> i32; }
        extend Point with Show { fun show(&self) -> i32 { return self.x; } }
        fun f() { let value = 1; }
    ";

    #[test]
    fn each_lookup_finds_its_definition_kind() {
        let hir = lower_to_hir(EVERY_KIND);

        assert!(matches!(hir.def(first_struct(&hir)), OwnerNode::Struct(_)));
        assert!(matches!(
            hir.def(named_def(&hir, "Choice")),
            OwnerNode::Enum(_)
        ));
        assert!(matches!(hir.def(first_trait(&hir)), OwnerNode::Trait(_)));
        assert!(matches!(hir.def(first_extend(&hir)), OwnerNode::Extend(_)));
        assert!(matches!(
            hir.def(first_function(&hir)),
            OwnerNode::Function(_)
        ));
        assert!(matches!(
            hir.def(first_extend_method(&hir)),
            OwnerNode::Function(_)
        ));
    }

    #[test]
    fn find_return_returns_the_statement_and_expression() {
        let hir = lower_to_hir("fun f() -> i32 { return 1; }");
        let function = first_function(&hir);

        let (statement, expression) = find_return(&hir, function);

        assert!(matches!(
            hir.stmt(statement).kind,
            StmtKind::Return(Some(_))
        ));
        assert!(matches!(
            hir.expr(expression).kind,
            crate::hir::ExprKind::Literal(_)
        ));
    }
}
