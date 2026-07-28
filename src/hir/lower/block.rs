//! Lowers blocks and statements.

use crate::ast;
use crate::hir::ids::LocalId;
use crate::hir::lower::owner::OwnerLowerer;
use crate::hir::{LetStmt, StmtKind, WithLend};

impl OwnerLowerer<'_> {
    /// Lowers a block. If the last statement is a bare expression without a trailing semicolon,
    /// it becomes the block's tail value instead of an ordinary statement; every other statement
    /// lowers as-is.
    pub(super) fn lower_block(&mut self, b: &ast::Block) -> LocalId {
        self.synth_block(b.span, |low, _id| {
            let mut stmts = Vec::new();
            let mut tail = None;
            for (i, s) in b.stmts.iter().enumerate() {
                if i + 1 == b.stmts.len()
                    && let ast::StmtKind::Expr { expr, semi: false } = &s.kind
                {
                    tail = Some(low.lower_expr(expr));
                    continue;
                }
                stmts.push(low.lower_stmt(s));
            }
            (stmts, tail)
        })
    }

    pub(super) fn lower_stmt(&mut self, s: &ast::Stmt) -> LocalId {
        self.synth_stmt(s.span, |low, _id| match &s.kind {
            ast::StmtKind::While { cond, body } => {
                let loop_expr = low.lower_while(cond, body);
                StmtKind::Expr(loop_expr)
            }
            ast::StmtKind::WhileLet {
                pat,
                scrutinee,
                body,
            } => {
                let loop_expr = low.lower_while_let(pat, scrutinee, body);
                StmtKind::Expr(loop_expr)
            }
            ast::StmtKind::For { name, iter, body } => {
                let loop_expr = low.lower_for(name, iter, body);
                StmtKind::Expr(loop_expr)
            }
            ast::StmtKind::Break => StmtKind::Break,
            ast::StmtKind::Continue => StmtKind::Continue,
            ast::StmtKind::Return { ret } => StmtKind::Return(Some(low.lower_expr(ret))),
            ast::StmtKind::Defer { defer } => StmtKind::Defer(low.lower_expr(defer)),
            ast::StmtKind::Decl(decl) => StmtKind::Let(low.lower_decl(decl)),
            ast::StmtKind::With { lends, body } => {
                let lends = lends.iter().map(|l| low.lower_with_lend(l)).collect();
                let body = low.lower_block(body);
                StmtKind::With { lends, body }
            }
            ast::StmtKind::Expr { expr, .. } => StmtKind::Expr(low.lower_expr(expr)),
            ast::StmtKind::Error => StmtKind::Error,
        })
    }

    pub(super) fn lower_decl(&mut self, d: &ast::DeclStmt) -> LetStmt {
        let pat = self.lower_pat(&d.name);
        let ty = d.ty.as_ref().map(|t| self.lower_ty(t));
        let init = self.lower_expr(&d.expr);
        LetStmt {
            mutability: d.mutability,
            pat,
            ty,
            init,
        }
    }

    pub(super) fn lower_with_lend(&mut self, l: &ast::WithStmtLend) -> WithLend {
        let pat = self.lower_pat(&l.name);
        let ty = l.ty.as_ref().map(|t| self.lower_ty(t));
        let init = self.lower_expr(&l.expr);
        WithLend {
            pat,
            ty,
            init,
            span: l.span,
        }
    }
}
