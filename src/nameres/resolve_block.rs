//! Resolves blocks and statements, threading the scope each `let`/`with` binding introduces
//! through to the statements and expressions that follow it.

use crate::hir::{HirId, Node, StmtKind};
use crate::nameres::NameResolver;

impl<'hir> NameResolver<'hir> {
    pub fn resolve_block(&mut self, id: HirId) {
        let Node::Block(block) = self.hir.node(id) else {
            unreachable!("Expected a block's local id to name a block");
        };

        self.symbol_tab.push_scope(); // enter a new scope for the block
        for &stmt_id in &block.stmts {
            self.resolve_stmt(stmt_id);
        }
        if let Some(expr_id) = block.expr {
            self.resolve_expr(expr_id);
        }
        self.symbol_tab.pop_scope(); // exit the scope
    }

    pub fn resolve_stmt(&mut self, id: HirId) {
        let hir = self.hir;

        let Node::Stmt(stmt) = hir.node(id) else {
            unreachable!("Expected a stmt's local id to name a stmt");
        };

        match &stmt.kind {
            StmtKind::Let { pat, init, .. } => {
                self.resolve_expr(*init);
                self.bind_pat(*pat);
            }
            StmtKind::Expr(expr_id) | StmtKind::Defer(expr_id) => {
                self.resolve_expr(*expr_id);
            }
            StmtKind::With { lends, block } => {
                for lend in lends {
                    self.resolve_expr(lend.init);
                    self.bind_pat(lend.pat);
                }

                self.resolve_block(*block);
            }
            StmtKind::Return(Some(expr_id)) => self.resolve_expr(*expr_id),
            StmtKind::Return(None) | StmtKind::Break | StmtKind::Continue | StmtKind::Error => {}
        }
    }
}
