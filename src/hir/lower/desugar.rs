//! Desugars `while`/`for` loops into [`ExprKind::Loop`], and the `let` forms of `if`/`while`
//! into [`ExprKind::Match`].

use crate::ast;
use crate::ast::interner::Interner;
use crate::ast::{Ident, Mutability};
use crate::hir::lower::owner::OwnerLowerer;
use crate::hir::{
    AccessArgs, BindingMode, ExprKind, HirId, LoopSource, PatKind, Payload, StmtKind,
};

impl OwnerLowerer<'_, '_> {
    /// `if let pat = scrutinee { then } else { else }` desugars to
    /// `match scrutinee { pat => { then }, _ => { else } }`.
    pub(super) fn lower_if_let(
        &mut self,
        pat: &ast::Pat,
        scrutinee: &ast::Expr,
        then_block: &ast::Block,
        else_expr: &Option<Box<ast::Expr>>,
    ) -> ExprKind {
        let span = scrutinee.span;
        let scrutinee = self.lower_expr(scrutinee);

        let match_arm = self.synth_arm(then_block.span, |low, _arm_id| {
            let pat = low.lower_pat(pat);
            let block = low.lower_block(then_block);
            (pat, block)
        });

        let else_arm = self.synth_arm(span, |low, _arm_id| {
            let pat = low.synth_pat(span, |_, _| PatKind::Wildcard);
            let block = match else_expr {
                Some(else_expr) => low.lower_expr_as_block(else_expr),
                None => low.synth_block(span, |_, _| (Vec::new(), None)),
            };
            (pat, block)
        });

        ExprKind::Match {
            scrutinee,
            arms: vec![match_arm, else_arm],
        }
    }

    /// `while let pat = scrutinee { body }` ->
    /// `loop { match scrutinee { pat => { body }, _ => break } }`.
    ///
    /// Unlike [`Self::lower_while`], the body can't be spliced into the loop directly -- it only
    /// runs when the pattern matches -- so it stays nested inside the matching arm.
    pub(super) fn lower_while_let(
        &mut self,
        pat: &ast::Pat,
        scrutinee: &ast::Expr,
        body: &ast::Block,
    ) -> HirId {
        let span = scrutinee.span;
        self.synth_expr(span, |low, _loop_id| {
            let loop_body = low.synth_block(body.span, |low, _block_id| {
                let match_expr = low.synth_expr(span, |low, _match_id| {
                    let scrutinee = low.lower_expr(scrutinee);

                    let match_arm = low.synth_arm(body.span, |low, _arm_id| {
                        let pat = low.lower_pat(pat);
                        let block = low.lower_block(body);
                        (pat, block)
                    });

                    let break_arm = low.synth_arm(span, |low, _arm_id| {
                        let pat = low.synth_pat(span, |_, _| PatKind::Wildcard);
                        let block = low.synth_block(span, |low, _bb_id| {
                            let brk = low.synth_stmt(span, |_, _| StmtKind::Break);
                            (vec![brk], None)
                        });
                        (pat, block)
                    });

                    ExprKind::Match {
                        scrutinee,
                        arms: vec![match_arm, break_arm],
                    }
                });
                let match_stmt = low.synth_stmt(span, move |_, _| StmtKind::Expr(match_expr));
                (vec![match_stmt], None)
            });
            ExprKind::Loop {
                source: LoopSource::While,
                block: loop_body,
            }
        })
    }

    /// `while cond { body }` desugars to `loop { if !cond { break }; body }`.
    ///
    /// Unlike [`Self::lower_while_let`], the body always runs when the loop doesn't break, so it
    /// gets spliced directly into the loop rather than nested inside a match arm.
    pub(super) fn lower_while(&mut self, cond: &ast::Expr, body: &ast::Block) -> HirId {
        let span = cond.span;
        self.synth_expr(span, |low, _loop_id| {
            let loop_body = low.synth_block(body.span, |low, _block_id| {
                let guard_if = low.synth_expr(span, |low, _if_id| {
                    let not_cond = low.synth_expr(span, |low, _id| ExprKind::Unary {
                        op: ast::UnaryOp::Not,
                        operand: low.lower_expr(cond),
                    });
                    let break_block = low.synth_block(span, |low, _bb_id| {
                        let brk = low.synth_stmt(span, |_, _| StmtKind::Break);
                        (vec![brk], None)
                    });
                    ExprKind::If {
                        cond: not_cond,
                        then_block: break_block,
                        else_block: None,
                    }
                });
                let guard_stmt = low.synth_stmt(span, move |_, _| StmtKind::Expr(guard_if));

                let mut stmts = vec![guard_stmt];
                for s in &body.stmts {
                    // A `while` body has no value regardless of what its last statement looks
                    // like -- unlike `lower_block`, a trailing bare expression here is just an
                    // ordinary discarded statement, never the loop's value.
                    stmts.push(low.lower_stmt(s));
                }
                (stmts, None)
            });
            ExprKind::Loop {
                source: LoopSource::While,
                block: loop_body,
            }
        })
    }

    /// `for pat in iter { body }`, desugared through the iterator protocol:
    /// `{ let mut __iter = iter; loop { match __iter.next() { Some(pat) => { body }, None => break } } }`.
    /// See `LoopSource::For`.
    pub(super) fn lower_for(
        &mut self,
        pat: &ast::Pat,
        iter: &ast::Expr,
        body: &ast::Block,
    ) -> HirId {
        let span = iter.span;
        self.synth_expr(span, |low, _outer_id| {
            let inner_block = low.synth_block(span, |low, _block_id| {
                let iter_ident = Ident {
                    text: Interner::intern("__iter"),
                    span,
                };

                let iter_init = low.lower_expr(iter);
                let iter_pat = low.synth_pat(span, move |_, _| PatKind::Binding {
                    name: iter_ident,
                    mode: BindingMode::Inferred,
                });
                let let_stmt = low.synth_stmt(span, move |_, _| StmtKind::Let {
                    mutability: Mutability::Mutable,
                    pat: iter_pat,
                    ty: None,
                    init: iter_init,
                    else_block: None,
                });

                let loop_expr = low.synth_expr(span, |low, _loop_id| {
                    let loop_body = low.synth_block(span, |low, _loop_block_id| {
                        let match_expr = low.synth_expr(span, |low, _match_id| {
                            let next_call = low.synth_expr(span, move |low, _call_id| {
                                // `__iter` is bound just above, in this same desugaring, so its
                                // `HirId` is already at hand -- no need to go through
                                // `NameResolutions`, which (being AST-level) never saw this
                                // synthesized binding or this synthesized use of it at all.
                                let receiver = low.synth_expr(span, move |_, _| {
                                    ExprKind::Path(crate::hir::Path {
                                        segments: vec![iter_ident],
                                        span,
                                        res: crate::hir::Res::Local(crate::hir::Local::Variable(
                                            iter_pat,
                                        )),
                                    })
                                });
                                ExprKind::Access {
                                    base: receiver,
                                    member: Ident {
                                        text: Interner::intern("next"),
                                        span,
                                    },
                                    args: AccessArgs::Call(Vec::new()),
                                }
                            });

                            let some_arm = low.synth_arm(span, |low, _arm_id| {
                                let some_pat = low.synth_pat(span, |low, _pat_id| {
                                    let user_pat = low.lower_pat(pat);
                                    PatKind::Variant {
                                        variant: Ident {
                                            text: Interner::intern("some"),
                                            span,
                                        },
                                        payload: Payload::Single(user_pat),
                                    }
                                });
                                let some_block = low.lower_block(body);
                                (some_pat, some_block)
                            });

                            let none_arm = low.synth_arm(span, |low, _arm_id| {
                                let none_pat = low.synth_pat(span, |_, _| PatKind::Variant {
                                    variant: Ident {
                                        text: Interner::intern("none"),
                                        span,
                                    },
                                    payload: Payload::None,
                                });
                                let none_block = low.synth_block(span, |low, _bb_id| {
                                    let brk = low.synth_stmt(span, |_, _| StmtKind::Break);
                                    (vec![brk], None)
                                });
                                (none_pat, none_block)
                            });

                            ExprKind::Match {
                                scrutinee: next_call,
                                arms: vec![some_arm, none_arm],
                            }
                        });
                        let match_stmt =
                            low.synth_stmt(span, move |_, _| StmtKind::Expr(match_expr));
                        (vec![match_stmt], None)
                    });
                    ExprKind::Loop {
                        source: LoopSource::For,
                        block: loop_body,
                    }
                });
                let loop_stmt = low.synth_stmt(span, move |_, _| StmtKind::Expr(loop_expr));

                (vec![let_stmt, loop_stmt], None)
            });
            ExprKind::Block(inner_block)
        })
    }
}
