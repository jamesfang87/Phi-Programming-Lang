//! Resolves blocks and statements, threading the scope each `let`/`with` binding introduces
//! through to the statements and expressions that follow it.

use crate::hir::{DefId, HirId, LocalId, Node, StmtKind};
use crate::nameres::NameResolver;

impl<'hir> NameResolver<'hir> {
    pub fn resolve_block(&mut self, owner_id: DefId, block_id: LocalId) {
        let hir = self.hir;

        let Node::Block(block) = hir.node(HirId {
            owner: owner_id,
            local_id: block_id,
        }) else {
            unreachable!("Expected a block's local id to name a block");
        };

        self.symbol_tab.push_scope(); // enter a new scope for the block
        for &stmt_id in &block.stmts {
            self.resolve_stmt(owner_id, stmt_id);
        }
        if let Some(expr_id) = block.expr {
            self.resolve_expr(owner_id, expr_id);
        }
        self.symbol_tab.pop_scope(); // exit the scope
    }

    pub fn resolve_stmt(&mut self, owner_id: DefId, stmt_id: LocalId) {
        let hir = self.hir;

        let Node::Stmt(stmt) = hir.node(HirId {
            owner: owner_id,
            local_id: stmt_id,
        }) else {
            unreachable!("Expected a stmt's local id to name a stmt");
        };

        match &stmt.kind {
            StmtKind::Let(let_stmt) => {
                self.resolve_expr(owner_id, let_stmt.init);
                self.bind_pat(owner_id, let_stmt.pat);
            }
            StmtKind::Expr(expr_id) | StmtKind::Defer(expr_id) => {
                self.resolve_expr(owner_id, *expr_id);
            }
            StmtKind::With { lends, body } => {
                for lend in lends {
                    self.resolve_expr(owner_id, lend.init);
                    self.bind_pat(owner_id, lend.pat);
                }

                self.resolve_block(owner_id, *body);
            }
            StmtKind::Return(Some(expr_id)) => self.resolve_expr(owner_id, *expr_id),
            StmtKind::Return(None) | StmtKind::Break | StmtKind::Continue | StmtKind::Error => {}
        }
    }
}
