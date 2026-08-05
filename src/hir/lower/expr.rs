//! Lowers expressions, including promoting closures to their own owner.

use crate::ast;
use crate::ast::Path;
use crate::driver::source::SrcSpan;
use crate::hir::ids::DefId;
use crate::hir::lower::owner::OwnerLowerer;
use crate::hir::{AccessArgs, Closure, ExprKind, HirId, OwnerNode, Payload, PayloadField};

impl OwnerLowerer<'_> {
    pub(super) fn lower_expr(&mut self, e: &ast::Expr) -> HirId {
        let span = e.span;
        self.synth_expr(span, |low, _id| low.lower_expr_kind(&e.kind, span))
    }

    fn lower_expr_kind(&mut self, kind: &ast::ExprKind, span: SrcSpan) -> ExprKind {
        match kind {
            ast::ExprKind::Literal(lit) => ExprKind::Literal(*lit),
            ast::ExprKind::Path(path) => ExprKind::Path(path.clone()),
            ast::ExprKind::Unary { op, operand } => ExprKind::Unary {
                op: *op,
                operand: self.lower_expr(operand),
            },
            ast::ExprKind::Binary { op, lhs, rhs } => ExprKind::Binary {
                op: *op,
                lhs: self.lower_expr(lhs),
                rhs: self.lower_expr(rhs),
            },
            ast::ExprKind::Assign { lhs, rhs } => ExprKind::Assign {
                lhs: self.lower_expr(lhs),
                rhs: self.lower_expr(rhs),
            },
            ast::ExprKind::AssignOp { op, lhs, rhs } => ExprKind::AssignOp {
                op: *op,
                lhs: self.lower_expr(lhs),
                rhs: self.lower_expr(rhs),
            },
            ast::ExprKind::Borrow {
                mutability,
                operand,
            } => ExprKind::Borrow {
                mutability: *mutability,
                operand: self.lower_expr(operand),
            },
            ast::ExprKind::Call { callee, args } => ExprKind::Call {
                callee: self.lower_expr(callee),
                args: args.iter().map(|a| self.lower_expr(a)).collect(),
            },
            ast::ExprKind::Access { base, member, args } => ExprKind::Access {
                base: self.lower_expr(base),
                member: *member,
                args: match args {
                    ast::AccessArgs::None => AccessArgs::None,
                    ast::AccessArgs::Call(args) => {
                        AccessArgs::Call(args.iter().map(|a| self.lower_expr(a)).collect())
                    }
                    ast::AccessArgs::Record(fields) => {
                        AccessArgs::Record(self.lower_record_fields(fields))
                    }
                },
            },
            ast::ExprKind::Index { base, index } => ExprKind::Index {
                base: self.lower_expr(base),
                index: self.lower_expr(index),
            },
            ast::ExprKind::Ctor { path, payload } => ExprKind::Ctor {
                path: path.clone(),
                payload: self.lower_record_fields(payload),
            },
            ast::ExprKind::Variant { variant, payload } => ExprKind::Variant {
                variant: *variant,
                payload: self.lower_expr_payload(payload),
            },
            ast::ExprKind::Tuple(exprs) => {
                ExprKind::Tuple(exprs.iter().map(|e| self.lower_expr(e)).collect())
            }
            ast::ExprKind::Range { lo, hi, inclusive } => ExprKind::Range {
                lo: lo.as_ref().map(|e| self.lower_expr(e)),
                hi: hi.as_ref().map(|e| self.lower_expr(e)),
                inclusive: *inclusive,
            },
            ast::ExprKind::Try(inner) => ExprKind::Try(self.lower_expr(inner)),
            ast::ExprKind::If {
                cond,
                then_block,
                else_expr,
            } => ExprKind::If {
                cond: self.lower_expr(cond),
                then_block: self.lower_block(then_block),
                else_block: else_expr.as_ref().map(|e| self.lower_expr_as_block(e)),
            },
            ast::ExprKind::IfLet {
                pat,
                scrutinee,
                then_block,
                else_expr,
            } => self.lower_if_let(pat, scrutinee, then_block, else_expr),
            ast::ExprKind::Match { scrutinee, arms } => ExprKind::Match {
                scrutinee: self.lower_expr(scrutinee),
                arms: arms.iter().map(|a| self.lower_arm(a)).collect(),
            },
            ast::ExprKind::Spawn(block) => ExprKind::Spawn(self.lower_block(block)),
            ast::ExprKind::Concurrent(block) => ExprKind::Concurrent(self.lower_block(block)),
            ast::ExprKind::Block(block) => ExprKind::Block(self.lower_block(block)),
            ast::ExprKind::Closure { params, ret, body } => {
                ExprKind::Closure(self.lower_closure(span, params, ret, body))
            }
            ast::ExprKind::Error => ExprKind::Error,
        }
    }

    fn lower_expr_payload(&mut self, payload: &ast::Payload<ast::Expr>) -> Payload {
        match payload {
            ast::Payload::None => Payload::None,
            ast::Payload::Single(value) => Payload::Single(self.lower_expr(value)),
            ast::Payload::Record(fields) => Payload::Record(self.lower_record_fields(fields)),
        }
    }

    /// Lowers a record payload's fields, desugaring the `{ l }` shorthand into `{ l: l }` so
    /// that every field has a real expression behind it in the HIR.
    fn lower_record_fields(
        &mut self,
        fields: &[ast::PayloadField<ast::Expr>],
    ) -> Vec<PayloadField> {
        fields
            .iter()
            .map(|f| {
                let value = match &f.value {
                    Some(value) => self.lower_expr(value),
                    None => {
                        let name = f.name;
                        self.synth_expr(f.span, move |_, _| {
                            ExprKind::Path(Path {
                                segments: vec![name],
                                span: name.span,
                            })
                        })
                    }
                };
                PayloadField {
                    name: f.name,
                    value,
                }
            })
            .collect()
    }

    /// Lowers a closure into its own owner, nested under the owner that creates it. Like a free
    /// function, it gets its own arena and `DefId`. Later passes can then look up and process
    /// its body on its own, without going through the expression that creates it.
    fn lower_closure(
        &mut self,
        span: SrcSpan,
        params: &[ast::ClosureParam],
        ret: &Option<ast::Ty>,
        body: &ast::Expr,
    ) -> DefId {
        let item_id = self.cx.items.alloc(Some(self.def_id()));
        let mut ow = OwnerLowerer::new(self.cx, item_id);
        let root = ow.reserve_root();
        let params = params.iter().map(|p| ow.lower_closure_param(p)).collect();
        let ret = ret.as_ref().map(|t| ow.lower_ty(t));
        let block = ow.lower_expr_as_block(body);
        ow.fill(
            root,
            OwnerNode::Closure(Closure {
                hir_id: root,
                params,
                ret,
                block,
                span,
            }),
        );
        ow.finish()
    }

    pub(super) fn lower_arm(&mut self, a: &ast::Arm) -> HirId {
        self.synth_arm(a.span, |low, _id| {
            let pat = low.lower_pat(&a.pat);
            let block = low.lower_expr_as_block(&a.body);
            (pat, block)
        })
    }
}
