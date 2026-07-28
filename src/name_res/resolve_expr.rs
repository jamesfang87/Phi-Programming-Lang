use crate::ast::{Ident, Path};
use crate::hir::{AccessArgs, DefId, ExprKind, HirId, LocalId, Node, OwnerNode, Payload};
use crate::name_res::NameResolver;
use crate::name_res::resolve_results::Res;
use crate::name_res::symbol_table::SymbolTable;

impl<'hir> NameResolver<'hir> {
    pub fn resolve_expr(&mut self, owner_id: DefId, expr_id: LocalId) {
        let hir_id = HirId {
            owner: owner_id,
            local_id: expr_id,
        };
        let Node::Expr(expr) = self.hir.node(hir_id) else {
            unreachable!("Expected a expr's local id to name an expr");
        };

        match &expr.kind {
            ExprKind::Path(path) => {
                let res = self.resolve_value_path(owner_id, path);
                self.results.add(hir_id, res);
            }
            ExprKind::Unary { operand, .. }
            | ExprKind::Borrow { operand, .. }
            | ExprKind::Try(operand) => {
                self.resolve_expr(owner_id, *operand);
            }
            ExprKind::Binary { lhs, rhs, .. }
            | ExprKind::Assign { lhs, rhs }
            | ExprKind::AssignOp { lhs, rhs, .. } => {
                self.resolve_expr(owner_id, *lhs);
                self.resolve_expr(owner_id, *rhs);
            }
            ExprKind::Call { callee, args } => {
                self.resolve_expr(owner_id, *callee);
                for &arg in args {
                    self.resolve_expr(owner_id, arg);
                }
            }
            ExprKind::If {
                cond,
                then_branch,
                else_branch,
            } => {
                self.resolve_expr(owner_id, *cond);
                self.resolve_block(owner_id, *then_branch);
                if let Some(else_branch) = else_branch {
                    self.resolve_expr(owner_id, *else_branch);
                }
            }
            ExprKind::Block(block_id) => self.resolve_block(owner_id, *block_id),
            ExprKind::Index { base, index } => {
                self.resolve_expr(owner_id, *base);
                self.resolve_expr(owner_id, *index);
            }
            ExprKind::Ctor { path, payload } => {
                if let Some(path) = path {
                    let res = self.resolve_struct_path(owner_id, path);
                    self.results.add(hir_id, res);
                }

                for &(_, field_expr) in payload {
                    self.resolve_expr(owner_id, field_expr);
                }
            }
            ExprKind::Variant { payload, .. } => match payload {
                Payload::None => {}
                Payload::Single(value) => self.resolve_expr(owner_id, *value),
                Payload::Record(fields) => {
                    for &(_, field_expr) in fields {
                        self.resolve_expr(owner_id, field_expr);
                    }
                }
            },
            ExprKind::Access { base, member, args } => {
                self.resolve_access(owner_id, hir_id, *base, *member, args)
            }
            ExprKind::Tuple(elems) => {
                for &elem in elems {
                    self.resolve_expr(owner_id, elem);
                }
            }
            ExprKind::Range { lo, hi, .. } => {
                if let Some(lo) = lo {
                    self.resolve_expr(owner_id, *lo);
                }
                if let Some(hi) = hi {
                    self.resolve_expr(owner_id, *hi);
                }
            }
            ExprKind::Match { scrutinee, arms } => {
                self.resolve_expr(owner_id, *scrutinee);

                for &arm_id in arms {
                    let Node::Arm(arm) = self.hir.node(HirId {
                        owner: owner_id,
                        local_id: arm_id,
                    }) else {
                        unreachable!("expected arms of match statement to be arm")
                    };

                    self.symbol_tab.push_scope();
                    self.bind_pat(owner_id, arm.pat);
                    self.resolve_expr(owner_id, arm.body);
                    self.symbol_tab.pop_scope();
                }
            }
            ExprKind::Loop { body, .. } => self.resolve_block(owner_id, *body),
            ExprKind::Spawn(block_id) | ExprKind::Concurrent(block_id) => {
                self.resolve_block(owner_id, *block_id);
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

    fn resolve_access(
        &mut self,
        owner_id: DefId,
        hir_id: HirId,
        base: LocalId,
        member: Ident,
        args: &AccessArgs,
    ) {
        // The payload or argument list is resolved the same way regardless of what the access
        // turns out to be.
        match args {
            AccessArgs::None => {}
            AccessArgs::Call(args) => {
                for &arg in args {
                    self.resolve_expr(owner_id, arg);
                }
            }
            AccessArgs::Record(fields) => {
                for &(_, field_expr) in fields {
                    self.resolve_expr(owner_id, field_expr);
                }
            }
        }

        match self.enum_named_by(owner_id, base) {
            Some(enum_def) => {
                let res = self
                    .symbol_tab
                    .lookup_variant(enum_def, member.text)
                    .unwrap_or_else(|| {
                        SymbolTable::report_not_found(member);
                        Res::Err
                    });
                self.results.add(hir_id, res);
            }
            // A value, so the member is a field or a method: deferred to typeck.
            None => self.resolve_expr(owner_id, base),
        }
    }

    fn enum_named_by(&self, owner_id: DefId, base: LocalId) -> Option<DefId> {
        let Node::Expr(base) = self.hir.node(HirId {
            owner: owner_id,
            local_id: base,
        }) else {
            unreachable!("Expected an access base's local id to name an expr");
        };
        let ExprKind::Path(path) = &base.kind else {
            return None;
        };
        if let [name] = path.segments.as_slice()
            && self.symbol_tab.lookup(name.text).is_some()
        {
            return None;
        }
        let def_id = self.symbol_tab.lookup_type_path(owner_id, path)?;
        matches!(self.hir.owner(def_id), OwnerNode::Enum(_)).then_some(def_id)
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
