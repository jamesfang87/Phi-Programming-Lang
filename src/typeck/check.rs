use std::collections::HashSet;

use crate::ast::{Literal, UnaryOp};
use crate::diagnostics::typeck::pat::{
    report_irrefutable_let_with_else, report_refutable_let_without_else,
};
use crate::diagnostics::typeck::{
    report_binding_type_mismatch, report_bodiless_function, report_body_return_mismatch,
    report_integer_literal_out_of_range, report_negative_literal_for_unsigned_type,
    report_return_mismatch,
};
use crate::driver::source::{FileOrigin, SrcSpan};
use crate::hir::BindingMode;
use crate::hir::visit::{self, Visitor};
use crate::hir::{
    BlockId, DefId, ExprId, ExprKind, Hir, HirId, Node, OwnerNode, PatId, StmtKind, TyId,
    TyKind as HirTyKind, WithLend,
};
use crate::langitems::LangItem;
use crate::nameres::PrimTy;
use crate::typeck::Typeck;
use crate::typeck::results::ResolvedCall;
use crate::typeck::traits::TraitRef;
use crate::typeck::traits::solve::{Goal, Solution};
use crate::typeck::ty::ctx::TyCtx;
use crate::typeck::ty::unify::UnifyError;
use crate::typeck::ty::visitor;
use crate::typeck::ty::{InferVar, Ty, TyKind};

pub mod cast;
pub mod expr;
pub mod pat;

impl<'hir> Typeck<'hir> {
    pub fn check_module(&mut self, module: DefId) {
        BodyChecker(self).visit_module(module);
    }

    fn finalize_function(&mut self, owner: DefId) {
        self.finalize_annotations(owner);
        self.finalize_call_resolutions(owner);
        self.report_integer_literals(owner);
    }

    fn finalize_annotations(&mut self, owner: DefId) {
        for (id, ty) in self.annotations_of(owner) {
            let finalized = self.resolve_and_default(ty);
            self.types.record(id, finalized);
            self.record_copyability(finalized, owner);
        }
    }

    fn finalize_call_resolutions(&mut self, owner: DefId) {
        for (id, call) in self.calls_of(owner) {
            let args = call
                .args
                .iter()
                .map(|&arg| self.resolve_and_default(arg))
                .collect();
            let extend_args = call
                .extend_args
                .iter()
                .map(|&arg| self.resolve_and_default(arg))
                .collect();
            let self_ty = call.self_ty.map(|ty| self.resolve_and_default(ty));
            self.types
                .record_method_call(id, call.def, args, extend_args, self_ty);
        }
    }

    pub(crate) fn annotations_of(&self, owner: DefId) -> Vec<(HirId, Ty)> {
        self.types
            .tys_iter()
            .filter(|(id, _)| self.owner_is_lexically_inside(id.owner, owner))
            .collect()
    }

    fn calls_of(&self, owner: DefId) -> Vec<(HirId, ResolvedCall)> {
        self.types
            .calls_iter()
            .filter(|(id, _)| self.owner_is_lexically_inside(id.owner, owner))
            .map(|(id, call)| (id, call.clone()))
            .collect()
    }

    fn owner_is_lexically_inside(&self, owner: DefId, ancestor: DefId) -> bool {
        let mut current = Some(owner);
        while let Some(def) = current {
            if def == ancestor {
                return true;
            }
            current = self.hir.parent(def);
        }
        false
    }

    fn resolve_and_default(&mut self, ty: Ty) -> Ty {
        let resolved = self.unifier.find_deep(&mut self.tcx, ty);
        self.default_unconstrained_types(resolved)
    }

    fn report_integer_literals(&self, owner: DefId) {
        let entries = self.annotations_of(owner);
        let negated = self.negated_literal_operands(&entries);
        let array_lengths = self.array_length_exprs(owner);
        let (out_of_range, unsigned_negation) =
            self.integer_literal_violations(&entries, &negated, &array_lengths);

        for (literal, ty, span) in out_of_range {
            report_integer_literal_out_of_range(self.display_cx(), &literal, ty, span);
        }
        for (ty, span) in unsigned_negation {
            report_negative_literal_for_unsigned_type(self.display_cx(), ty, span);
        }
    }

    fn negated_literal_operands(&self, entries: &[(HirId, Ty)]) -> HashSet<HirId> {
        entries
            .iter()
            .filter_map(|&(id, _)| match self.hir.node(id) {
                Node::Expr(expr) => match expr.kind {
                    ExprKind::Unary {
                        op: UnaryOp::Neg,
                        operand,
                    } => Some(operand.into()),
                    _ => None,
                },
                _ => None,
            })
            .collect()
    }

    fn integer_literal_violations(
        &self,
        entries: &[(HirId, Ty)],
        negated: &HashSet<HirId>,
        array_lengths: &HashSet<HirId>,
    ) -> (Vec<(String, Ty, SrcSpan)>, Vec<(Ty, SrcSpan)>) {
        let mut out_of_range = Vec::new();
        let mut unsigned_negation = Vec::new();

        for &(id, ty) in entries {
            if array_lengths.contains(&id) {
                continue;
            }
            let Node::Expr(expr) = self.hir.node(id) else {
                continue;
            };
            let Some((min, max)) = integer_bounds(&self.tcx, ty) else {
                continue;
            };
            match &expr.kind {
                ExprKind::Literal(Literal::Int { value, .. }) => {
                    if let Some((literal, span)) = self.out_of_range_literal(
                        *value,
                        min,
                        max,
                        negated.contains(&id),
                        expr.span,
                    ) {
                        out_of_range.push((literal, ty, span));
                    }
                }
                ExprKind::Unary {
                    op: UnaryOp::Neg,
                    operand,
                } if min >= 0 && self.is_integer_literal((*operand).into()) => {
                    unsigned_negation.push((ty, expr.span));
                }
                _ => {}
            }
        }

        (out_of_range, unsigned_negation)
    }

    fn out_of_range_literal(
        &self,
        value: crate::ast::Symbol,
        min: i128,
        max: i128,
        negated: bool,
        span: SrcSpan,
    ) -> Option<(String, SrcSpan)> {
        let literal = self.session.resolve(value);
        let out_of_range = match literal.parse::<i128>() {
            Err(_) => true,
            Ok(value) if negated => value > -min,
            Ok(value) => value < min || value > max,
        };
        (out_of_range && !(negated && min >= 0)).then(|| (literal.to_string(), span))
    }

    fn is_integer_literal(&self, id: HirId) -> bool {
        matches!(
            self.hir.node(id),
            Node::Expr(expr) if matches!(expr.kind, ExprKind::Literal(Literal::Int { .. }))
        )
    }

    fn array_length_exprs(&self, owner: DefId) -> HashSet<HirId> {
        let mut exprs = HashSet::new();
        for (id, _) in self
            .types
            .tys_iter()
            .filter(|(id, _)| self.owner_is_lexically_inside(id.owner, owner))
        {
            let Node::Ty(ty) = self.hir.node(id) else {
                continue;
            };
            if let HirTyKind::Array { len: Some(len), .. } = ty.kind {
                self.collect_const_exprs(len, &mut exprs);
            }
        }
        exprs
    }

    fn collect_const_exprs(&self, id: ExprId, out: &mut HashSet<HirId>) {
        out.insert(id.into());
        match &self.hir.expr(id).kind {
            ExprKind::Unary { operand, .. } => self.collect_const_exprs(*operand, out),
            ExprKind::Binary { lhs, rhs, .. } => {
                self.collect_const_exprs(*lhs, out);
                self.collect_const_exprs(*rhs, out);
            }
            _ => {}
        }
    }

    fn default_unconstrained_types(&mut self, ty: Ty) -> Ty {
        visitor::fold_ty(&mut self.tcx, ty, &mut |tcx, ty| match *tcx.kind(ty) {
            TyKind::Var(InferVar::Int(_)) => Some(tcx.mk_prim(PrimTy::I32)),
            TyKind::Var(InferVar::Float(_)) => Some(tcx.mk_prim(PrimTy::F64)),
            TyKind::Var(InferVar::Any(_)) => Some(tcx.unit()),
            _ => None,
        })
    }

    pub(crate) fn unify_allowing_any(&mut self, expected: Ty, found: Ty) -> Result<(), UnifyError> {
        if let TyKind::Any(inner) = *self.tcx.kind(expected) {
            let (peeled, _layers) = self.peel_receiver(found);
            return self.unifier.unify(&self.tcx, inner, peeled);
        }
        self.unifier.unify(&self.tcx, expected, found)
    }

    pub(crate) fn coerce_unsize(
        &mut self,
        expected: Ty,
        found: Ty,
        expr_id: impl Into<HirId>,
    ) -> bool {
        let expr_id = expr_id.into();
        let Some((trait_, args, found_base)) = self.dyn_coercion(expected, found) else {
            return false;
        };

        let goal = Goal {
            self_ty: found_base,
            trait_: TraitRef { def: trait_, args },
        };
        let env = self.bounds_env(expr_id.owner);
        if !matches!(self.implements(&goal, &env), Solution::Holds) {
            return false;
        }

        self.types.record_unsize(expr_id, expected);
        true
    }

    fn dyn_coercion(&mut self, expected: Ty, found: Ty) -> Option<(DefId, Vec<Ty>, Ty)> {
        let TyKind::Ref {
            base: expected_base,
            mutability: expected_mut,
        } = self.tcx.kind(expected).clone()
        else {
            return None;
        };
        let TyKind::Dyn { trait_, args } = self.tcx.kind(expected_base).clone() else {
            return None;
        };
        let TyKind::Ref {
            base: found_base,
            mutability: found_mut,
        } = self.tcx.kind(found).clone()
        else {
            return None;
        };
        if expected_mut != found_mut || self.mentions_infer_var(found_base) {
            return None;
        }
        Some((trait_, args, found_base))
    }

    pub fn check_stmt(&mut self, id: impl Into<HirId>) {
        let id = id.into();
        let stmt = self.hir.stmt(id);
        let span = stmt.span;

        match &stmt.kind {
            StmtKind::Let {
                pat,
                ty,
                init,
                else_block,
                ..
            } => {
                let (pat, ty, init, else_block) = (*pat, *ty, *init, *else_block);
                self.check_let_stmt(pat, ty, init, else_block, span);
            }
            StmtKind::With { lends, block } => self.check_with_stmt(lends, *block),
            StmtKind::Return(expr) => self.check_return_stmt(id.owner, *expr, span),
            StmtKind::Defer(expr) | StmtKind::Expr(expr) => {
                self.ty_of(*expr);
            }
            _ => {}
        }
    }

    fn check_let_stmt(
        &mut self,
        pat: PatId,
        ty: Option<TyId>,
        init: ExprId,
        else_block: Option<BlockId>,
        span: SrcSpan,
    ) {
        self.check_binding(pat, ty, init, span);
        self.check_let_else(pat, else_block);
    }

    fn check_let_else(&mut self, pat: PatId, else_block: Option<BlockId>) {
        match (self.pat_is_irrefutable(pat), else_block) {
            (false, None) => {
                report_refutable_let_without_else(self.session, self.hir.pat(pat).span);
            }
            (true, Some(block)) => {
                report_irrefutable_let_with_else(self.session, self.hir.block(block).span);
            }
            _ => {}
        }

        if let Some(block) = else_block {
            self.check_block(block);
        }
    }

    fn check_with_stmt(&mut self, lends: &'hir [WithLend], block: BlockId) {
        for lend in lends {
            self.check_binding(lend.pat, lend.ty, lend.init, lend.span);
        }
        self.check_block(block);
    }

    fn check_return_stmt(&mut self, owner: DefId, expr: Option<ExprId>, span: SrcSpan) {
        let ret = self.return_ty(owner);

        let Some(expr) = expr else {
            let unit = self.tcx.unit();
            if let Err(err) = self.unifier.unify(&self.tcx, ret, unit) {
                report_return_mismatch(self.display_cx(), err, span);
            }
            return;
        };

        let expr_ty = self.ty_of_expecting(expr, Some(ret));
        if let Err(err) = self.unify_allowing_any(ret, expr_ty)
            && !self.coerce_unsize(ret, expr_ty, expr)
        {
            report_return_mismatch(self.display_cx(), err, span);
        }
    }

    fn check_binding(&mut self, pat: PatId, ty: Option<TyId>, init: ExprId, span: SrcSpan) {
        let declared = ty.map(|ty_id| self.lower_binding_annotation(ty_id));
        let init_ty = self.ty_of_expecting(init, declared);

        let bound = match declared {
            Some(declared) => {
                if let Err(err) = self.unifier.unify(&self.tcx, declared, init_ty)
                    && !self.coerce_unsize(declared, init_ty, init)
                {
                    report_binding_type_mismatch(self.display_cx(), err, span);
                }
                declared
            }
            None => init_ty,
        };
        self.check_pat(pat, bound, BindingMode::Value);
    }

    fn lower_binding_annotation(&mut self, ty_id: TyId) -> Ty {
        let declared = self.lower_ty(ty_id);
        let span = self.hir.ty(ty_id).span;
        self.check_not_any(declared, span);
        self.check_no_dyn(declared, span);
        declared
    }

    fn return_ty(&mut self, owner: DefId) -> Ty {
        match self.signature(owner) {
            Some((_, Some(ret))) => ret,
            _ => self.tcx.unit(),
        }
    }

    pub fn check_block(&mut self, id: impl Into<HirId>) -> Ty {
        let id = id.into();
        self.check_block_expecting(id, None)
    }

    fn check_block_expecting(&mut self, id: impl Into<HirId>, expected: Option<Ty>) -> Ty {
        let id = id.into();
        let block = self.hir.block(id);
        let tail = block.expr;

        let mut diverges = false;
        for &stmt in &block.stmts {
            self.check_stmt(stmt);
            diverges |= match self.hir.stmt(stmt).kind {
                StmtKind::Return(_) | StmtKind::Break | StmtKind::Continue => true,
                StmtKind::Expr(expr) => self.ty_of(expr) == self.tcx.never(),
                _ => false,
            };
        }

        let tail_ty = match tail {
            Some(tail) => self.ty_of_expecting(tail, expected),
            None => self.tcx.unit(),
        };

        if diverges { self.tcx.never() } else { tail_ty }
    }

    pub fn check_function(&mut self, function: DefId) {
        let function_node = self.hir.function(function);
        let (block, span) = (function_node.block, function_node.span);

        self.check_function_body(function, block, span);
        self.resolve_pending_method_calls(function);
        self.finalize_function(function);
    }

    fn check_function_body(&mut self, function: DefId, block: Option<BlockId>, span: SrcSpan) {
        let Some(block) = block else {
            return self.check_bodiless_function(function, span);
        };

        let ret = self.return_ty(function);
        let body = self.check_block_expecting(block, Some(ret));
        if let Err(err) = self.unify_allowing_any(ret, body) {
            let tail = self.hir.block(block).expr;
            let coerced = tail.is_some_and(|tail| self.coerce_unsize(ret, body, tail));
            if !coerced {
                report_body_return_mismatch(self.display_cx(), err, span);
            }
        }
    }

    fn check_bodiless_function(&mut self, function: DefId, span: SrcSpan) {
        let parent = self
            .hir
            .parent(function)
            .expect("a function is never the root module, so it always has a parent");
        if !matches!(self.hir.def(parent), OwnerNode::Module(_)) {
            return;
        }

        let declared_in_core = self
            .session
            .file_containing(span.get_begin())
            .is_some_and(|file| file.origin == FileOrigin::Core);
        let is_write_bytes = self.hir.lang_items().get(LangItem::WriteBytes) == Some(function);

        if !declared_in_core || !is_write_bytes {
            report_bodiless_function(self.session, span);
        }
    }
}

fn integer_bounds(tcx: &TyCtx, ty: Ty) -> Option<(i128, i128)> {
    let TyKind::Primitive(prim) = *tcx.kind(ty) else {
        return None;
    };
    match prim {
        PrimTy::I8 => Some((i8::MIN as i128, i8::MAX as i128)),
        PrimTy::I16 => Some((i16::MIN as i128, i16::MAX as i128)),
        PrimTy::I32 => Some((i32::MIN as i128, i32::MAX as i128)),
        PrimTy::I64 => Some((i64::MIN as i128, i64::MAX as i128)),
        PrimTy::U8 => Some((0, u8::MAX as i128)),
        PrimTy::U16 => Some((0, u16::MAX as i128)),
        PrimTy::U32 => Some((0, u32::MAX as i128)),
        PrimTy::U64 | PrimTy::Usize => Some((0, u64::MAX as i128)),
        _ => None,
    }
}

struct BodyChecker<'a, 'hir>(&'a mut Typeck<'hir>);

impl<'hir> Visitor<'hir> for BodyChecker<'_, 'hir> {
    fn hir(&self) -> &'hir Hir {
        self.0.hir
    }

    fn visit_nested_owner(&mut self, def_id: DefId) {
        visit::walk_item(self, def_id);
    }

    fn visit_function(&mut self, def_id: DefId) {
        self.0.check_function(def_id);
    }

    fn visit_struct(&mut self, _def_id: DefId) {}

    fn visit_enum(&mut self, _def_id: DefId) {}

    fn visit_closure(&mut self, def_id: DefId) {
        unreachable!("stage two reached a closure ({def_id:?}) outside the body declaring it")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ast::Mutability;
    use crate::diagnostics::Severity;
    use crate::diagnostics::display::DisplayCtx;
    use crate::nameres::PrimTy;
    use crate::testing::{
        TypeckStage, checker_through, find_return, first_extend_method, first_function,
        first_struct, first_trait, lower_to_hir, typeck_accepts as accepts,
        typeck_rejects as rejects, typeck_src_as_core,
    };
    use crate::typeck::ty::unify::UnifyError;

    fn checker_with_signatures_collected<'hir>(hir: &'hir Hir) -> Typeck<'hir> {
        checker_through(hir, TypeckStage::Collect)
    }

    #[test]
    fn return_stmt_accepts_a_value_matching_the_return_type() {
        let hir = lower_to_hir("fun f() -> i32 { return 0; }");
        let def = first_function(&hir);
        let (stmt_id, _expr_id) = find_return(&hir, def);

        let mut checker = checker_with_signatures_collected(&hir);

        crate::testing::clear_diagnostics();
        checker.check_stmt(stmt_id);
        let diagnostics = crate::testing::diagnostics();
        assert!(diagnostics.is_empty(), "{diagnostics:?}");
    }

    #[test]
    fn return_stmt_rejects_a_value_not_matching_the_return_type() {
        let hir = lower_to_hir("fun f() -> i32 { return true; }");
        let def = first_function(&hir);
        let (stmt_id, _expr_id) = find_return(&hir, def);

        let mut checker = checker_with_signatures_collected(&hir);

        crate::testing::clear_diagnostics();
        checker.check_stmt(stmt_id);
        let diagnostics = crate::testing::diagnostics();
        assert_eq!(diagnostics.len(), 1, "{diagnostics:?}");
        assert_eq!(diagnostics[0].severity, Severity::Error);
    }

    #[test]
    fn return_stmt_in_a_function_with_no_declared_return_type_rejects_a_value() {
        let hir = lower_to_hir("fun f() { return true; }");
        let def = first_function(&hir);
        let (stmt_id, _expr_id) = find_return(&hir, def);

        let mut checker = checker_with_signatures_collected(&hir);

        crate::testing::clear_diagnostics();
        checker.check_stmt(stmt_id);
        let diagnostics = crate::testing::diagnostics();
        assert_eq!(diagnostics.len(), 1, "{diagnostics:?}");
        assert_eq!(diagnostics[0].severity, Severity::Error);
    }

    #[test]
    fn a_bare_return_with_no_declared_return_type_checks() {
        accepts("fun f() { return; }");
    }

    #[test]
    fn a_bare_return_in_a_function_declaring_a_return_type_is_rejected() {
        rejects("fun f() -> i32 { return; }", "mismatched types");
    }

    #[test]
    fn two_functions_with_no_return_type_do_not_interfere() {
        let hir = lower_to_hir(
            "fun f() -> bool { return true; }
             fun g() -> i32 { return 1; }",
        );
        let mut checker = checker_with_signatures_collected(&hir);

        crate::testing::clear_diagnostics();
        checker.check_module(hir.root_id());
        let diagnostics = crate::testing::diagnostics();
        assert!(diagnostics.is_empty(), "{diagnostics:?}");
    }

    #[test]
    fn bodiless_free_function_in_a_user_file_is_rejected() {
        rejects("fun write_bytes(fd: i32) -> i64;", "no body");
    }

    #[test]
    fn bodiless_free_function_in_a_core_file_with_an_unknown_name_is_rejected() {
        let reported = typeck_src_as_core("module core::io; fun mystery_intrinsic() -> i64;");
        assert_eq!(reported.len(), 1, "{reported:?}");
        assert!(reported[0].contains("no body"), "{reported:?}");
    }

    #[test]
    fn write_bytes_declared_in_core_is_accepted() {
        let reported = typeck_src_as_core(
            "module core::io; public fun write_bytes(fd: i32, buf: &[u8]) -> i64;",
        );
        assert!(reported.is_empty(), "{reported:?}");
    }

    #[test]
    fn bodiless_trait_method_is_unaffected() {
        accepts("trait Shape { fun area(&self) -> i32; }");
    }

    fn checker_with_impls_built<'hir>(hir: &'hir Hir) -> Typeck<'hir> {
        checker_through(hir, TypeckStage::Index)
    }

    fn find_owner(hir: &Hir, from: DefId, pred: &impl Fn(&OwnerNode) -> bool) -> DefId {
        find_owner_opt(hir, from, pred)
            .unwrap_or_else(|| panic!("no item matching the predicate anywhere under {from:?}"))
    }

    fn find_owner_opt(hir: &Hir, from: DefId, pred: &impl Fn(&OwnerNode) -> bool) -> Option<DefId> {
        let module = hir.module(from);

        for &item in &module.items {
            if pred(hir.def(item)) {
                return Some(item);
            }
            if matches!(hir.def(item), OwnerNode::Module(_))
                && let Some(found) = find_owner_opt(hir, item, pred)
            {
                return Some(found);
            }
        }
        None
    }

    #[test]
    fn binary_add_on_a_struct_with_an_add_impl_resolves_through_the_solver() {
        let hir = lower_to_hir(
            "module core::ops;

             public trait Add {
                 fun add(&self, other: &Self) -> Self;
             }

             struct Foo {
                 x: i32,
             }

             extend Foo with Add {
                 fun add(&self, other: &Self) -> Self {
                     return .{ x: self.x };
                 }
             }

             fun f(a: Foo, b: Foo) -> Foo {
                 return a + b;
             }",
        );
        let f = find_owner(&hir, hir.root_id(), &|def| {
            matches!(def, OwnerNode::Function(_))
        });
        let (stmt_id, _expr_id) = find_return(&hir, f);
        let mut checker = checker_with_impls_built(&hir);

        crate::testing::clear_diagnostics();
        checker.check_stmt(stmt_id);
        let diagnostics = crate::testing::diagnostics();
        assert!(diagnostics.is_empty(), "{diagnostics:?}");
    }

    #[test]
    fn binary_add_on_a_struct_with_no_add_impl_is_rejected() {
        let hir = lower_to_hir(
            "module core::ops;

             public trait Add {
                 fun add(&self, other: &Self) -> Self;
             }

             struct Foo {
                 x: i32,
             }

             fun f(a: Foo, b: Foo) -> Foo {
                 return a + b;
             }",
        );
        let f = find_owner(&hir, hir.root_id(), &|def| {
            matches!(def, OwnerNode::Function(_))
        });
        let (stmt_id, _expr_id) = find_return(&hir, f);
        let mut checker = checker_with_impls_built(&hir);

        crate::testing::clear_diagnostics();
        checker.check_stmt(stmt_id);
        let diagnostics = crate::testing::diagnostics();
        assert_eq!(diagnostics.len(), 1, "{diagnostics:?}");
        assert!(diagnostics[0].message.contains("Add"), "{diagnostics:?}");
    }

    #[test]
    fn binary_add_on_primitives_bypasses_the_solver() {
        let hir = lower_to_hir("fun f() -> i32 { return 1 + 2; }");
        let def = first_function(&hir);
        let (stmt_id, _expr_id) = find_return(&hir, def);
        let mut checker = checker_with_impls_built(&hir);

        crate::testing::clear_diagnostics();
        checker.check_stmt(stmt_id);
        let diagnostics = crate::testing::diagnostics();
        assert!(diagnostics.is_empty(), "{diagnostics:?}");
    }

    #[test]
    fn binary_add_between_two_unresolved_int_literals_resolves_to_the_return_type() {
        let hir = lower_to_hir("fun f() -> i32 { return 1 + 2; }");
        let def = first_function(&hir);
        let (_stmt_id, expr_id) = find_return(&hir, def);
        let mut checker = checker_with_signatures_collected(&hir);

        checker.check_function(def);

        let ty = checker
            .types
            .ty(expr_id)
            .expect("checking the body records the binary expression's type");
        assert_eq!(
            *checker.tcx.kind(ty),
            TyKind::Primitive(PrimTy::I32),
            "the operator bypassed the solver and the return unified it with i32"
        );
    }

    #[test]
    fn an_operator_on_two_still_unresolved_operands_needs_an_annotation() {
        use crate::testing::typeck_rejects;

        typeck_rejects(
            "fun make<T>() -> T { return make(); }
             fun f() {
                 let a = make();
                 let b = make();
                 let c = a - b;
             }",
            "type annotations needed",
        );
    }

    #[test]
    fn a_type_read_back_after_unification_is_the_unified_type() {
        let hir = lower_to_hir("fun f() -> i32 { return 1; }");
        let def = first_function(&hir);
        let (_stmt_id, expr_id) = find_return(&hir, def);
        let mut checker = checker_with_signatures_collected(&hir);

        let recorded = checker.ty_of(expr_id);
        assert!(matches!(checker.tcx.kind(recorded), TyKind::Var(_)));

        let i32_ty = checker.tcx.mk_prim(PrimTy::I32);
        checker
            .unifier
            .unify(&checker.tcx, recorded, i32_ty)
            .expect("an int var unifies with i32");

        assert_eq!(checker.types.ty(expr_id), Some(recorded));
        assert_eq!(checker.ty_of(expr_id), i32_ty);
    }

    #[test]
    fn finalization_leaves_no_unresolved_variables_behind() {
        let hir = lower_to_hir("fun f() -> i32 { return 1; }");
        let def = first_function(&hir);
        let (_stmt_id, expr_id) = find_return(&hir, def);
        let mut checker = checker_with_signatures_collected(&hir);

        checker.check_function(def);

        let recorded = checker
            .types
            .ty(expr_id)
            .expect("checking the body records the returned expression's type");
        assert_eq!(
            *checker.tcx.kind(recorded),
            TyKind::Primitive(PrimTy::I32),
            "the return unified the literal with i32, and finalization stored that"
        );
    }

    #[test]
    fn an_unconstrained_int_literal_defaults_to_i32() {
        let hir = lower_to_hir("fun f() { let x = 5; }");
        let def = first_function(&hir);
        let mut checker = checker_with_signatures_collected(&hir);
        checker.check_function(def);

        let function = hir.function(def);
        let block = hir.block(function.block.unwrap());
        let stmt = hir.stmt(block.stmts[0]);
        let StmtKind::Let { init, .. } = stmt.kind else {
            panic!("expected a let statement");
        };

        let ty = checker
            .types
            .ty(init.into())
            .expect("finalization records the initializer's type");
        assert_eq!(*checker.tcx.kind(ty), TyKind::Primitive(PrimTy::I32));
    }

    #[test]
    fn an_unconstrained_float_literal_defaults_to_f64() {
        let hir = lower_to_hir("fun f() { let x = 5.0; }");
        let def = first_function(&hir);
        let mut checker = checker_with_signatures_collected(&hir);
        checker.check_function(def);

        let function = hir.function(def);
        let block = hir.block(function.block.unwrap());
        let stmt = hir.stmt(block.stmts[0]);
        let StmtKind::Let { init, .. } = stmt.kind else {
            panic!("expected a let statement");
        };

        let ty = checker
            .types
            .ty(init.into())
            .expect("finalization records the initializer's type");
        assert_eq!(*checker.tcx.kind(ty), TyKind::Primitive(PrimTy::F64));
    }

    #[test]
    fn bool_literal_checks_to_the_bool_primitive() {
        let hir = lower_to_hir("fun f() -> bool { return true; }");
        let def = first_function(&hir);
        let (_stmt_id, expr_id) = find_return(&hir, def);
        let mut checker = checker_with_signatures_collected(&hir);

        let ty = checker.ty_of(expr_id);
        assert_eq!(*checker.tcx.kind(ty), TyKind::Primitive(PrimTy::Bool));
        assert_eq!(checker.types.ty(expr_id), Some(ty));
    }

    #[test]
    fn char_literal_checks_to_the_char_primitive() {
        let hir = lower_to_hir("fun f() -> char { return 'a'; }");
        let def = first_function(&hir);
        let (_stmt_id, expr_id) = find_return(&hir, def);
        let mut checker = checker_with_signatures_collected(&hir);

        let ty = checker.ty_of(expr_id);
        assert_eq!(*checker.tcx.kind(ty), TyKind::Primitive(PrimTy::Char));
    }

    #[test]
    fn unsuffixed_int_literal_checks_to_an_int_inference_var() {
        let hir = lower_to_hir("fun f() -> i32 { return 0; }");
        let def = first_function(&hir);
        let (_stmt_id, expr_id) = find_return(&hir, def);
        let mut checker = checker_with_signatures_collected(&hir);

        let ty = checker.ty_of(expr_id);
        assert!(matches!(
            checker.tcx.kind(ty),
            TyKind::Var(crate::typeck::ty::InferVar::Int(_))
        ));
    }

    #[test]
    fn unsuffixed_float_literal_checks_to_a_float_inference_var() {
        let hir = lower_to_hir("fun f() -> f64 { return 0.0; }");
        let def = first_function(&hir);
        let (_stmt_id, expr_id) = find_return(&hir, def);
        let mut checker = checker_with_signatures_collected(&hir);

        let ty = checker.ty_of(expr_id);
        assert!(matches!(
            checker.tcx.kind(ty),
            TyKind::Var(crate::typeck::ty::InferVar::Float(_))
        ));
    }

    #[test]
    fn suffixed_int_literal_checks_directly_to_its_primitive() {
        let hir = lower_to_hir("fun f() -> u8 { return 5_u8; }");
        let def = first_function(&hir);
        let (_stmt_id, expr_id) = find_return(&hir, def);
        let mut checker = checker_with_signatures_collected(&hir);

        let ty = checker.ty_of(expr_id);
        assert_eq!(*checker.tcx.kind(ty), TyKind::Primitive(PrimTy::U8));
    }

    #[test]
    fn suffixed_float_literal_checks_directly_to_its_primitive() {
        let hir = lower_to_hir("fun f() -> f32 { return 3.14_f32; }");
        let def = first_function(&hir);
        let (_stmt_id, expr_id) = find_return(&hir, def);
        let mut checker = checker_with_signatures_collected(&hir);

        let ty = checker.ty_of(expr_id);
        assert_eq!(*checker.tcx.kind(ty), TyKind::Primitive(PrimTy::F32));
    }

    #[test]
    fn whole_number_with_a_float_suffix_checks_to_the_float_primitive() {
        let hir = lower_to_hir("fun f() -> f64 { return 5_f64; }");
        let def = first_function(&hir);
        let (_stmt_id, expr_id) = find_return(&hir, def);
        let mut checker = checker_with_signatures_collected(&hir);

        let ty = checker.ty_of(expr_id);
        assert_eq!(*checker.tcx.kind(ty), TyKind::Primitive(PrimTy::F64));
    }

    #[test]
    fn digit_separators_do_not_interfere_with_a_suffix() {
        let hir = lower_to_hir("fun f() -> i64 { return 1_000_000_i64; }");
        let def = first_function(&hir);
        let (_stmt_id, expr_id) = find_return(&hir, def);
        let mut checker = checker_with_signatures_collected(&hir);

        let ty = checker.ty_of(expr_id);
        assert_eq!(*checker.tcx.kind(ty), TyKind::Primitive(PrimTy::I64));
    }

    #[test]
    fn suffix_disagreeing_with_the_expected_type_is_a_mismatch() {
        use crate::testing::typeck_rejects;

        typeck_rejects("fun f() -> i32 { return 5_u8; }", "found");
    }

    #[test]
    fn fractional_literal_with_an_integer_suffix_is_rejected() {
        use crate::testing::typeck_rejects;

        typeck_rejects("fun f() -> i32 { return 3.14_i32; }", "fractional part");
    }

    #[test]
    fn unknown_literal_suffix_is_rejected() {
        use crate::testing::typeck_rejects;

        typeck_rejects(
            "fun f() -> i32 { return 1_bogus; }",
            "invalid literal suffix",
        );
    }

    #[test]
    fn tuple_expr_checks_to_a_tuple_of_its_elements_types() {
        let hir = lower_to_hir("fun f() -> (bool, char) { return (true, 'a'); }");
        let def = first_function(&hir);
        let (_stmt_id, expr_id) = find_return(&hir, def);
        let mut checker = checker_with_signatures_collected(&hir);

        let ty = checker.ty_of(expr_id);
        let TyKind::Tuple(elems) = checker.tcx.kind(ty) else {
            panic!("a tuple expression checks to TyKind::Tuple, got {ty:?}");
        };
        let elem_kinds: Vec<TyKind> = elems
            .iter()
            .map(|&elem| checker.tcx.kind(elem).clone())
            .collect();
        assert_eq!(
            elem_kinds,
            vec![
                TyKind::Primitive(PrimTy::Bool),
                TyKind::Primitive(PrimTy::Char),
            ]
        );
    }

    #[test]
    fn path_to_a_parameter_checks_to_the_parameters_type() {
        let hir = lower_to_hir("fun f(x: i32) -> i32 { return x; }");
        let def = first_function(&hir);
        let (_stmt_id, expr_id) = find_return(&hir, def);
        let mut checker = checker_with_signatures_collected(&hir);

        let ty = checker.ty_of(expr_id);
        assert_eq!(*checker.tcx.kind(ty), TyKind::Primitive(PrimTy::I32));
    }

    #[test]
    fn path_to_a_function_checks_to_its_signature() {
        let hir = lower_to_hir(
            "fun g() -> bool { return true; }
             fun f() -> bool { return g; }",
        );

        let module = hir.root();
        let g_def = first_function(&hir);
        let f_def = module
            .items
            .iter()
            .copied()
            .find(|&item| matches!(hir.def(item), OwnerNode::Function(_)) && item != g_def)
            .expect("fixture declares a second top-level function");
        let (_stmt_id, expr_id) = find_return(&hir, f_def);
        let mut checker = checker_with_signatures_collected(&hir);

        let ty = checker.ty_of(expr_id);
        let TyKind::Fun { params, ret } = checker.tcx.kind(ty) else {
            panic!("a function path checks to TyKind::Fun, got {ty:?}");
        };
        assert!(params.is_empty());
        assert_eq!(
            ret.map(|ret| checker.tcx.kind(ret).clone()),
            Some(TyKind::Primitive(PrimTy::Bool))
        );
    }

    #[test]
    fn path_to_self_checks_to_the_self_parameters_type() {
        let hir = lower_to_hir(
            "struct S {}
             extend S { fun m(&self) -> i32 { return self; } }",
        );
        let method_def = first_extend_method(&hir);
        let (_stmt_id, expr_id) = find_return(&hir, method_def);
        let mut checker = checker_with_signatures_collected(&hir);

        let ty = checker.ty_of(expr_id);
        assert_eq!(checker.display_cx().show(ty).to_string(), "&S");
    }

    #[test]
    fn primitive_displays_as_its_keyword() {
        let hir = lower_to_hir("fun f() {}");
        let mut checker = checker_with_signatures_collected(&hir);
        let ty = checker.tcx.mk_prim(PrimTy::I32);
        assert_eq!(checker.display_cx().show(ty).to_string(), "i32");
    }

    #[test]
    fn any_ty_var_displays_as_underscore() {
        let hir = lower_to_hir("fun f() {}");
        let mut checker = checker_with_signatures_collected(&hir);
        let ty = checker.tcx.next_infer_var();
        assert_eq!(checker.display_cx().show(ty).to_string(), "_");
    }

    #[test]
    fn int_var_displays_as_integer_placeholder() {
        let hir = lower_to_hir("fun f() {}");
        let mut checker = checker_with_signatures_collected(&hir);
        let ty = checker.tcx.next_int_var();
        assert_eq!(checker.display_cx().show(ty).to_string(), "{integer}");
    }

    #[test]
    fn float_var_displays_as_float_placeholder() {
        let hir = lower_to_hir("fun f() {}");
        let mut checker = checker_with_signatures_collected(&hir);
        let ty = checker.tcx.next_float_var();
        assert_eq!(checker.display_cx().show(ty).to_string(), "{float}");
    }

    #[test]
    fn never_displays_as_bang() {
        let hir = lower_to_hir("fun f() {}");
        let mut checker = checker_with_signatures_collected(&hir);
        let never = checker.tcx.never();
        assert_eq!(checker.display_cx().show(never).to_string(), "!");
    }

    #[test]
    fn unit_displays_as_empty_parens() {
        let hir = lower_to_hir("fun f() {}");
        let mut checker = checker_with_signatures_collected(&hir);
        let unit = checker.tcx.unit();
        assert_eq!(checker.display_cx().show(unit).to_string(), "()");
    }

    #[test]
    fn unit_is_a_singleton_distinct_from_the_empty_tuple() {
        let hir = lower_to_hir("fun f() {}");
        let mut checker = checker_with_signatures_collected(&hir);
        let unit = checker.tcx.unit();
        let empty_tuple = checker.tcx.mk_tuple(vec![]);
        assert_ne!(unit, empty_tuple);
        assert_eq!(
            checker.tcx.unit(),
            unit,
            "unit() always returns the same handle"
        );
    }

    #[test]
    fn error_displays_as_placeholder() {
        let hir = lower_to_hir("fun f() {}");
        let mut checker = checker_with_signatures_collected(&hir);
        let error = checker.tcx.error();
        assert_eq!(checker.display_cx().show(error).to_string(), "{error}");
    }

    #[test]
    fn immutable_ref_displays_with_ampersand() {
        let hir = lower_to_hir("fun f() {}");
        let mut checker = checker_with_signatures_collected(&hir);
        let bool_ty = checker.tcx.mk_prim(PrimTy::Bool);
        let ty = checker.tcx.mk_ref(bool_ty, Mutability::Immutable);
        assert_eq!(checker.display_cx().show(ty).to_string(), "&bool");
    }

    #[test]
    fn mutable_ref_displays_with_ampersand_mut() {
        let hir = lower_to_hir("fun f() {}");
        let mut checker = checker_with_signatures_collected(&hir);
        let bool_ty = checker.tcx.mk_prim(PrimTy::Bool);
        let ty = checker.tcx.mk_ref(bool_ty, Mutability::Mutable);
        assert_eq!(checker.display_cx().show(ty).to_string(), "&mut bool");
    }

    #[test]
    fn any_ty_displays_with_any_keyword() {
        let hir = lower_to_hir("fun f() {}");
        let mut checker = checker_with_signatures_collected(&hir);
        let bool_ty = checker.tcx.mk_prim(PrimTy::Bool);
        let ty = checker.tcx.mk_any(bool_ty);
        assert_eq!(checker.display_cx().show(ty).to_string(), "any bool");
    }

    #[test]
    fn empty_tuple_displays_as_empty_parens() {
        let hir = lower_to_hir("fun f() {}");
        let mut checker = checker_with_signatures_collected(&hir);
        let ty = checker.tcx.mk_tuple(vec![]);
        assert_eq!(checker.display_cx().show(ty).to_string(), "()");
    }

    #[test]
    fn one_element_tuple_displays_with_a_trailing_comma() {
        let hir = lower_to_hir("fun f() {}");
        let mut checker = checker_with_signatures_collected(&hir);
        let bool_ty = checker.tcx.mk_prim(PrimTy::Bool);
        let ty = checker.tcx.mk_tuple(vec![bool_ty]);
        assert_eq!(checker.display_cx().show(ty).to_string(), "(bool,)");
    }

    #[test]
    fn multi_element_tuple_displays_comma_separated() {
        let hir = lower_to_hir("fun f() {}");
        let mut checker = checker_with_signatures_collected(&hir);
        let bool_ty = checker.tcx.mk_prim(PrimTy::Bool);
        let char_ty = checker.tcx.mk_prim(PrimTy::Char);
        let ty = checker.tcx.mk_tuple(vec![bool_ty, char_ty]);
        assert_eq!(checker.display_cx().show(ty).to_string(), "(bool, char)");
    }

    #[test]
    fn array_displays_with_brackets_and_a_placeholder_length() {
        let hir = lower_to_hir("fun f() {}");
        let mut checker = checker_with_signatures_collected(&hir);
        let i32_ty = checker.tcx.mk_prim(PrimTy::I32);
        let ty = checker.tcx.mk_array(i32_ty, None);
        assert_eq!(checker.display_cx().show(ty).to_string(), "[i32; _]");
    }

    #[test]
    fn fun_with_no_params_or_ret_displays_as_bare_fun() {
        let hir = lower_to_hir("fun f() {}");
        let mut checker = checker_with_signatures_collected(&hir);
        let ty = checker.tcx.mk_fun(vec![], None);
        assert_eq!(checker.display_cx().show(ty).to_string(), "fun()");
    }

    #[test]
    fn fun_with_params_and_ret_displays_with_arrow() {
        let hir = lower_to_hir("fun f() {}");
        let mut checker = checker_with_signatures_collected(&hir);
        let i32_ty = checker.tcx.mk_prim(PrimTy::I32);
        let bool_ty = checker.tcx.mk_prim(PrimTy::Bool);
        let ty = checker.tcx.mk_fun(vec![i32_ty, i32_ty], Some(bool_ty));
        assert_eq!(
            checker.display_cx().show(ty).to_string(),
            "fun(i32, i32) -> bool"
        );
    }

    #[test]
    fn generic_displays_with_its_declared_name() {
        let hir = lower_to_hir("struct Wrap<T> { inner: T }");
        let def = first_struct(&hir);
        let s = hir.struct_(def);
        let generic_id = s.generics[0];
        let mut checker = checker_with_signatures_collected(&hir);
        let ty = checker.tcx.mk_generic(generic_id);
        assert_eq!(checker.display_cx().show(ty).to_string(), "T");
    }

    #[test]
    fn adt_displays_with_its_name_and_generic_args() {
        let hir = lower_to_hir("struct Wrap<T> { inner: T }");
        let def = first_struct(&hir);
        let checker = checker_with_signatures_collected(&hir);

        let ty = checker
            .types
            .ty_of_def(def)
            .expect("collect_struct records the struct's own type under its owner node");
        assert_eq!(checker.display_cx().show(ty).to_string(), "Wrap<T>");
    }

    #[test]
    fn adt_with_no_generics_displays_with_just_its_name() {
        let hir = lower_to_hir("struct Unit {}");
        let def = first_struct(&hir);
        let checker = checker_with_signatures_collected(&hir);

        let ty = checker
            .types
            .ty_of_def(def)
            .expect("collect_struct records the struct's own type under its owner node");
        assert_eq!(checker.display_cx().show(ty).to_string(), "Unit");
    }

    #[test]
    fn self_param_displays_as_self() {
        let hir = lower_to_hir("trait Greet { fun hello(); }");
        let def = first_trait(&hir);
        let checker = checker_with_signatures_collected(&hir);

        let ty = checker
            .types
            .ty_of_def(def)
            .expect("collect_trait records the trait's own Self type under its owner node");
        assert_eq!(checker.display_cx().show(ty).to_string(), "Self");
    }

    #[test]
    fn dyn_displays_with_dyn_keyword_and_trait_name() {
        let hir = lower_to_hir("trait Greet { fun hello(); }");
        let def = first_trait(&hir);
        let mut checker = checker_with_signatures_collected(&hir);
        let ty = checker.tcx.mk_dyn(def, vec![]);
        assert_eq!(checker.display_cx().show(ty).to_string(), "dyn Greet");
    }

    #[test]
    fn a_mismatch_names_both_types_as_the_user_wrote_them() {
        let hir = lower_to_hir("fun f() {}");
        let mut tcx = TyCtx::new();
        let (expected, found) = (tcx.mk_prim(PrimTy::I32), tcx.mk_prim(PrimTy::Bool));
        let cx = DisplayCtx::new(crate::testing::session(), &hir, &tcx);

        assert_eq!(
            cx.show(UnifyError::Mismatch { expected, found })
                .to_string(),
            "mismatched types: expected `i32`, found `bool`"
        );
    }

    #[test]
    fn an_int_var_mismatch_says_an_integer_type_was_expected() {
        let hir = lower_to_hir("fun f() {}");
        let mut tcx = TyCtx::new();
        let var = tcx.next_int_var();
        let found = tcx.mk_prim(PrimTy::Bool);
        let cx = DisplayCtx::new(crate::testing::session(), &hir, &tcx);

        assert_eq!(
            cx.show(UnifyError::ExpectedInteger { var, found })
                .to_string(),
            "mismatched types: expected an integer type, found `bool`"
        );
    }

    #[test]
    fn a_functions_trailing_expression_is_checked_against_its_return_type() {
        rejects("fun f() -> i32 { true }", "mismatched types");
    }

    #[test]
    fn an_empty_body_is_checked_against_a_declared_return_type() {
        rejects("fun f() -> i32 {}", "mismatched types");
    }

    #[test]
    fn a_partial_return_does_not_guarantee_every_path_produces_the_declared_type() {
        rejects(
            "fun f(c: bool) -> i32 { if c { return 1; } }",
            "mismatched types",
        );
    }

    #[test]
    fn a_function_body_ending_in_an_if_else_that_always_returns_checks() {
        accepts("fun f(c: bool) -> i32 { if c { return 1; } else { return 2; } }");
    }

    #[test]
    fn a_statement_position_if_else_that_always_returns_still_lets_later_code_check() {
        accepts(
            "fun f(c: bool) -> i32 {
                 if c { return 1; } else { return 2; };
                 return 0;
             }",
        );
    }

    #[test]
    fn every_integer_primitive_round_trips_through_a_function_signature() {
        for name in ["i8", "i16", "i32", "i64", "u8", "u16", "u32", "u64"] {
            accepts(&format!("fun f(x: {name}) -> {name} {{ return x; }}"));
        }
    }

    #[test]
    fn every_float_primitive_round_trips_through_a_function_signature() {
        for name in ["f32", "f64"] {
            accepts(&format!("fun f(x: {name}) -> {name} {{ return x; }}"));
        }
    }

    #[test]
    fn a_negative_int_literal_still_checks_as_an_integer() {
        accepts("fun f() -> i32 { return -1; }");
    }

    #[test]
    fn a_negative_float_literal_still_checks_as_a_float() {
        accepts("fun f() -> f64 { return -1.5; }");
    }

    #[test]
    fn an_int_literal_and_a_float_literal_do_not_unify() {
        rejects("fun f() { let x = 1 + 1.0; }", "mismatched types");
    }

    #[test]
    fn a_bool_and_a_char_do_not_unify() {
        rejects(
            "fun f() -> bool { return 'a' == true; }",
            "mismatched types",
        );
    }

    #[test]
    fn and_and_or_accept_two_bools() {
        accepts("fun f(a: bool, b: bool) -> bool { return a && b; }");
        accepts("fun f(a: bool, b: bool) -> bool { return a || b; }");
    }

    #[test]
    fn and_rejects_operands_of_different_types_exactly_once() {
        rejects("fun f() { let x = 1 && true; }", "mismatched types");
    }

    #[test]
    fn and_rejects_two_operands_of_the_same_non_bool_type() {
        rejects("fun f() { let x = 1 && 2; }", "expected an integer type");
    }

    #[test]
    fn a_struct_implementing_every_operator_trait_supports_every_operator() {
        accepts(
            "module core::ops;

             public trait Add { fun add(&self, other: &Self) -> Self; }
             public trait Sub { fun sub(&self, other: &Self) -> Self; }
             public trait Mul { fun mul(&self, other: &Self) -> Self; }
             public trait Div { fun div(&self, other: &Self) -> Self; }
             public trait Rem { fun rem(&self, other: &Self) -> Self; }
             public trait Neg { fun neg(&self) -> Self; }
             public trait Not { fun not(&self) -> Self; }
             public trait Eq { fun eq(&self, other: &Self) -> bool; }
             public trait Comparable { fun compare(&self, other: &Self) -> i32; }

             struct N { v: i32 }

             extend N with Add { fun add(&self, other: &Self) -> Self { return .{ v: self.v }; } }
             extend N with Sub { fun sub(&self, other: &Self) -> Self { return .{ v: self.v }; } }
             extend N with Mul { fun mul(&self, other: &Self) -> Self { return .{ v: self.v }; } }
             extend N with Div { fun div(&self, other: &Self) -> Self { return .{ v: self.v }; } }
             extend N with Rem { fun rem(&self, other: &Self) -> Self { return .{ v: self.v }; } }
             extend N with Neg { fun neg(&self) -> Self { return .{ v: self.v }; } }
             extend N with Not { fun not(&self) -> Self { return .{ v: self.v }; } }
             extend N with Eq { fun eq(&self, other: &Self) -> bool { return true; } }
             extend N with Comparable { fun compare(&self, other: &Self) -> i32 { return 0; } }

             fun f(a: N, b: N) -> N {
                 let sum = a + b;
                 let diff = a - b;
                 let prod = a * b;
                 let quot = a / b;
                 let rem = a % b;
                 let negated = -a;
                 let inverted = !a;
                 let is_eq = a == b;
                 let is_ne = a != b;
                 let lt = a < b;
                 let le = a <= b;
                 let gt = a > b;
                 let ge = a >= b;
                 return sum;
             }",
        );
    }

    #[test]
    fn sub_neg_and_comparable_each_report_their_own_missing_trait() {
        rejects(
            "module core::ops;
             public trait Sub { fun sub(&self, other: &Self) -> Self; }
             struct N { v: i32 }
             fun f(a: N, b: N) -> N { return a - b; }",
            "does not implement `Sub`",
        );
        rejects(
            "module core::ops;
             public trait Neg { fun neg(&self) -> Self; }
             struct N { v: i32 }
             fun f(a: N) -> N { return -a; }",
            "does not implement `Neg`",
        );
        rejects(
            "module core::ops;
             public trait Comparable { fun compare(&self, other: &Self) -> i32; }
             struct N { v: i32 }
             fun f(a: N, b: N) -> bool { return a < b; }",
            "does not implement `Comparable`",
        );
    }

    #[test]
    fn an_undefaulted_int_literal_still_gets_the_arithmetic_operators_for_free() {
        accepts("fun f() { let a = 1; let b = 2; let _ = a + b; }");
    }

    #[test]
    fn a_concretely_typed_primitive_needs_a_real_impl_for_arithmetic() {
        rejects(
            "module core::ops;
             public trait Add { fun add(&self, other: &Self) -> Self; }
             fun f(x: i32) -> i32 { return x + 1; }",
            "does not implement `Add`",
        );
    }

    #[test]
    fn a_concretely_typed_primitive_with_a_real_impl_gets_arithmetic() {
        accepts(
            "module core::ops;
             public trait Add { fun add(&self, other: &Self) -> Self; }
             public trait Copy { fun copy(&self) -> Self; }
             extend i32 with Add { fun add(&self, other: &Self) -> Self { return *self + *other; } }
             extend i32 with Copy { fun copy(&self) -> Self { return *self; } }
             fun f(x: i32) -> i32 { return x + 1; }",
        );
    }

    #[test]
    fn a_let_may_rebind_a_name_at_a_different_type() {
        accepts(
            "fun f() -> bool {
                 let x = 1;
                 let x = true;
                 return x;
             }",
        );
    }

    #[test]
    fn a_block_scoped_shadow_does_not_leak_out() {
        accepts(
            "fun f() -> i32 {
                 let x = 1;
                 { let x = true; }
                 return x;
             }",
        );
        rejects(
            "fun f() -> bool {
                 let x = 1;
                 { let x = true; }
                 return x;
             }",
            "mismatched types",
        );
    }

    #[test]
    fn a_directly_recursive_function_checks() {
        accepts("fun fact(n: i32) -> i32 { return fact(n); }");
    }

    #[test]
    fn two_mutually_recursive_functions_check_regardless_of_order() {
        accepts(
            "fun is_even(n: i32) -> bool { return is_odd(n); }
             fun is_odd(n: i32) -> bool { return is_even(n); }",
        );
    }

    #[test]
    fn a_plain_binding_pattern_needs_no_else() {
        accepts("fun f() { let x = 1; }");
    }

    #[test]
    fn a_wildcard_pattern_needs_no_else() {
        accepts("fun f() { let _ = 1; }");
    }

    #[test]
    fn a_tuple_of_irrefutable_patterns_needs_no_else() {
        accepts("fun f(p: (i32, bool)) { let (x, y) = p; }");
    }

    #[test]
    fn a_refutable_variant_pattern_without_an_else_is_rejected() {
        rejects(
            "enum Option<T> { some: T, none }
             fun f(o: Option<i32>) { let .some(x) = o; }",
            "with no `else`",
        );
    }

    #[test]
    fn a_refutable_variant_pattern_with_an_else_is_accepted() {
        accepts(
            "enum Option<T> { some: T, none }
             fun f(o: Option<i32>) -> i32 {
                 let .some(x) = o else { return 0; };
                 return x;
             }",
        );
    }

    #[test]
    fn a_tuple_containing_a_refutable_element_needs_an_else() {
        rejects(
            "enum Option<T> { some: T, none }
             fun f(p: (i32, Option<i32>)) { let (x, .some(y)) = p; }",
            "with no `else`",
        );
    }

    #[test]
    fn a_single_variant_enums_pattern_is_irrefutable() {
        accepts(
            "enum Only { one: i32 }
             fun f(o: Only) -> i32 { let .one(x) = o; return x; }",
        );
        rejects(
            "enum Only { one: i32 }
             fun f(o: Only) -> i32 { let .one(x) = o else { return 0; }; return x; }",
            "irrefutable pattern",
        );
    }

    #[test]
    fn an_irrefutable_binding_pattern_with_an_else_is_rejected() {
        rejects(
            "fun f() -> i32 { let x = 1 else { return 0; }; return x; }",
            "irrefutable pattern",
        );
    }

    #[test]
    fn an_irrefutable_tuple_pattern_with_an_else_is_rejected() {
        rejects(
            "fun f(p: (i32, bool)) -> i32 { let (x, y) = p else { return 0; }; return x; }",
            "irrefutable pattern",
        );
    }

    #[test]
    fn a_with_lends_pattern_is_never_checked_for_refutability() {
        accepts("fun f(x: i32) { with y = &x { let _ = y; } }");
    }

    #[test]
    fn a_closure_parameter_constrained_by_its_call_site_does_not_stay_a_variable() {
        let (hir, tcx, types) = crate::testing::typecheck_only(
            "fun conv<A, B>(x: A, f: fun(A) -> B) -> B { return f(x); }\n\
             fun main() { let n: i32 = conv(true, |x| 9); }",
        );
        let _ = hir;
        for (id, ty) in types.tys_iter() {
            assert!(
                !matches!(tcx.kind(ty), TyKind::Var(_)),
                "{id:?} kept an unresolved variable: {ty:?}"
            );
        }
    }

    #[test]
    fn an_out_of_range_integer_literal_is_rejected() {
        for src in [
            "fun f() -> u8 { let x: u8 = 300; return x; }",
            "fun f() -> i8 { let x: i8 = 200; return x; }",
            "fun f() -> i32 { let x: i32 = 4294967296; return x; }",
        ] {
            let reported = crate::testing::typeck_src(src);
            assert!(
                !reported.is_empty(),
                "expected a literal-out-of-range error for {src:?}, got {reported:?}"
            );
        }
    }

    #[test]
    fn a_negative_literal_for_an_unsigned_type_is_rejected() {
        let reported = crate::testing::typeck_src("fun f() -> u8 { let x: u8 = -1; return x; }");
        assert!(
            !reported.is_empty(),
            "`-1` has type `u8` only by truncation; expected a rejection, got {reported:?}"
        );
    }

    #[test]
    fn a_match_missing_a_variant_payload_case_is_rejected() {
        let reported = crate::testing::typeck_src(
            "enum Opt { some: bool, none }\n\
             fun f(o: Opt) -> i32 { return match o { .some(true) => 1, .none => 3, }; }",
        );
        assert!(
            !reported.is_empty(),
            "`.some(false)` is uncovered; expected a non-exhaustive error, got {reported:?}"
        );
    }

    #[test]
    fn a_refutable_let_pattern_is_rejected() {
        let reported = crate::testing::typeck_src(
            "enum Only { one: bool }\n\
             fun f(o: Only) { let .one(true) = o; }",
        );
        assert!(
            !reported.is_empty(),
            "`.one(true)` fails for `.one(false)`; expected a refutability error, got {reported:?}"
        );
    }

    #[test]
    fn assigning_to_a_temporary_is_rejected() {
        let reported = crate::testing::typeck_src(
            "struct P { x: i32 }\n\
             fun make() -> P { return P { x: 1 }; }\n\
             fun f() { make().x = 2; }",
        );
        assert!(
            !reported.is_empty(),
            "a call result is not an assignable place; expected a rejection, got {reported:?}"
        );
    }

    #[test]
    fn a_mutable_method_cannot_be_called_through_a_shared_reference() {
        let reported = crate::testing::typeck_src(
            "struct S { x: i32 }\n\
             extend S { fun set(&mut self, v: i32) { self.x = v; } }\n\
             fun f(r: &S) { let m: &mut &S = &mut r; m.set(1); }",
        );
        assert!(
            !reported.is_empty(),
            "reborrowing `**m` mutably through `&S` must be rejected, got {reported:?}"
        );
    }

    #[test]
    fn a_parenthesized_type_is_not_a_one_element_tuple() {
        accepts("fun f(x: (i32)) -> i32 { return x; }");
    }

    #[test]
    fn a_char_can_be_cast_to_usize() {
        accepts("fun f(c: char) -> usize { return c as usize; }");
    }
}
