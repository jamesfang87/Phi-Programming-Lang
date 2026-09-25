use crate::ast::Ident;
use crate::driver::source::SrcSpan;
use crate::hir::{HirId, PatKind, StmtKind};
use crate::mir::lower::ctx::{BodyLowerCtx, ExitObligation};
use crate::mir::{Place, StatementKind, TerminatorKind};
use crate::typeck::ty::Ty;

impl<'a> BodyLowerCtx<'a> {
    pub(crate) fn lower_block(&mut self, block_id: impl Into<HirId>, dest: Option<Place>) {
        let block_id = block_id.into();
        let block = self.hir.block(block_id);
        let stmts = block.stmts.clone();
        let trailing = block.expr;
        let span = block.span;

        self.push_block_scope();
        let mut diverged = false;
        for stmt_id in stmts {
            if diverged {
                break;
            }
            diverged = self.lower_stmt(stmt_id);
        }
        if !diverged {
            match (trailing, dest) {
                (Some(expr_id), Some(dest)) => self.lower_expr_into(expr_id, dest),
                (Some(expr_id), None) => self.lower_expr_discarding(expr_id),
                (None, Some(dest)) => self.assign_unit(dest, span),
                (None, None) => {}
            }
        }
        let obligations = self.pop_block_scope();
        self.replay_obligations(&obligations);
    }

    fn lower_stmt(&mut self, stmt_id: impl Into<HirId>) -> bool {
        let stmt_id = stmt_id.into();
        let stmt = self.hir.stmt(stmt_id);
        let span = stmt.span;
        match stmt.kind {
            StmtKind::Let {
                pat,
                init,
                else_block,
                ..
            } => {
                let ty = self.expr_ty(init);
                let diverges = matches!(self.tcx.kind(ty), crate::typeck::ty::TyKind::Never);
                self.lower_let(pat, init, else_block, span);
                diverges
            }
            StmtKind::With { ref lends, block } => {
                let lends = lends.clone();
                self.push_block_scope();
                for lend in &lends {
                    self.lower_with_lend(lend);
                }
                self.lower_block(block, None);
                let obligations = self.pop_block_scope();
                self.replay_obligations(&obligations);
                false
            }
            StmtKind::Break => {
                self.lower_break(span);
                true
            }
            StmtKind::Continue => {
                self.lower_continue(span);
                true
            }
            StmtKind::Return(value) => {
                self.lower_return(value, span);
                true
            }
            StmtKind::Defer(expr) => {
                self.register_exit_obligation(ExitObligation::RunDeferred(expr.into()));
                false
            }
            StmtKind::Expr(expr) => {
                let ty = self.expr_ty(expr);
                let diverges = matches!(self.tcx.kind(ty), crate::typeck::ty::TyKind::Never);
                self.lower_expr_discarding(expr);
                diverges
            }
            StmtKind::Error => {
                unreachable!("a fully type-checked body contains no StmtKind::Error")
            }
        }
    }

    fn lower_let(
        &mut self,
        pat: impl Into<HirId>,
        init: impl Into<HirId>,
        else_block: Option<impl Into<HirId>>,
        span: SrcSpan,
    ) {
        let (pat, init) = (pat.into(), init.into());
        let else_block = else_block.map(Into::into);
        let init_ty = self.compute_init_ty(init);

        if else_block.is_none()
            && let PatKind::Binding { name, .. } = self.hir.pat(pat).kind
        {
            self.lower_binding_let(pat, init, name, init_ty, span);
            return;
        }
        self.lower_scrutinee_let(pat, init, else_block, init_ty, span);
    }

    /// Returns the type a `let` initializer is stored at, which is its unsized coercion target
    /// when it has one and its own type otherwise.
    fn compute_init_ty(&mut self, init: HirId) -> Ty {
        self.types
            .unsize(init)
            .unwrap_or_else(|| self.expr_ty(init))
    }

    fn lower_binding_let(
        &mut self,
        pat: HirId,
        init: HirId,
        name: Ident,
        init_ty: Ty,
        span: SrcSpan,
    ) {
        let local = self.new_local(init_ty, Some(name), span);
        self.push_stmt(StatementKind::StorageLive(local), span);
        self.lower_expr_into(init, Place::from_local(local));
        self.bind_local(pat, local);
        self.register_exit_obligation(ExitObligation::StorageDead(local));
    }

    fn lower_scrutinee_let(
        &mut self,
        pat: HirId,
        init: HirId,
        else_block: Option<HirId>,
        init_ty: Ty,
        span: SrcSpan,
    ) {
        let scrutinee = self.new_local(init_ty, None, span);
        self.push_stmt(StatementKind::StorageLive(scrutinee), span);
        self.lower_expr_into(init, Place::from_local(scrutinee));

        match else_block {
            None => self.bind_pat(pat, Place::from_local(scrutinee)),
            Some(else_id) => self.lower_let_else(pat, else_id, scrutinee, span),
        }
        self.register_exit_obligation(ExitObligation::StorageDead(scrutinee));
    }

    /// Lowers the `else` block of a refutable `let`, which runs when `scrutinee` fails to match.
    fn lower_let_else(
        &mut self,
        pat: HirId,
        else_id: HirId,
        scrutinee: crate::mir::Local,
        span: SrcSpan,
    ) {
        let fail_block = self.new_block();
        self.test_pat(pat, Place::from_local(scrutinee), fail_block);
        self.bind_pat(pat, Place::from_local(scrutinee));
        let after = self.current_block();

        self.switch_to(fail_block);
        self.lower_block(else_id, None);
        self.set_terminator(TerminatorKind::Unreachable, span);

        self.switch_to(after);
    }

    fn lower_with_lend(&mut self, lend: &crate::hir::WithLend) {
        let span = lend.span;
        let ty = self.expr_ty(lend.init);
        let name = match self.hir.pat(lend.pat).kind {
            PatKind::Binding { name, .. } => Some(name),
            _ => None,
        };
        let local = self.new_local(ty, name, span);
        self.push_stmt(StatementKind::StorageLive(local), span);
        self.push_stmt(StatementKind::WithLend(local), span);
        self.lower_expr_into(lend.init, Place::from_local(local));
        match name {
            Some(_) => self.bind_local(lend.pat, local),
            None => self.bind_pat(lend.pat, Place::from_local(local)),
        }
        self.register_exit_obligation(ExitObligation::StorageDead(local));
    }

    fn lower_break(&mut self, span: SrcSpan) {
        let (target, obligations) = self
            .break_target()
            .expect("typeck accepts `break` only inside a loop");
        self.replay_obligations(&obligations);
        self.set_terminator(TerminatorKind::Goto { target }, span);
        let fresh = self.new_block();
        self.switch_to(fresh);
    }

    fn lower_continue(&mut self, span: SrcSpan) {
        let (target, obligations) = self
            .continue_target()
            .expect("typeck accepts `continue` only inside a loop");
        self.replay_obligations(&obligations);
        self.set_terminator(TerminatorKind::Goto { target }, span);
        let fresh = self.new_block();
        self.switch_to(fresh);
    }

    fn lower_return(&mut self, value: Option<impl Into<HirId>>, span: SrcSpan) {
        let value = value.map(Into::into);
        let dest = Place::from_local(crate::mir::Local::RETURN_PLACE);
        match value {
            Some(expr_id) => self.lower_expr_into(expr_id, dest),
            None => self.assign_unit(dest, span),
        }
        let obligations = self.obligations_for_return();
        self.replay_obligations(&obligations);
        self.set_terminator(TerminatorKind::Return, span);
        let fresh = self.new_block();
        self.switch_to(fresh);
    }

    pub(crate) fn replay_obligations(&mut self, obligations: &[ExitObligation]) {
        for &obligation in obligations {
            match obligation {
                ExitObligation::StorageDead(local) => {
                    let span = self.local_decl_span(local);
                    self.push_stmt(StatementKind::StorageDead(local), span);
                }
                ExitObligation::RunDeferred(expr_id) => {
                    self.push_block_scope();
                    self.lower_expr_discarding(expr_id);
                    let nested = self.pop_block_scope();
                    self.replay_obligations(&nested);
                }
            }
        }
    }
}
