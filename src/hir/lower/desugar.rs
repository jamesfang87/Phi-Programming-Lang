//! Desugars `while`/`for` loops into [`ExprKind::Loop`], and the `let` forms of `if`/`while`
//! into [`ExprKind::Match`].

use crate::ast;
use crate::ast::interner::Interner;
use crate::ast::{Ident, Mutability, Path};
use crate::hir::ids::LocalId;
use crate::hir::lower::owner::OwnerLowerer;
use crate::hir::{
    AccessArgs, BindingMode, ExprKind, LetStmt, LoopSource, PatKind, Payload, StmtKind,
};

impl OwnerLowerer<'_> {
    /// `if let pat = scrutinee { then } else { else }` desugars to
    /// `match scrutinee { pat => { then }, _ => { else } }`.
    pub(super) fn lower_if_let(
        &mut self,
        pat: &ast::Pattern,
        scrutinee: &ast::Expr,
        then_branch: &ast::Block,
        else_branch: &Option<Box<ast::Expr>>,
    ) -> ExprKind {
        let span = scrutinee.span;
        let scrutinee = self.lower_expr(scrutinee);

        let match_arm = self.synth_arm(then_branch.span, |low, _arm_id| {
            let pat = low.lower_pat(pat);
            let body = low.synth_expr(then_branch.span, |low, _expr_id| {
                let block = low.lower_block(then_branch);
                ExprKind::Block(block)
            });
            (pat, body)
        });

        let else_arm = self.synth_arm(span, |low, _arm_id| {
            let pat = low.synth_pat(span, |_, _| PatKind::Wildcard);
            let body = match else_branch {
                Some(else_branch) => low.lower_expr(else_branch),
                None => low.synth_expr(span, |low, _expr_id| {
                    let empty = low.synth_block(span, |_, _| (Vec::new(), None));
                    ExprKind::Block(empty)
                }),
            };
            (pat, body)
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
        pat: &ast::Pattern,
        scrutinee: &ast::Expr,
        body: &ast::Block,
    ) -> LocalId {
        let span = scrutinee.span;
        self.synth_expr(span, |low, _loop_id| {
            let loop_body = low.synth_block(body.span, |low, _block_id| {
                let match_expr = low.synth_expr(span, |low, _match_id| {
                    let scrutinee = low.lower_expr(scrutinee);

                    let match_arm = low.synth_arm(body.span, |low, _arm_id| {
                        let pat = low.lower_pat(pat);
                        let arm_body = low.synth_expr(body.span, |low, _expr_id| {
                            let block = low.lower_block(body);
                            ExprKind::Block(block)
                        });
                        (pat, arm_body)
                    });

                    let break_arm = low.synth_arm(span, |low, _arm_id| {
                        let pat = low.synth_pat(span, |_, _| PatKind::Wildcard);
                        let arm_body = low.synth_expr(span, |low, _expr_id| {
                            let brk_block = low.synth_block(span, |low, _bb_id| {
                                let brk = low.synth_stmt(span, |_, _| StmtKind::Break);
                                (vec![brk], None)
                            });
                            ExprKind::Block(brk_block)
                        });
                        (pat, arm_body)
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
                body: loop_body,
            }
        })
    }

    /// `while cond { body }` desugars to `loop { if !cond { break }; body }`.
    ///
    /// Unlike [`Self::lower_while_let`], the body always runs when the loop doesn't break, so it
    /// gets spliced directly into the loop rather than nested inside a match arm.
    pub(super) fn lower_while(&mut self, cond: &ast::Expr, body: &ast::Block) -> LocalId {
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
                        then_branch: break_block,
                        else_branch: None,
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
                body: loop_body,
            }
        })
    }

    /// `for pat in iter { body }`, desugared through the iterator protocol:
    /// `{ let mut __iter = iter; loop { match __iter.next() { Some(pat) => { body }, None => break } } }`.
    /// See `LoopSource::For`.
    pub(super) fn lower_for(
        &mut self,
        pat: &ast::Pattern,
        iter: &ast::Expr,
        body: &ast::Block,
    ) -> LocalId {
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
                let let_stmt = low.synth_stmt(span, move |_, _| {
                    StmtKind::Let(LetStmt {
                        mutability: Mutability::Mutable,
                        pat: iter_pat,
                        ty: None,
                        init: iter_init,
                    })
                });

                let loop_expr = low.synth_expr(span, |low, _loop_id| {
                    let loop_body = low.synth_block(span, |low, _loop_block_id| {
                        let match_expr = low.synth_expr(span, |low, _match_id| {
                            let next_call = low.synth_expr(span, move |low, _call_id| {
                                let receiver = low.synth_expr(span, move |_, _| {
                                    ExprKind::Path(Path {
                                        segments: vec![iter_ident],
                                        span,
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
                                let some_body = low.synth_expr(body.span, |low, _expr_id| {
                                    let block = low.lower_block(body);
                                    ExprKind::Block(block)
                                });
                                (some_pat, some_body)
                            });

                            let none_arm = low.synth_arm(span, |low, _arm_id| {
                                let none_pat = low.synth_pat(span, |_, _| PatKind::Variant {
                                    variant: Ident {
                                        text: Interner::intern("none"),
                                        span,
                                    },
                                    payload: Payload::None,
                                });
                                let none_body = low.synth_expr(span, |low, _expr_id| {
                                    let brk_block = low.synth_block(span, |low, _bb_id| {
                                        let brk = low.synth_stmt(span, |_, _| StmtKind::Break);
                                        (vec![brk], None)
                                    });
                                    ExprKind::Block(brk_block)
                                });
                                (none_pat, none_body)
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
                        body: loop_body,
                    }
                });
                let loop_stmt = low.synth_stmt(span, move |_, _| StmtKind::Expr(loop_expr));

                (vec![let_stmt, loop_stmt], None)
            });
            ExprKind::Block(inner_block)
        })
    }
}
