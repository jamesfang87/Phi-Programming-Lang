use std::collections::{BTreeMap, HashMap, HashSet};

use crate::ast::{BinaryOp, Literal, Mutability, SelfMode, UnaryOp, Visibility};
use crate::diagnostics::typeck::display::DisplayCx;
use crate::diagnostics::typeck::pat::{
    report_irrefutable_let_with_else, report_refutable_let_without_else,
};
use crate::diagnostics::typeck::traits::solve::report_operator_trait_missing;
use crate::diagnostics::typeck::{
    report_any_outside_signature, report_binary_operand_mismatch, report_binding_type_mismatch,
    report_bodiless_function, report_body_return_mismatch, report_int_suffix_on_float_literal,
    report_logic_op_needs_bool_operands, report_operand_has_unknown_type, report_reference_field,
    report_return_mismatch, report_unknown_literal_suffix,
};
use crate::driver::source::{FileOrigin, SrcMap, SrcSpan};
use crate::hir::visit::{self, Visitor};
use crate::hir::{
    DefId, ExprKind, Hir, HirId, Local, Node, OwnerNode, PatKind, Res, StmtKind,
    TyKind as HirTyKind, VariantPayload,
};
use crate::langitems::LangItem;
use crate::nameres::PrimTy;
use crate::nameres::symbol_table::prim_ty;
use crate::typeck::expr::DerefContext;
use crate::typeck::results::TypeResolutions;
use crate::typeck::traits::bounds::Obligation;
use crate::typeck::traits::index::ExtendIndex;
use crate::typeck::traits::solve::{Query, Solution};
use crate::typeck::ty::{Ty, TyKind, TyVar};
use crate::typeck::tyctx::TyCtx;
use crate::typeck::unify::{Unifier, UnifyError, is_float, is_integer};

pub mod cast;
pub mod expr;
pub mod fold;
pub mod lower_ty;
pub mod pat;
pub mod results;
pub mod traits;
pub mod ty;
pub mod tyctx;
pub mod unify;

pub struct Typeck<'hir> {
    hir: &'hir Hir,

    // Typeck
    tcx: TyCtx,
    types: TypeResolutions,
    unifier: Unifier,

    // Trait solving
    extends: ExtendIndex,
    trait_bound_obligations: BTreeMap<DefId, Vec<Obligation>>,

    self_tys: HashMap<DefId, Ty>,

    /// The definitions whose `Self` is being computed right now, which
    /// are used for cycle detection (such as extend Wrap<Self>)
    computing_self_tys: HashSet<DefId>,
}

impl<'hir> Typeck<'hir> {
    pub fn new(hir: &'hir Hir) -> Self {
        Typeck {
            hir,
            tcx: TyCtx::new(),
            types: TypeResolutions::new(),
            unifier: Unifier::new(),
            extends: ExtendIndex::new(),
            trait_bound_obligations: BTreeMap::new(),
            self_tys: HashMap::new(),
            computing_self_tys: HashSet::new(),
        }
    }

    pub fn collect_module(&mut self, module_id: DefId) {
        Collect(self).visit_module(module_id);
    }

    pub fn collect_function(&mut self, function: DefId) {
        let hir: &'hir Hir = self.hir;
        let function_node = hir.function(function);
        let (generics, self_param, params, ret) = (
            &function_node.generics,
            function_node.self_param,
            &function_node.params,
            function_node.ret,
        );

        self.collect_generics(generics);

        let mut param_tys = Vec::with_capacity(params.len() + usize::from(self_param.is_some()));
        if let Some(id) = self_param {
            param_tys.push(self.collect_self_param(id));
        }

        for &id in params {
            let param = hir.param(id);

            let ty = self.lower_ty(param.ty);
            self.types.record(id, ty);
            self.check_no_dyn(ty, hir.ty(param.ty).span);
            param_tys.push(ty);
        }

        let ret = ret.map(|ret| {
            let ret_span = hir.ty(ret).span;
            let ret = self.lower_ty(ret);
            self.check_no_dyn(ret, ret_span);
            ret
        });

        let sig = self.tcx.mk_fun(param_tys, ret);
        self.types.record_def(function, sig);
    }

    fn collect_self_param(&mut self, id: HirId) -> Ty {
        let self_param = self.hir.self_param(id);
        let (mode, span) = (self_param.mode, self_param.span);

        let self_ty = self.self_ty(id.owner, span);
        let ty = match mode {
            SelfMode::Immutable => self.tcx.mk_ref(self_ty, Mutability::Immutable),
            SelfMode::Mutable => self.tcx.mk_ref(self_ty, Mutability::Mutable),
            SelfMode::Move => self_ty,
            SelfMode::Any => self.tcx.mk_any(self_ty),
        };
        self.types.record(id, ty);
        ty
    }

    pub fn collect_struct(&mut self, r#struct: DefId) {
        let hir: &'hir Hir = self.hir;
        let struct_node = hir.struct_(r#struct);
        let (generics, fields, span) =
            (&struct_node.generics, &struct_node.fields, struct_node.span);

        self.collect_generics(generics);
        let self_ty = self.self_ty(r#struct, span);
        self.types.record_def(r#struct, self_ty);

        self.collect_fields(fields);
    }

    pub fn collect_enum(&mut self, r#enum: DefId) {
        let hir: &'hir Hir = self.hir;
        let enum_node = hir.enum_(r#enum);
        let (generics, variants, span) = (&enum_node.generics, &enum_node.variants, enum_node.span);

        self.collect_generics(generics);
        let self_ty = self.self_ty(r#enum, span);
        self.types.record_def(r#enum, self_ty);

        for &id in variants {
            let variant = hir.variant(id);

            match &variant.payload {
                VariantPayload::Unit => {}
                VariantPayload::Type(ty_id) => {
                    let payload_span = hir.ty(*ty_id).span;
                    let ty = self.lower_ty(*ty_id);
                    self.types.record(id, ty);
                    self.check_not_a_reference(ty, payload_span);
                    self.check_not_any(ty, payload_span);
                    self.check_no_dyn(ty, payload_span);
                }
                VariantPayload::Record(fields) => self.collect_fields(fields),
            }
        }
    }

    pub fn collect_trait(&mut self, r#trait: DefId) {
        let hir: &'hir Hir = self.hir;
        let trait_node = hir.trait_(r#trait);
        let (generics, span) = (&trait_node.generics, trait_node.span);

        self.collect_generics(generics);
        let self_ty = self.self_ty(r#trait, span);
        self.types.record_def(r#trait, self_ty);
    }

    pub fn collect_extend(&mut self, extend: DefId) {
        let hir: &'hir Hir = self.hir;
        let extend_node = hir.extend(extend);
        let (extend_generics, self_ty_id, trait_generics, span) = (
            &extend_node.extend_generics,
            extend_node.self_ty,
            &extend_node.trait_generics,
            extend_node.span,
        );

        self.collect_generics(extend_generics);
        match &self.hir.ty(self_ty_id).kind {
            HirTyKind::Path { args, .. } => {
                let args = args.clone();
                self.lower_tys(&args);
            }
            _ => {
                self.lower_ty(self_ty_id);
            }
        }
        self.lower_tys(trait_generics);

        let self_ty = self.self_ty(extend, span);
        self.types.record_def(extend, self_ty);
    }

    fn collect_generics(&mut self, generics: &[HirId]) {
        for &id in generics {
            let ty = self.tcx.mk_generic(id);
            self.types.record(id, ty);
        }
    }

    fn collect_fields(&mut self, fields: &[HirId]) {
        for &id in fields {
            let field = self.hir.field(id);
            let field_span = field.span;

            let ty = self.lower_ty(field.ty);
            self.types.record(id, ty);
            self.check_not_a_reference(ty, field_span);
            self.check_not_any(ty, field_span);
            self.check_no_dyn(ty, field_span);
        }
    }

    fn check_not_a_reference(&mut self, ty: Ty, span: SrcSpan) {
        if self.tcx.contains_ref(ty) {
            report_reference_field(self.display_cx(), ty, span);
        }
    }

    fn check_not_any(&mut self, ty: Ty, span: SrcSpan) {
        if self.tcx.contains_any(ty) {
            report_any_outside_signature(self.display_cx(), ty, span);
        }
    }

    //-------------------------------------------------------------------------

    pub fn check_module(&mut self, module: DefId) {
        Check(self).visit_module(module);
    }

    fn ty_of(&mut self, id: HirId) -> Ty {
        self.ty_of_expecting(id, None)
    }

    fn ty_of_expecting(&mut self, id: HirId, expected: Option<Ty>) -> Ty {
        if let Some(ty) = self.types.ty(id) {
            return self.unifier.find_deep(&mut self.tcx, ty);
        }

        debug_assert!(
            matches!(self.hir.node(id), Node::Expr(_)),
            "{} node {id:?} was asked for its type before whatever records one ran",
            self.hir.node(id).kind_name()
        );

        let ty = self.check_expr(id, expected);
        self.types.record(id, ty);
        self.unifier.find_deep(&mut self.tcx, ty)
    }

    fn ty_of_as_place(&mut self, id: HirId) -> Ty {
        self.ty_of_as_place_expecting(id, None)
    }

    fn ty_of_as_place_expecting(&mut self, id: HirId, expected: Option<Ty>) -> Ty {
        if let Some(ty) = self.types.ty(id) {
            return self.unifier.find_deep(&mut self.tcx, ty);
        }
        let expr = self.hir.expr(id);
        if let ExprKind::Unary {
            op: UnaryOp::Deref,
            operand,
        } = expr.kind
        {
            let span = expr.span;
            let ty = self.check_deref_as(id, operand, span, DerefContext::Place);
            self.types.record(id, ty);
            return self.unifier.find_deep(&mut self.tcx, ty);
        }
        self.ty_of_expecting(id, expected)
    }

    fn writeback(&mut self, owner: DefId) {
        let entries: Vec<(HirId, Ty)> = self
            .types
            .tys_iter()
            .filter(|(id, _)| id.owner == owner)
            .collect();

        for (id, ty) in entries {
            let resolved = self.unifier.find_deep(&mut self.tcx, ty);
            let defaulted = self.default_unconstrained_types(resolved);
            self.types.record(id, defaulted);
        }

        let call_entries: Vec<(HirId, DefId, Vec<Ty>)> = self
            .types
            .calls_iter()
            .filter(|(id, _)| id.owner == owner)
            .map(|(id, call)| (id, call.def, call.args.clone()))
            .collect();
        for (id, def, args) in call_entries {
            let defaulted = args
                .iter()
                .map(|&arg| {
                    let resolved = self.unifier.find_deep(&mut self.tcx, arg);
                    self.default_unconstrained_types(resolved)
                })
                .collect();
            self.types.record_call(id, def, defaulted);
        }
    }

    /// Defaults every unconstrained `TyVar::Int`/`TyVar::Float` still inside `ty` to `i32`/`f64`.
    fn default_unconstrained_types(&mut self, ty: Ty) -> Ty {
        fold::fold_ty(&mut self.tcx, ty, &mut |tcx, ty| match *tcx.kind(ty) {
            TyKind::Var(TyVar::Int(_)) => Some(tcx.mk_prim(PrimTy::I32)),
            TyKind::Var(TyVar::Float(_)) => Some(tcx.mk_prim(PrimTy::F64)),
            _ => None,
        })
    }

    /// Unifies `found` against `expected`, allowing `any T` to unify with `T`, `&T`, or
    /// `&mut T`.
    fn unify_allowing_any(&mut self, expected: Ty, found: Ty) -> Result<(), UnifyError> {
        if let TyKind::Any(inner) = *self.tcx.kind(expected) {
            let (peeled, _layers) = self.peel_receiver(found);
            return self.unifier.unify(&self.tcx, inner, peeled);
        }
        self.unifier.unify(&self.tcx, expected, found)
    }

    #[must_use]
    fn check_expr(&mut self, id: HirId, expected: Option<Ty>) -> Ty {
        let expr = self.hir.expr(id);

        match &expr.kind {
            ExprKind::Literal(lit) => self.check_literal(lit, expr.span),
            ExprKind::Tuple(elems) => {
                let tys = elems.iter().map(|&elem| self.ty_of(elem)).collect();
                self.tcx.mk_tuple(tys)
            }
            ExprKind::Path(path) => match path.res {
                Res::Local(Local::Param(local) | Local::Variable(local)) => self.ty_of(local),
                Res::Local(Local::SelfParam(self_param)) => self.ty_of(self_param),
                Res::Function(def) => self.ty_of(def.owner_id()),
                Res::Err => self.tcx.error(),
                Res::Type(_) | Res::Module(_) | Res::SelfTy(_) => unreachable!(
                    "name resolution never resolves a value-position path to a type, a \
                         module, or Self"
                ),
            },
            ExprKind::Unary {
                op: UnaryOp::Deref,
                operand,
            } => self.check_deref(id, *operand, expr.span),
            ExprKind::Unary { op, operand } => {
                let operand_ty = self.ty_of(*operand);
                let resolved = self.unifier.find_deep(&mut self.tcx, operand_ty);

                let item = match op {
                    UnaryOp::Neg => LangItem::Neg,
                    UnaryOp::Not => LangItem::Not,
                    UnaryOp::Deref => unreachable!("handled by the arm above"),
                };

                if self.is_undefaulted_numeric_var(resolved)
                    || self.implements_operator(item, resolved, id.owner, expr.span)
                {
                    resolved
                } else {
                    self.tcx.error()
                }
            }
            ExprKind::Binary { op, lhs, rhs } => {
                let (lhs, rhs) = (self.ty_of(*lhs), self.ty_of(*rhs));
                if let Err(error) = self.unifier.unify(&self.tcx, lhs, rhs) {
                    report_binary_operand_mismatch(self.display_cx(), error, lhs, rhs, expr.span);
                    return self.tcx.error();
                }
                let resolved = self.unifier.find_deep(&mut self.tcx, lhs);
                self.check_operator(*op, resolved, id.owner, expr.span)
            }
            ExprKind::Assign { lhs, rhs } => self.check_assign(*lhs, *rhs, expr.span),
            ExprKind::AssignOp { op, lhs, rhs } => self.check_assign_op(*op, *lhs, *rhs, expr.span),
            ExprKind::Borrow {
                mutability,
                operand,
            } => self.check_borrow(*mutability, *operand, expected),
            ExprKind::Call { callee, args } => self.check_call(id, *callee, args, expr.span),
            ExprKind::Access { base, member, args } => self.check_access(id, *base, *member, args),
            ExprKind::Index { base, index } => self.check_index(id, *base, *index),
            ExprKind::Ctor { path, payload } => {
                self.check_ctor(id, path.as_ref(), payload, expected)
            }
            ExprKind::Variant { variant, payload } => {
                self.check_variant_expr(*variant, payload, expected, expr.span)
            }
            ExprKind::Range { lo, hi, .. } => self.check_range(*lo, *hi, expr.span),
            ExprKind::Try(operand) => self.check_try(id, *operand),
            ExprKind::If {
                cond,
                then_block,
                else_block,
            } => self.check_if(*cond, *then_block, *else_block, expected, expr.span),
            ExprKind::Match { scrutinee, arms } => {
                self.check_match(*scrutinee, arms, expected, expr.span)
            }
            ExprKind::Loop { block, .. } => {
                self.check_block(*block);
                self.tcx.unit()
            }
            ExprKind::Spawn(block) | ExprKind::Concurrent(block) => self.check_block(*block),
            ExprKind::Block(block_id) => self.check_block_expecting(*block_id, expected),
            ExprKind::Closure(def) => self.check_closure(*def, expected),
            ExprKind::Cast { expr: operand, ty } => self.check_cast(*operand, *ty, expr.span),
            ExprKind::New(operand) => self.check_new(*operand),
            ExprKind::NewArray { elem, count } => self.check_new_array(*elem, *count),
            ExprKind::Assert { cond, msg } => self.check_assert(*cond, *msg),
            ExprKind::Panic { msg } | ExprKind::Unreachable { msg } => {
                self.check_panic_message(*msg)
            }
            ExprKind::Error => self.tcx.error(),
        }
    }

    fn check_operator(&mut self, op: BinaryOp, operand: Ty, owner: DefId, span: SrcSpan) -> Ty {
        let operand = self.peel_any(operand);
        let bool_ty = self.tcx.mk_prim(PrimTy::Bool);

        let (item, produced) = match op {
            BinaryOp::Add => (LangItem::Add, operand),
            BinaryOp::Sub => (LangItem::Sub, operand),
            BinaryOp::Mul => (LangItem::Mul, operand),
            BinaryOp::Div => (LangItem::Div, operand),
            BinaryOp::Rem => (LangItem::Rem, operand),
            BinaryOp::Eq | BinaryOp::Ne => (LangItem::Eq, bool_ty),
            BinaryOp::Lt | BinaryOp::Le | BinaryOp::Gt | BinaryOp::Ge => {
                (LangItem::Comparable, bool_ty)
            }
            BinaryOp::And | BinaryOp::Or => {
                if let Err(error) = self.unifier.unify(&self.tcx, operand, bool_ty) {
                    report_logic_op_needs_bool_operands(self.display_cx(), error, operand, span);
                }
                return bool_ty;
            }
        };

        if self.is_undefaulted_numeric_var(operand)
            || self.implements_operator(item, operand, owner, span)
        {
            produced
        } else {
            self.tcx.error()
        }
    }

    fn implements_operator(
        &mut self,
        item: LangItem,
        self_ty: Ty,
        owner: DefId,
        span: SrcSpan,
    ) -> bool {
        if matches!(self.tcx.kind(self_ty), TyKind::Var(TyVar::Any(_))) {
            report_operand_has_unknown_type(span);
            return false;
        }
        let Some(def) = self.hir.lang_items().get(item) else {
            return false;
        };

        let goal = Query::new(self_ty, def);
        let env = self.bounds_env(owner);
        match self.implements(&goal, &env) {
            Solution::Holds => true,
            Solution::DoesNotHold => {
                let name = crate::diagnostics::typeck::display::def_name(self.hir, def);
                report_operator_trait_missing(self.display_cx(), self_ty, name, span);
                false
            }
            Solution::Ambiguous | Solution::Error => false,
        }
    }

    fn is_undefaulted_numeric_var(&self, ty: Ty) -> bool {
        matches!(
            self.tcx.kind(ty),
            TyKind::Var(TyVar::Int(_) | TyVar::Float(_))
        )
    }

    /// Strips every `any` layer off `ty`
    fn peel_any(&self, mut ty: Ty) -> Ty {
        while let TyKind::Any(base) = *self.tcx.kind(ty) {
            ty = base;
        }
        ty
    }

    pub(crate) fn check_literal(&mut self, lit: &Literal, span: SrcSpan) -> Ty {
        match lit {
            Literal::Bool(_) => self.tcx.mk_prim(PrimTy::Bool),
            Literal::Char(_) => self.tcx.mk_prim(PrimTy::Char),
            Literal::Int { suffix, .. } => match suffix {
                None => self.tcx.next_int_var(),
                Some(suffix) => match prim_ty(*suffix) {
                    Some(prim) if is_integer(prim) || is_float(prim) => self.tcx.mk_prim(prim),
                    _ => {
                        report_unknown_literal_suffix(*suffix, span);
                        self.tcx.error()
                    }
                },
            },
            Literal::Float { suffix, .. } => match suffix {
                None => self.tcx.next_float_var(),
                Some(suffix) => match prim_ty(*suffix) {
                    Some(prim) if is_float(prim) => self.tcx.mk_prim(prim),
                    Some(prim) if is_integer(prim) => {
                        report_int_suffix_on_float_literal(*suffix, span);
                        self.tcx.error()
                    }
                    _ => {
                        report_unknown_literal_suffix(*suffix, span);
                        self.tcx.error()
                    }
                },
            },
            Literal::Str(_) => self.tcx.mk_prim(PrimTy::Str),
        }
    }

    pub fn check_stmt(&mut self, id: HirId) {
        let stmt = self.hir.stmt(id);

        match &stmt.kind {
            StmtKind::Let {
                pat,
                ty,
                init,
                else_block,
                ..
            } => {
                let (pat, ty, init, else_block) = (*pat, *ty, *init, *else_block);
                self.check_binding(pat, ty, init, stmt.span);

                match (self.pat_is_irrefutable(pat), else_block) {
                    (false, None) => {
                        report_refutable_let_without_else(self.hir.pat(pat).span);
                    }
                    (true, Some(block)) => {
                        report_irrefutable_let_with_else(self.hir.block(block).span);
                    }
                    _ => {}
                }

                if let Some(block) = else_block {
                    self.check_block(block);
                }
            }
            StmtKind::With { lends, block } => {
                let lends: Vec<(HirId, Option<HirId>, HirId, SrcSpan)> = lends
                    .iter()
                    .map(|lend| (lend.pat, lend.ty, lend.init, lend.span))
                    .collect();
                for (pat, ty, init, span) in lends {
                    self.check_binding(pat, ty, init, span);
                }
                self.check_block(*block);
            }
            StmtKind::Return(Some(expr)) => {
                let expr = *expr;
                let ret = self.return_ty(id.owner);

                let expr_ty = self.ty_of_expecting(expr, Some(ret));
                if let Err(err) = self.unify_allowing_any(ret, expr_ty) {
                    report_return_mismatch(self.display_cx(), err, stmt.span);
                }
            }
            StmtKind::Return(None) => {
                let ret = self.return_ty(id.owner);
                let unit = self.tcx.unit();
                if let Err(err) = self.unifier.unify(&self.tcx, ret, unit) {
                    report_return_mismatch(self.display_cx(), err, stmt.span);
                }
            }
            StmtKind::Defer(expr) | StmtKind::Expr(expr) => {
                self.ty_of(*expr);
            }
            _ => {}
        }
    }

    fn check_binding(&mut self, pat: HirId, ty: Option<HirId>, init: HirId, span: SrcSpan) {
        let declared = ty.map(|ty_id| {
            let declared = self.lower_ty(ty_id);
            let span = self.hir.ty(ty_id).span;
            self.check_not_any(declared, span);
            self.check_no_dyn(declared, span);
            declared
        });
        let init_ty = self.ty_of_expecting(init, declared);

        let bound = match declared {
            Some(declared) => {
                if let Err(err) = self.unifier.unify(&self.tcx, declared, init_ty) {
                    report_binding_type_mismatch(self.display_cx(), err, span);
                }
                declared
            }
            None => init_ty,
        };
        self.check_pat(pat, bound);
    }

    fn pat_is_irrefutable(&mut self, pat_id: HirId) -> bool {
        let pat = self.hir.pat(pat_id);
        match &pat.kind {
            PatKind::Wildcard | PatKind::Binding { .. } => true,
            PatKind::Literal(_) => false,
            PatKind::Variant { .. } => {
                let ty = self
                    .types
                    .ty(pat_id)
                    .map(|ty| self.unifier.find_deep(&mut self.tcx, ty))
                    .unwrap_or_else(|| self.tcx.error());
                match self.tcx.kind(ty) {
                    TyKind::Error | TyKind::Var(_) => true,
                    TyKind::Adt { def, .. } => match self.hir.def(*def) {
                        OwnerNode::Enum(enum_) => enum_.variants.len() == 1,
                        _ => false,
                    },
                    _ => false,
                }
            }
            PatKind::Tuple(elems) => elems.iter().all(|&elem| self.pat_is_irrefutable(elem)),
            PatKind::Error => true,
        }
    }

    pub(crate) fn signature(&mut self, def: DefId) -> Option<(Vec<Ty>, Option<Ty>)> {
        let sig = self.ty_of(def.owner_id());
        match self.tcx.kind(sig) {
            TyKind::Fun { params, ret } => Some((params.clone(), *ret)),
            _ => None,
        }
    }

    fn return_ty(&mut self, owner: DefId) -> Ty {
        match self.signature(owner) {
            Some((_, Some(ret))) => ret,
            _ => self.tcx.unit(),
        }
    }

    fn display_cx(&self) -> DisplayCx<'_> {
        DisplayCx::new(self.hir, &self.tcx)
    }

    fn is_visible_from(&self, owner_module: DefId, from: DefId, visibility: Visibility) -> bool {
        match visibility {
            Visibility::Public => true,
            Visibility::Private => {
                let mut current = Some(self.hir.module_of(from));
                while let Some(module) = current {
                    if module == owner_module {
                        return true;
                    }
                    current = self.hir.parent(module);
                }
                false
            }
        }
    }

    pub fn check_block(&mut self, id: HirId) -> Ty {
        self.check_block_expecting(id, None)
    }

    fn check_block_expecting(&mut self, id: HirId, expected: Option<Ty>) -> Ty {
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

        match function_node.block {
            Some(block) => {
                let ret = self.return_ty(function);
                let body = self.check_block_expecting(block, Some(ret));
                if let Err(err) = self.unify_allowing_any(ret, body) {
                    report_body_return_mismatch(self.display_cx(), err, function_node.span);
                }
            }
            None => self.check_bodiless_function(function, function_node.span),
        }
        self.writeback(function);
    }

    fn check_bodiless_function(&mut self, function: DefId, span: SrcSpan) {
        let parent = self
            .hir
            .parent(function)
            .expect("a function is never the root module, so it always has a parent");
        if !matches!(self.hir.def(parent), OwnerNode::Module(_)) {
            return;
        }

        let declared_in_core = SrcMap::file_containing(span.get_begin())
            .is_some_and(|file| file.origin == FileOrigin::Core);
        let is_write_bytes = self.hir.lang_items().get(LangItem::WriteBytes) == Some(function);

        if !declared_in_core || !is_write_bytes {
            report_bodiless_function(span);
        }
    }
}

struct Collect<'a, 'hir>(&'a mut Typeck<'hir>);

impl<'hir> Visitor<'hir> for Collect<'_, 'hir> {
    fn hir(&self) -> &'hir Hir {
        self.0.hir
    }

    fn visit_nested_owner(&mut self, def_id: DefId) {
        visit::walk_item(self, def_id);
    }

    fn visit_function(&mut self, def_id: DefId) {
        self.0.collect_function(def_id);
    }

    fn visit_struct(&mut self, def_id: DefId) {
        self.0.collect_struct(def_id);
    }

    fn visit_enum(&mut self, def_id: DefId) {
        self.0.collect_enum(def_id);
    }

    fn visit_trait(&mut self, def_id: DefId) {
        self.0.collect_trait(def_id);
        visit::walk_trait(self, def_id);
    }

    fn visit_extend(&mut self, def_id: DefId) {
        self.0.collect_extend(def_id);
        visit::walk_extend(self, def_id);
    }

    fn visit_closure(&mut self, def_id: DefId) {
        unreachable!("stage one reached a closure ({def_id:?}), which owns no signature to collect")
    }
}

struct Check<'a, 'hir>(&'a mut Typeck<'hir>);

impl<'hir> Visitor<'hir> for Check<'_, 'hir> {
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

pub struct TypeckOutput {
    pub tcx: TyCtx,
    pub types: TypeResolutions,
}

pub fn check(hir: &Hir) -> TypeckOutput {
    let mut checker = Typeck::new(hir);
    checker.collect_module(hir.root_id());
    checker.build_extend_index();
    checker.check_coherence();
    checker.check_trait_members();
    checker.check_declared_bounds();
    checker.check_extend_headers();
    checker.check_module(hir.root_id());
    checker.select_obligations();
    TypeckOutput {
        tcx: checker.tcx,
        types: checker.types,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::diagnostics::{DiagCtx, Severity};
    use crate::nameres::PrimTy;
    use crate::testing::{
        Stage, checker_through, find_return, first_extend_method, first_function, first_struct,
        first_trait, resolve_src, typeck_accepts as accepts, typeck_rejects as rejects,
        typeck_src_as_core,
    };
    use crate::typeck::unify::UnifyError;

    fn checker_with_signatures_collected<'hir>(hir: &'hir Hir) -> Typeck<'hir> {
        checker_through(hir, Stage::Collect)
    }

    #[test]
    fn return_stmt_accepts_a_value_matching_the_return_type() {
        let hir = resolve_src("fun f() -> i32 { return 0; }");
        let def = first_function(&hir);
        let (stmt_id, _expr_id) = find_return(&hir, def);

        let mut checker = checker_with_signatures_collected(&hir);

        DiagCtx::clear();
        checker.check_stmt(stmt_id);
        let diagnostics = DiagCtx::diagnostics();
        assert!(diagnostics.is_empty(), "{diagnostics:?}");
    }

    #[test]
    fn return_stmt_rejects_a_value_not_matching_the_return_type() {
        let hir = resolve_src("fun f() -> i32 { return true; }");
        let def = first_function(&hir);
        let (stmt_id, _expr_id) = find_return(&hir, def);

        let mut checker = checker_with_signatures_collected(&hir);

        DiagCtx::clear();
        checker.check_stmt(stmt_id);
        let diagnostics = DiagCtx::diagnostics();
        assert_eq!(diagnostics.len(), 1, "{diagnostics:?}");
        assert_eq!(diagnostics[0].severity, Severity::Error);
    }

    #[test]
    fn return_stmt_in_a_function_with_no_declared_return_type_rejects_a_value() {
        let hir = resolve_src("fun f() { return true; }");
        let def = first_function(&hir);
        let (stmt_id, _expr_id) = find_return(&hir, def);

        let mut checker = checker_with_signatures_collected(&hir);

        DiagCtx::clear();
        checker.check_stmt(stmt_id);
        let diagnostics = DiagCtx::diagnostics();
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
        let hir = resolve_src(
            "fun f() -> bool { return true; }
             fun g() -> i32 { return 1; }",
        );
        let mut checker = checker_with_signatures_collected(&hir);

        DiagCtx::clear();
        checker.check_module(hir.root_id());
        let diagnostics = DiagCtx::diagnostics();
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

    // -----------------------------------------------------------------
    // check_expr
    // -----------------------------------------------------------------

    fn checker_with_impls_built<'hir>(hir: &'hir Hir) -> Typeck<'hir> {
        checker_through(hir, Stage::Index)
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
        let hir = resolve_src(
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

        DiagCtx::clear();
        checker.check_stmt(stmt_id);
        let diagnostics = DiagCtx::diagnostics();
        assert!(diagnostics.is_empty(), "{diagnostics:?}");
    }

    #[test]
    fn binary_add_on_a_struct_with_no_add_impl_is_rejected() {
        let hir = resolve_src(
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

        DiagCtx::clear();
        checker.check_stmt(stmt_id);
        let diagnostics = DiagCtx::diagnostics();
        assert_eq!(diagnostics.len(), 1, "{diagnostics:?}");
        assert!(diagnostics[0].message.contains("Add"), "{diagnostics:?}");
    }

    #[test]
    fn binary_add_on_primitives_bypasses_the_solver() {
        let hir = resolve_src("fun f() -> i32 { return 1 + 2; }");
        let def = first_function(&hir);
        let (stmt_id, _expr_id) = find_return(&hir, def);
        let mut checker = checker_with_impls_built(&hir);

        DiagCtx::clear();
        checker.check_stmt(stmt_id);
        let diagnostics = DiagCtx::diagnostics();
        assert!(diagnostics.is_empty(), "{diagnostics:?}");
    }

    #[test]
    fn binary_add_between_two_unresolved_int_literals_resolves_to_the_return_type() {
        let hir = resolve_src("fun f() -> i32 { return 1 + 2; }");
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
        let hir = resolve_src("fun f() -> i32 { return 1; }");
        let def = first_function(&hir);
        let (_stmt_id, expr_id) = find_return(&hir, def);
        let mut checker = checker_with_signatures_collected(&hir);

        // An unsuffixed literal starts out as an integer inference variable.
        let recorded = checker.ty_of(expr_id);
        assert!(matches!(checker.tcx.kind(recorded), TyKind::Var(_)));

        let i32_ty = checker.tcx.mk_prim(PrimTy::I32);
        checker
            .unifier
            .unify(&checker.tcx, recorded, i32_ty)
            .expect("an int var unifies with i32");

        // The table still holds the variable, but nothing reads it directly.
        assert_eq!(checker.types.ty(expr_id), Some(recorded));
        assert_eq!(checker.ty_of(expr_id), i32_ty);
    }

    #[test]
    fn writeback_leaves_no_unresolved_variables_behind() {
        let hir = resolve_src("fun f() -> i32 { return 1; }");
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
            "the return unified the literal with i32, and writeback stored that"
        );
    }

    #[test]
    fn an_unconstrained_int_literal_defaults_to_i32() {
        let hir = resolve_src("fun f() { let x = 5; }");
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
            .ty(init)
            .expect("writeback records the initializer's type");
        assert_eq!(*checker.tcx.kind(ty), TyKind::Primitive(PrimTy::I32));
    }

    /// The float counterpart of the test above.
    #[test]
    fn an_unconstrained_float_literal_defaults_to_f64() {
        let hir = resolve_src("fun f() { let x = 5.0; }");
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
            .ty(init)
            .expect("writeback records the initializer's type");
        assert_eq!(*checker.tcx.kind(ty), TyKind::Primitive(PrimTy::F64));
    }

    #[test]
    fn bool_literal_checks_to_the_bool_primitive() {
        let hir = resolve_src("fun f() -> bool { return true; }");
        let def = first_function(&hir);
        let (_stmt_id, expr_id) = find_return(&hir, def);
        let mut checker = checker_with_signatures_collected(&hir);

        let ty = checker.ty_of(expr_id);
        assert_eq!(*checker.tcx.kind(ty), TyKind::Primitive(PrimTy::Bool));
        assert_eq!(checker.types.ty(expr_id), Some(ty));
    }

    #[test]
    fn char_literal_checks_to_the_char_primitive() {
        let hir = resolve_src("fun f() -> char { return 'a'; }");
        let def = first_function(&hir);
        let (_stmt_id, expr_id) = find_return(&hir, def);
        let mut checker = checker_with_signatures_collected(&hir);

        let ty = checker.ty_of(expr_id);
        assert_eq!(*checker.tcx.kind(ty), TyKind::Primitive(PrimTy::Char));
    }

    #[test]
    fn unsuffixed_int_literal_checks_to_an_int_inference_var() {
        let hir = resolve_src("fun f() -> i32 { return 0; }");
        let def = first_function(&hir);
        let (_stmt_id, expr_id) = find_return(&hir, def);
        let mut checker = checker_with_signatures_collected(&hir);

        let ty = checker.ty_of(expr_id);
        assert!(matches!(
            checker.tcx.kind(ty),
            TyKind::Var(crate::typeck::ty::TyVar::Int(_))
        ));
    }

    #[test]
    fn unsuffixed_float_literal_checks_to_a_float_inference_var() {
        let hir = resolve_src("fun f() -> f64 { return 0.0; }");
        let def = first_function(&hir);
        let (_stmt_id, expr_id) = find_return(&hir, def);
        let mut checker = checker_with_signatures_collected(&hir);

        let ty = checker.ty_of(expr_id);
        assert!(matches!(
            checker.tcx.kind(ty),
            TyKind::Var(crate::typeck::ty::TyVar::Float(_))
        ));
    }

    #[test]
    fn suffixed_int_literal_checks_directly_to_its_primitive() {
        let hir = resolve_src("fun f() -> u8 { return 5_u8; }");
        let def = first_function(&hir);
        let (_stmt_id, expr_id) = find_return(&hir, def);
        let mut checker = checker_with_signatures_collected(&hir);

        let ty = checker.ty_of(expr_id);
        assert_eq!(*checker.tcx.kind(ty), TyKind::Primitive(PrimTy::U8));
    }

    #[test]
    fn suffixed_float_literal_checks_directly_to_its_primitive() {
        let hir = resolve_src("fun f() -> f32 { return 3.14_f32; }");
        let def = first_function(&hir);
        let (_stmt_id, expr_id) = find_return(&hir, def);
        let mut checker = checker_with_signatures_collected(&hir);

        let ty = checker.ty_of(expr_id);
        assert_eq!(*checker.tcx.kind(ty), TyKind::Primitive(PrimTy::F32));
    }

    #[test]
    fn whole_number_with_a_float_suffix_checks_to_the_float_primitive() {
        let hir = resolve_src("fun f() -> f64 { return 5_f64; }");
        let def = first_function(&hir);
        let (_stmt_id, expr_id) = find_return(&hir, def);
        let mut checker = checker_with_signatures_collected(&hir);

        let ty = checker.ty_of(expr_id);
        assert_eq!(*checker.tcx.kind(ty), TyKind::Primitive(PrimTy::F64));
    }

    #[test]
    fn digit_separators_do_not_interfere_with_a_suffix() {
        let hir = resolve_src("fun f() -> i64 { return 1_000_000_i64; }");
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
        let hir = resolve_src("fun f() -> (bool, char) { return (true, 'a'); }");
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

    // -----------------------------------------------------------------
    // check_expr: Path
    // -----------------------------------------------------------------

    #[test]
    fn path_to_a_parameter_checks_to_the_parameters_type() {
        let hir = resolve_src("fun f(x: i32) -> i32 { return x; }");
        let def = first_function(&hir);
        let (_stmt_id, expr_id) = find_return(&hir, def);
        let mut checker = checker_with_signatures_collected(&hir);

        let ty = checker.ty_of(expr_id);
        assert_eq!(*checker.tcx.kind(ty), TyKind::Primitive(PrimTy::I32));
    }

    #[test]
    fn path_to_a_function_checks_to_its_signature() {
        let hir = resolve_src(
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
        let hir = resolve_src(
            "struct S {}
             extend S { fun m(&self) -> i32 { return self; } }",
        );
        let method_def = first_extend_method(&hir);
        let (_stmt_id, expr_id) = find_return(&hir, method_def);
        let mut checker = checker_with_signatures_collected(&hir);

        let ty = checker.ty_of(expr_id);
        assert_eq!(checker.display_cx().show(ty).to_string(), "&S");
    }

    // -----------------------------------------------------------------
    // Diagnostic rendering
    // -----------------------------------------------------------------

    #[test]
    fn primitive_displays_as_its_keyword() {
        let hir = resolve_src("fun f() {}");
        let mut checker = checker_with_signatures_collected(&hir);
        let ty = checker.tcx.mk_prim(PrimTy::I32);
        assert_eq!(checker.display_cx().show(ty).to_string(), "i32");
    }

    #[test]
    fn any_ty_var_displays_as_underscore() {
        let hir = resolve_src("fun f() {}");
        let mut checker = checker_with_signatures_collected(&hir);
        let ty = checker.tcx.next_ty_var();
        assert_eq!(checker.display_cx().show(ty).to_string(), "_");
    }

    #[test]
    fn int_var_displays_as_integer_placeholder() {
        let hir = resolve_src("fun f() {}");
        let mut checker = checker_with_signatures_collected(&hir);
        let ty = checker.tcx.next_int_var();
        assert_eq!(checker.display_cx().show(ty).to_string(), "{integer}");
    }

    #[test]
    fn float_var_displays_as_float_placeholder() {
        let hir = resolve_src("fun f() {}");
        let mut checker = checker_with_signatures_collected(&hir);
        let ty = checker.tcx.next_float_var();
        assert_eq!(checker.display_cx().show(ty).to_string(), "{float}");
    }

    #[test]
    fn never_displays_as_bang() {
        let hir = resolve_src("fun f() {}");
        let mut checker = checker_with_signatures_collected(&hir);
        let never = checker.tcx.never();
        assert_eq!(checker.display_cx().show(never).to_string(), "!");
    }

    #[test]
    fn unit_displays_as_empty_parens() {
        let hir = resolve_src("fun f() {}");
        let mut checker = checker_with_signatures_collected(&hir);
        let unit = checker.tcx.unit();
        assert_eq!(checker.display_cx().show(unit).to_string(), "()");
    }

    #[test]
    fn unit_is_a_singleton_distinct_from_the_empty_tuple() {
        let hir = resolve_src("fun f() {}");
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
        let hir = resolve_src("fun f() {}");
        let mut checker = checker_with_signatures_collected(&hir);
        let error = checker.tcx.error();
        assert_eq!(checker.display_cx().show(error).to_string(), "{error}");
    }

    #[test]
    fn immutable_ref_displays_with_ampersand() {
        let hir = resolve_src("fun f() {}");
        let mut checker = checker_with_signatures_collected(&hir);
        let bool_ty = checker.tcx.mk_prim(PrimTy::Bool);
        let ty = checker.tcx.mk_ref(bool_ty, Mutability::Immutable);
        assert_eq!(checker.display_cx().show(ty).to_string(), "&bool");
    }

    #[test]
    fn mutable_ref_displays_with_ampersand_mut() {
        let hir = resolve_src("fun f() {}");
        let mut checker = checker_with_signatures_collected(&hir);
        let bool_ty = checker.tcx.mk_prim(PrimTy::Bool);
        let ty = checker.tcx.mk_ref(bool_ty, Mutability::Mutable);
        assert_eq!(checker.display_cx().show(ty).to_string(), "&mut bool");
    }

    #[test]
    fn any_ty_displays_with_any_keyword() {
        let hir = resolve_src("fun f() {}");
        let mut checker = checker_with_signatures_collected(&hir);
        let bool_ty = checker.tcx.mk_prim(PrimTy::Bool);
        let ty = checker.tcx.mk_any(bool_ty);
        assert_eq!(checker.display_cx().show(ty).to_string(), "any bool");
    }

    #[test]
    fn empty_tuple_displays_as_empty_parens() {
        let hir = resolve_src("fun f() {}");
        let mut checker = checker_with_signatures_collected(&hir);
        let ty = checker.tcx.mk_tuple(vec![]);
        assert_eq!(checker.display_cx().show(ty).to_string(), "()");
    }

    #[test]
    fn one_element_tuple_displays_with_a_trailing_comma() {
        let hir = resolve_src("fun f() {}");
        let mut checker = checker_with_signatures_collected(&hir);
        let bool_ty = checker.tcx.mk_prim(PrimTy::Bool);
        let ty = checker.tcx.mk_tuple(vec![bool_ty]);
        assert_eq!(checker.display_cx().show(ty).to_string(), "(bool,)");
    }

    #[test]
    fn multi_element_tuple_displays_comma_separated() {
        let hir = resolve_src("fun f() {}");
        let mut checker = checker_with_signatures_collected(&hir);
        let bool_ty = checker.tcx.mk_prim(PrimTy::Bool);
        let char_ty = checker.tcx.mk_prim(PrimTy::Char);
        let ty = checker.tcx.mk_tuple(vec![bool_ty, char_ty]);
        assert_eq!(checker.display_cx().show(ty).to_string(), "(bool, char)");
    }

    #[test]
    fn array_displays_with_brackets_and_a_placeholder_length() {
        let hir = resolve_src("fun f() {}");
        let mut checker = checker_with_signatures_collected(&hir);
        let i32_ty = checker.tcx.mk_prim(PrimTy::I32);
        let ty = checker.tcx.mk_array(i32_ty, None);
        assert_eq!(checker.display_cx().show(ty).to_string(), "[i32; _]");
    }

    #[test]
    fn fun_with_no_params_or_ret_displays_as_bare_fun() {
        let hir = resolve_src("fun f() {}");
        let mut checker = checker_with_signatures_collected(&hir);
        let ty = checker.tcx.mk_fun(vec![], None);
        assert_eq!(checker.display_cx().show(ty).to_string(), "fun()");
    }

    #[test]
    fn fun_with_params_and_ret_displays_with_arrow() {
        let hir = resolve_src("fun f() {}");
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
        let hir = resolve_src("struct Wrap<T> { inner: T }");
        let def = first_struct(&hir);
        let s = hir.struct_(def);
        let generic_id = s.generics[0];
        let mut checker = checker_with_signatures_collected(&hir);
        let ty = checker.tcx.mk_generic(generic_id);
        assert_eq!(checker.display_cx().show(ty).to_string(), "T");
    }

    #[test]
    fn adt_displays_with_its_name_and_generic_args() {
        let hir = resolve_src("struct Wrap<T> { inner: T }");
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
        let hir = resolve_src("struct Unit {}");
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
        let hir = resolve_src("trait Greet { fun hello(); }");
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
        let hir = resolve_src("trait Greet { fun hello(); }");
        let def = first_trait(&hir);
        let mut checker = checker_with_signatures_collected(&hir);
        let ty = checker.tcx.mk_dyn(def, vec![]);
        assert_eq!(checker.display_cx().show(ty).to_string(), "dyn Greet");
    }

    #[test]
    fn a_mismatch_names_both_types_as_the_user_wrote_them() {
        let hir = resolve_src("fun f() {}");
        let mut tcx = TyCtx::new();
        let (expected, found) = (tcx.mk_prim(PrimTy::I32), tcx.mk_prim(PrimTy::Bool));
        let cx = DisplayCx::new(&hir, &tcx);

        assert_eq!(
            cx.show(UnifyError::Mismatch { expected, found })
                .to_string(),
            "mismatched types: expected `i32`, found `bool`"
        );
    }

    #[test]
    fn an_int_var_mismatch_says_an_integer_type_was_expected() {
        let hir = resolve_src("fun f() {}");
        let mut tcx = TyCtx::new();
        let var = tcx.next_int_var();
        let found = tcx.mk_prim(PrimTy::Bool);
        let cx = DisplayCx::new(&hir, &tcx);

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

    // -----------------------------------------------------------------
    // Primitives
    // -----------------------------------------------------------------

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

    // -----------------------------------------------------------------
    // Shadowing and recursion
    // -----------------------------------------------------------------

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
    fn a_struct_field_that_is_a_reference_is_rejected() {
        rejects(
            "struct Node { next: &Node }",
            "a field cannot hold a reference",
        );
    }

    #[test]
    fn an_enum_variant_payload_that_is_a_reference_is_rejected() {
        rejects(
            "enum List { cons: &List, nil }",
            "a field cannot hold a reference",
        );
    }

    #[test]
    fn an_enum_record_variant_field_that_is_a_reference_is_rejected() {
        rejects(
            "enum List { cons: { next: &List }, nil }",
            "a field cannot hold a reference",
        );
    }

    #[test]
    fn a_tuple_field_containing_a_reference_is_rejected() {
        rejects(
            "struct Pair { both: (i32, &i32) }",
            "a field cannot hold a reference",
        );
    }

    #[test]
    fn an_array_field_containing_a_reference_is_rejected() {
        rejects(
            "struct Bucket { items: [&i32; 3] }",
            "a field cannot hold a reference",
        );
    }

    #[test]
    fn instantiating_a_generic_struct_with_a_reference_argument_in_a_field_is_rejected() {
        rejects(
            "struct Boxed<T> { value: T }
             struct Outer { boxed: Boxed<&i32> }",
            "a field cannot hold a reference",
        );
    }

    #[test]
    fn instantiating_a_generic_struct_with_a_reference_argument_as_a_parameter_checks() {
        accepts(
            "struct Boxed<T> { value: T }
             fun f(b: Boxed<&i32>) {}",
        );
    }

    #[test]
    fn instantiating_a_generic_struct_with_a_reference_argument_as_a_return_type_checks() {
        accepts(
            "struct Boxed<T> { value: T }
             fun f(b: &i32) -> Boxed<&i32> { return Boxed { value: b }; }",
        );
    }

    #[test]
    fn extending_a_generic_struct_with_a_reference_argument_is_rejected() {
        rejects(
            "struct Boxed<T> { value: T }
             extend Boxed<&i32> { fun get(&self) {} }",
            "a struct or enum cannot be instantiated with a reference",
        );
    }

    #[test]
    fn instantiating_a_generic_struct_with_an_owned_argument_checks() {
        accepts(
            "struct Boxed<T> { value: T }
             fun f(b: Boxed<i32>) {}",
        );
    }

    #[test]
    fn a_reference_typed_function_parameter_still_checks() {
        accepts("fun f(x: &i32) {}");
    }

    // -----------------------------------------------------------------
    // `any` is confined to a function's parameter and return types
    // -----------------------------------------------------------------

    #[test]
    fn any_as_a_parameter_or_return_type_checks() {
        accepts("fun f(x: any i32) -> any i32 { return x; }");
    }

    #[test]
    fn a_struct_field_that_is_any_is_rejected() {
        rejects(
            "struct Wrap { inner: any i32 }",
            "`any` may only appear in a parameter or return type",
        );
    }

    #[test]
    fn an_enum_variant_payload_that_is_any_is_rejected() {
        rejects(
            "enum Opt { some: any i32, none }",
            "`any` may only appear in a parameter or return type",
        );
    }

    #[test]
    fn a_tuple_field_containing_any_is_rejected() {
        rejects(
            "struct Pair { both: (i32, any i32) }",
            "`any` may only appear in a parameter or return type",
        );
    }

    #[test]
    fn instantiating_a_generic_struct_with_any_as_the_argument_is_rejected() {
        rejects(
            "struct Boxed<T> { value: T }
             fun f(b: Boxed<any i32>) {}",
            "`any` may only appear in a parameter or return type",
        );
    }

    #[test]
    fn a_let_binding_annotated_any_is_rejected() {
        rejects(
            "fun f(x: any i32) { let y: any i32 = x; }",
            "`any` may only appear in a parameter or return type",
        );
    }

    // -----------------------------------------------------------------
    // `dyn` requires wrapping with `&` or `iso`
    // -----------------------------------------------------------------

    #[test]
    fn a_bare_dyn_parameter_is_rejected() {
        rejects(
            "trait Show { fun show(&self); }
             fun f(x: dyn Show) {}",
            "`dyn` has no size known at compile time",
        );
    }

    #[test]
    fn a_bare_dyn_return_type_is_rejected() {
        let messages = crate::testing::typeck_src(
            "trait Show { fun show(&self); }
             fun f() -> dyn Show { }",
        );
        assert!(
            messages
                .iter()
                .any(|m| m.contains("`dyn` has no size known at compile time")),
            "{messages:?}"
        );
    }

    #[test]
    fn a_reference_typed_dyn_parameter_checks() {
        accepts(
            "trait Show { fun show(&self); }
             fun f(x: &dyn Show) {}",
        );
    }

    #[test]
    fn an_iso_typed_dyn_parameter_checks() {
        accepts(
            "trait Show { fun show(&self); }
             fun f(x: iso dyn Show) {}",
        );
    }

    #[test]
    fn a_struct_field_that_is_a_bare_dyn_is_rejected() {
        rejects(
            "trait Show { fun show(&self); }
             struct Wrap { inner: dyn Show }",
            "`dyn` has no size known at compile time",
        );
    }

    #[test]
    fn a_struct_field_that_is_a_reference_to_dyn_is_rejected() {
        rejects(
            "trait Show { fun show(&self); }
             struct Wrap { inner: &dyn Show }",
            "a field cannot hold a reference",
        );
    }

    #[test]
    fn a_struct_field_that_is_an_iso_dyn_checks() {
        accepts(
            "trait Show { fun show(&self); }
             struct Wrap { inner: iso dyn Show }",
        );
    }

    #[test]
    fn instantiating_a_generic_struct_with_a_bare_dyn_argument_in_a_field_is_rejected() {
        rejects(
            "trait Show { fun show(&self); }
             struct Boxed<T> { value: T }
             struct Outer { boxed: Boxed<dyn Show> }",
            "`dyn` has no size known at compile time",
        );
    }

    #[test]
    fn instantiating_a_generic_struct_with_an_iso_dyn_argument_in_a_field_checks() {
        accepts(
            "trait Show { fun show(&self); }
             struct Boxed<T> { value: T }
             struct Outer { boxed: Boxed<iso dyn Show> }",
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
}
