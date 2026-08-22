//! Block and statement lowering: `let`/`let-else`, `with` lends, block-scoped `defer`,
//! `break`/`continue`/`return`, and the block-level exit-obligation replay every one of those
//! shares.

use crate::ast::Mutability;
use crate::driver::source::SrcSpan;
use crate::hir::{HirId, PatKind, StmtKind};
use crate::mir::lower::ctx::{BodyLowerCtx, ExitObligation};
use crate::mir::{Place, StatementKind, TerminatorKind};

impl<'a> BodyLowerCtx<'a> {
    /// Lowers every statement in `block_id`, then its trailing expression (if it has one) into
    /// `dest`, or discards the trailing expression's value if `dest` is `None`. Opens this
    /// block's own exit-obligation scope on entry and replays it via ordinary fallthrough once
    /// the trailing expression is lowered; an early exit reached from inside the block
    /// (`break`/`continue`/`return`) replays its own copy separately and leaves this scope open,
    /// since whatever lexically follows the early exit -- or a later loop iteration -- still
    /// needs it.
    ///
    /// A statement that unconditionally diverges (`break`, `continue`, `return`, or an
    /// expression statement whose own type is `Never`) makes every statement after it, and the
    /// block's own trailing expression, dead code: neither is lowered at all. This is not a full
    /// reachability analysis -- an `if`/`match` all of whose arms diverge is not detected, for
    /// instance -- but it is what keeps a block ending in an explicit `return` (the overwhelming
    /// common case) from also trying to assign its own trailing value, of a possibly unrelated
    /// type, into `dest` on a path nothing ever actually reaches.
    pub(crate) fn lower_block(&mut self, block_id: HirId, dest: Option<Place>) {
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

    /// Lowers one statement, and reports whether it unconditionally diverges -- see
    /// [`BodyLowerCtx::lower_block`]'s docs for exactly what that does and does not detect.
    fn lower_stmt(&mut self, stmt_id: HirId) -> bool {
        let stmt = self.hir.stmt(stmt_id);
        let span = stmt.span;
        match stmt.kind {
            StmtKind::Let {
                mutability,
                pat,
                init,
                else_block,
                ..
            } => {
                self.lower_let(mutability, pat, init, else_block, span);
                false
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
                self.register_exit_obligation(ExitObligation::RunDeferred(expr));
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

    /// `let pat = init;` and `let pat = init else { block };`. A pattern with no `else` is
    /// always irrefutable (typeck already rejects a refutable one with no `else`), so this
    /// lowers straight into [`BodyLowerCtx::bind_pat`] with no structural test. With an `else`,
    /// the pattern is tested first; a refutation runs `else_block`, which typeck already
    /// guarantees diverges, so there is no fallthrough to close off, only the reserved block's
    /// mandatory terminator.
    ///
    /// A bare `let name = init;` -- no destructuring, and, with no `else`, always irrefutable --
    /// is special-cased to lower `init` directly into one local for `name`, rather than through an
    /// intermediate "scrutinee" local that `bind_pat` would then copy into a second, separate one:
    /// there is no structure to test and nothing else the scrutinee's own place would be projected
    /// out of. Every other shape keeps the general scrutinee-then-bind path: a `Tuple` pattern
    /// needs a stable place to project each element from, and an `else`'s own refutation test
    /// needs one to test against before any binding happens.
    fn lower_let(
        &mut self,
        mutability: Mutability,
        pat: HirId,
        init: HirId,
        else_block: Option<HirId>,
        span: SrcSpan,
    ) {
        if else_block.is_none()
            && let PatKind::Binding { name, .. } = self.hir.pat(pat).kind
        {
            let init_ty = self.expr_ty(init);
            let local = self.new_local(init_ty, mutability, Some(name), span);
            self.push_stmt(StatementKind::StorageLive(local), span);
            self.lower_expr_into(init, Place::from_local(local));
            self.bind_local(pat, local);
            self.register_exit_obligation(ExitObligation::StorageDead(local));
            return;
        }

        let init_ty = self.expr_ty(init);
        let scrutinee = self.new_local(init_ty, mutability, None, span);
        self.push_stmt(StatementKind::StorageLive(scrutinee), span);
        self.lower_expr_into(init, Place::from_local(scrutinee));

        match else_block {
            None => self.bind_pat(pat, Place::from_local(scrutinee), mutability),
            Some(else_id) => {
                let fail_block = self.new_block();
                self.test_pat(pat, Place::from_local(scrutinee), fail_block);
                self.bind_pat(pat, Place::from_local(scrutinee), mutability);
                let after = self.current_block();

                self.switch_to(fail_block);
                self.lower_block(else_id, None);
                self.set_terminator(TerminatorKind::Unreachable, span);

                self.switch_to(after);
            }
        }
        self.register_exit_obligation(ExitObligation::StorageDead(scrutinee));
    }

    /// A `with` lend's initializer is an ordinary `&`/`&mut` expression (`with a = &x { .. }`);
    /// this lowers it exactly like a `let`'s initializer, and binds the lend's own pattern
    /// straight to that local rather than through [`BodyLowerCtx::bind_pat`]'s general
    /// scrutinee-projection walk, since a lend's pattern is always a plain name in practice.
    fn lower_with_lend(&mut self, lend: &crate::hir::WithLend) {
        let span = lend.span;
        let ty = self.expr_ty(lend.init);
        let PatKind::Binding { name, .. } = self.hir.pat(lend.pat).kind else {
            panic!(
                "mir::lower: a `with` lend pattern other than a plain binding is not yet \
                 implemented"
            );
        };
        // A `with` lend has no `mut` syntax of its own, so, like a `match` arm or a `for`
        // binding, it is left unrestricted; see `StatementKind::CheckMutable`'s own docs.
        let local = self.new_local(ty, Mutability::Mutable, Some(name), span);
        self.push_stmt(StatementKind::StorageLive(local), span);
        self.lower_expr_into(lend.init, Place::from_local(local));
        self.bind_local(lend.pat, local);
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

    fn lower_return(&mut self, value: Option<HirId>, span: SrcSpan) {
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

    /// Replays a list of exit obligations, in the order they are already given (the innermost
    /// block's most-recently-registered obligation first).
    ///
    /// A replay always runs after the block scope the obligations came from has already been
    /// popped (see `lower_block`'s and `StmtKind::With`'s own `pop_block_scope`-then-`replay`
    /// pairing), so a `RunDeferred` obligation cannot lower its expression directly: were it to
    /// need a temporary of its own -- and `new_temp` now registers a `StorageDead` obligation for
    /// every temporary it allocates, not only a `let`/`with` local -- `register_exit_obligation`
    /// would find no open scope to register it against. Opening a scope of its own around exactly
    /// that one expression, then immediately popping and replaying it, gives the deferred
    /// expression's own temporaries the same live range everything else's have, entirely self-
    /// contained within this one replay step, before this loop moves on to the next obligation.
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
