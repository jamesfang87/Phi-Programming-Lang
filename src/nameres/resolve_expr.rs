use crate::ast::{Ident, Path};
use crate::hir::{AccessArgs, DefId, ExprKind, HirId, OwnerNode, Payload};
use crate::nameres::NameResolver;
use crate::nameres::results::Res;
use crate::nameres::symbol_table::SymbolTable;

impl<'hir> NameResolver<'hir> {
    pub fn resolve_expr(&mut self, expr_id: HirId) {
        let expr = self.hir.expr(expr_id);

        match &expr.kind {
            ExprKind::Path(path) => {
                let res = self.resolve_value_path(expr_id.owner, path);
                self.results.record(expr_id, res);
            }
            ExprKind::Unary { operand, .. }
            | ExprKind::Borrow { operand, .. }
            | ExprKind::Try(operand) => {
                self.resolve_expr(*operand);
            }
            ExprKind::Binary { lhs, rhs, .. }
            | ExprKind::Assign { lhs, rhs }
            | ExprKind::AssignOp { lhs, rhs, .. } => {
                self.resolve_expr(*lhs);
                self.resolve_expr(*rhs);
            }
            ExprKind::Call { callee, args } => {
                self.resolve_expr(*callee);
                for &arg in args {
                    self.resolve_expr(arg);
                }
            }
            ExprKind::If {
                cond,
                then_block,
                else_block,
            } => {
                self.resolve_expr(*cond);
                self.resolve_block(*then_block);
                // Both branches are blocks, even when the source wrote `else if ...` or a bare
                // `else <expr>`: lowering wraps whatever followed `else` in a block of its own.
                if let Some(else_block) = else_block {
                    self.resolve_block(*else_block);
                }
            }
            ExprKind::Block(block_id) => self.resolve_block(*block_id),
            ExprKind::Index { base, index } => {
                self.resolve_expr(*base);
                self.resolve_expr(*index);
            }
            ExprKind::Ctor { path, payload } => {
                if let Some(path) = path {
                    let res = self.resolve_struct_path(expr_id.owner, path);
                    self.results.record(expr_id, res);
                }

                for field in payload {
                    self.resolve_expr(field.value);
                }
            }
            ExprKind::Variant { payload, .. } => match payload {
                Payload::None => {}
                Payload::Single(value) => self.resolve_expr(*value),
                Payload::Record(fields) => {
                    for field in fields {
                        self.resolve_expr(field.value);
                    }
                }
            },
            ExprKind::Access { base, member, args } => {
                self.resolve_access(expr_id, *base, *member, args)
            }
            ExprKind::Tuple(elems) => {
                for &elem in elems {
                    self.resolve_expr(elem);
                }
            }
            ExprKind::Range { lo, hi, .. } => {
                if let Some(lo) = lo {
                    self.resolve_expr(*lo);
                }
                if let Some(hi) = hi {
                    self.resolve_expr(*hi);
                }
            }
            ExprKind::Match { scrutinee, arms } => {
                self.resolve_expr(*scrutinee);

                for &arm_id in arms {
                    let arm = self.hir.arm(arm_id);

                    self.symbol_tab.push_scope();
                    self.bind_pat(arm.pat);
                    self.resolve_block(arm.block);
                    self.symbol_tab.pop_scope();
                }
            }
            ExprKind::Loop { block, .. } => self.resolve_block(*block),
            ExprKind::Spawn(block_id) | ExprKind::Concurrent(block_id) => {
                self.resolve_block(*block_id);
            }
            ExprKind::Closure(closure_id) => self.resolve_closure(*closure_id),

            // Do nothing
            ExprKind::Literal(_) => {}
            ExprKind::Error => {}
        }
    }

    pub fn resolve_value_path(&mut self, owner_id: DefId, path: &Path) -> Res {
        if let [name] = path.segments.as_slice() {
            if let Some(res) = self.symbol_tab.lookup(name.text) {
                return res;
            }
        }

        if let Some(def_id) = self.symbol_tab.lookup_value_path(owner_id, path) {
            return Res::Def(def_id);
        }

        let name = *path
            .segments
            .last()
            .expect("a path always has at least one segment");
        SymbolTable::report_not_found(name);
        Res::Err
    }

    fn resolve_access(&mut self, hir_id: HirId, base: HirId, member: Ident, args: &AccessArgs) {
        // The payload or argument list is resolved the same way regardless of what the access
        // turns out to be.
        match args {
            AccessArgs::None => {}
            AccessArgs::Call(args) => {
                for &arg in args {
                    self.resolve_expr(arg);
                }
            }
            AccessArgs::Record(fields) => {
                for field in fields {
                    self.resolve_expr(field.value);
                }
            }
        }

        match self.enum_named_by(base) {
            Some(enum_def) => {
                let res = self
                    .symbol_tab
                    .lookup_variant(enum_def, member.text)
                    .unwrap_or_else(|| {
                        SymbolTable::report_not_found(member);
                        Res::Err
                    });
                self.results.record(hir_id, res);
            }
            // A value, so the member is a field or a method: deferred to typeck.
            None => self.resolve_expr(base),
        }
    }

    fn enum_named_by(&self, base: HirId) -> Option<DefId> {
        let base = self.hir.expr(base);
        let ExprKind::Path(path) = &base.kind else {
            return None;
        };
        if let [name] = path.segments.as_slice()
            && self.symbol_tab.lookup(name.text).is_some()
        {
            return None;
        }
        let def_id = self.symbol_tab.lookup_type_path(base.hir_id.owner, path)?;
        matches!(self.hir.def(def_id), OwnerNode::Enum(_)).then_some(def_id)
    }

    /// Resolves the path of a struct literal (`Vector2D { .. }`) against the type namespace.
    fn resolve_struct_path(&mut self, owner_id: DefId, path: &Path) -> Res {
        if let Some(def_id) = self.symbol_tab.lookup_type_path(owner_id, path) {
            return Res::Def(def_id);
        }

        let name = *path
            .segments
            .last()
            .expect("a path always has at least one segment");
        SymbolTable::report_not_found(name);
        Res::Err
    }
}
