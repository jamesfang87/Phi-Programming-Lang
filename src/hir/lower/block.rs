//! Lowers blocks and statements.

use crate::ast;
use crate::hir::lower::owner::OwnerLowerer;
use crate::hir::{HirId, StmtKind, WithLend};

impl OwnerLowerer<'_, '_> {
    /// Lowers a block. If the last statement is a bare expression without a trailing semicolon,
    /// it becomes the block's tail value instead of an ordinary statement; every other statement
    /// lowers as-is.
    pub(super) fn lower_block(&mut self, b: &ast::Block) -> HirId {
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

    /// Lowers `e` into a block whose tail value is `e`.
    ///
    /// Every HIR construct that owns executable code owns a `Block`, so the surface forms that
    /// take a bare expression -- a match arm, a closure body, an `else if` -- are wrapped here.
    /// An expression already written as a block lowers to that block directly rather than
    /// picking up a second, redundant one.
    pub(super) fn lower_expr_as_block(&mut self, e: &ast::Expr) -> HirId {
        if let ast::ExprKind::Block(b) = &e.kind {
            return self.lower_block(b);
        }

        self.synth_block(e.span, |low, _id| {
            let tail = low.lower_expr(e);
            (Vec::new(), Some(tail))
        })
    }

    pub(super) fn lower_stmt(&mut self, s: &ast::Stmt) -> HirId {
        self.synth_stmt(s.span, |low, _id| match &s.kind {
            ast::StmtKind::While { cond, block } => {
                let loop_expr = low.lower_while(cond, block);
                StmtKind::Expr(loop_expr)
            }
            ast::StmtKind::WhileLet {
                pat,
                scrutinee,
                block,
            } => {
                let loop_expr = low.lower_while_let(pat, scrutinee, block);
                StmtKind::Expr(loop_expr)
            }
            ast::StmtKind::For { pat, iter, block } => {
                let loop_expr = low.lower_for(pat, iter, block);
                StmtKind::Expr(loop_expr)
            }
            ast::StmtKind::Break => StmtKind::Break,
            ast::StmtKind::Continue => StmtKind::Continue,
            ast::StmtKind::Return(ret) => StmtKind::Return(Some(low.lower_expr(ret))),
            ast::StmtKind::Defer(defer) => StmtKind::Defer(low.lower_expr(defer)),
            ast::StmtKind::Let {
                mutability,
                pat,
                ty,
                init,
                else_block,
            } => StmtKind::Let {
                mutability: *mutability,
                pat: low.lower_pat(pat),
                ty: ty.as_ref().map(|t| low.lower_ty(t)),
                init: low.lower_expr(init),
                else_block: else_block.as_ref().map(|b| low.lower_block(b)),
            },
            ast::StmtKind::With { lends, block } => {
                let lends = lends.iter().map(|l| low.lower_with_lend(l)).collect();
                let block = low.lower_block(block);
                StmtKind::With { lends, block }
            }
            ast::StmtKind::Expr { expr, .. } => StmtKind::Expr(low.lower_expr(expr)),
            ast::StmtKind::Error => StmtKind::Error,
        })
    }

    pub(super) fn lower_with_lend(&mut self, l: &ast::WithLend) -> WithLend {
        let pat = self.lower_pat(&l.pat);
        let ty = l.ty.as_ref().map(|t| self.lower_ty(t));
        let init = self.lower_expr(&l.init);
        WithLend {
            pat,
            ty,
            init,
            span: l.span,
        }
    }
}
