//! Lowers expressions, including promoting closures to their own owner.

use crate::ast;
use crate::driver::source::SrcSpan;
use crate::hir::ids::DefId;
use crate::hir::lower::owner::OwnerLowerer;
use crate::hir::{AccessArgs, Closure, ExprKind, HirId, OwnerNode, Payload, PayloadField};

impl OwnerLowerer<'_, '_> {
    pub(super) fn lower_expr(&mut self, e: &ast::Expr) -> HirId {
        let span = e.span;
        let node_id = e.id;
        self.synth_expr(span, |low, _id| low.lower_expr_kind(node_id, &e.kind, span))
    }

    fn lower_expr_kind(
        &mut self,
        node_id: ast::NodeId,
        kind: &ast::ExprKind,
        span: SrcSpan,
    ) -> ExprKind {
        match kind {
            ast::ExprKind::Literal(lit) => ExprKind::Literal(*lit),
            ast::ExprKind::Path(path) => ExprKind::Path(self.cx.lower_path(node_id, path)),
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
            // `path` is looked up through `lower_ctor_path`, not `lower_path`: unlike every
            // other path kind, `Resolver::visit_expr` (`src/nameres/resolver.rs`) does
            // not yet record an entry for a struct literal's own name, so a missing entry here
            // is a known gap in AST-level resolution rather than a lowering bug. See
            // `LoweringCtx::lower_ctor_path`.
            ast::ExprKind::Ctor { path, payload } => ExprKind::Ctor {
                path: path
                    .as_ref()
                    .map(|path| self.cx.lower_ctor_path(node_id, path)),
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
    ///
    /// The shorthand's implicit value has no `ast::Expr` behind it -- it's synthesized here,
    /// during lowering, from the field's own name -- so AST-level resolution can't key its
    /// answer under an `Expr`'s `NodeId` the way [`Self::lower_expr`] does. It keys under the
    /// `PayloadField`'s own `NodeId` instead, against the same single-segment path this
    /// synthesizes (`Resolver::visit_record_fields` in `src/nameres/resolver.rs`), which
    /// is why this looks the answer up with `f.id` as the owner rather than treating it as
    /// unresolved.
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
                        let path = ast::Path {
                            segments: vec![name],
                            span: name.span,
                        };
                        let path = self.cx.lower_path(f.id, &path);
                        self.synth_expr(f.span, move |_, _| ExprKind::Path(path))
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
