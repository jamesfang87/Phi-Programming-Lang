use std::collections::HashSet;

use crate::ast::{BinaryOp, Ident, Literal, Mutability, Symbol, UnaryOp};
use crate::diagnostics::typeck::expr::{
    report_assert_cond_not_bool, report_assign_mismatch, report_cast_not_allowed,
    report_cast_operand_unknown, report_cast_source_not_primitive,
    report_cast_target_not_primitive, report_closure_body_mismatch,
    report_compound_assign_mismatch, report_compound_assign_result_mismatch,
    report_ctor_not_a_struct, report_deref_not_a_reference, report_duplicate_field,
    report_elided_ctor_unknown, report_field_type_mismatch, report_if_branches_mismatch,
    report_if_cond_not_bool, report_if_no_else_mismatch, report_index_base_unknown,
    report_index_not_int, report_match_arm_mismatch, report_match_guard_not_bool,
    report_missing_fields, report_move_out_of_reference, report_new_array_count_not_usize,
    report_not_a_struct_literal, report_not_assignable, report_not_indexable, report_not_try,
    report_owned_element_in_new_array, report_panic_message_not_str, report_record_field_unknown,
    report_reference_in_new, report_try_error_mismatch, report_try_operand_unknown,
    report_try_outside, report_try_return_mismatch, report_variant_enum_unknown,
    report_variant_expr_payload_shape, report_variant_missing_fields,
    report_variant_payload_mismatch,
};
use crate::diagnostics::typeck::lower_ty::report_trait_as_ty;
use crate::diagnostics::typeck::traits::solve::report_operator_trait_missing;
use crate::diagnostics::typeck::{
    report_any_outside_signature, report_binary_operand_mismatch,
    report_int_suffix_on_float_literal, report_logic_op_needs_bool_operands, report_no_field,
    report_no_variant, report_operand_has_unknown_type, report_private_field,
    report_unknown_literal_suffix,
};
use crate::driver::source::SrcSpan;
use crate::hir::BindingMode;
use crate::hir::{
    AccessArgs, ArmId, DefId, ExprId, ExprKind, Hir, HirId, Local, OwnerNode, Path, Payload,
    PayloadField, Res, TyDef, Type,
};
use crate::langitems::LangItem;
use crate::nameres::PrimTy;
use crate::nameres::symbol_table::is_prim_ty;
use crate::typeck::Typeck;
use crate::typeck::check::cast;
use crate::typeck::check::pat::VariantTys;
use crate::typeck::results::DerefMode;
use crate::typeck::traits::solve::{Goal, Solution};
use crate::typeck::ty::{InferVar, Ty, TyKind};

#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum DerefContext {
    Value,
    Place,
}

#[derive(Clone)]
pub(crate) enum PayloadExprs<'hir> {
    Unit,

    Args(Vec<ExprId>),
    Record(&'hir [PayloadField]),
}

impl<'hir> PayloadExprs<'hir> {
    fn from_payload(payload: &'hir Payload) -> Self {
        match payload {
            Payload::None => PayloadExprs::Unit,
            Payload::Single(value) => PayloadExprs::Args(vec![(*value).into()]),
            Payload::Record(fields) => PayloadExprs::Record(fields),
        }
    }

    pub(crate) fn from_access_args(args: &'hir AccessArgs) -> Self {
        match args {
            AccessArgs::None => PayloadExprs::Unit,
            AccessArgs::Call(args) => PayloadExprs::Args(args.clone()),
            AccessArgs::Record(fields) => PayloadExprs::Record(fields),
        }
    }
}

impl<'hir> Typeck<'hir> {

    pub(crate) fn check_assign(
        &mut self,
        lhs: impl Into<HirId>,
        rhs: impl Into<HirId>,
        span: SrcSpan,
    ) -> Ty {
        let (lhs, rhs) = (lhs.into(), rhs.into());
        let lhs_ty = self.ty_of_as_place(lhs);

        if !self.is_place_expr(lhs) {
            report_not_assignable(self.session, self.hir.expr(lhs).span);
        }

        let rhs_ty = self.ty_of_expecting(rhs, Some(lhs_ty));
        if let Err(err) = self.unifier.unify(&self.tcx, lhs_ty, rhs_ty) {
            report_assign_mismatch(self.display_cx(), err, span);
        }
        self.tcx.unit()
    }

    pub(crate) fn check_assign_op(
        &mut self,
        op: BinaryOp,
        lhs: impl Into<HirId>,
        rhs: impl Into<HirId>,
        span: SrcSpan,
    ) -> Ty {
        let (lhs, rhs) = (lhs.into(), rhs.into());
        let lhs_ty = self.ty_of_as_place(lhs);

        if !self.is_place_expr(lhs) {
            report_not_assignable(self.session, self.hir.expr(lhs).span);
        }

        let rhs_ty = self.ty_of_expecting(rhs, Some(lhs_ty));
        if let Err(err) = self.unifier.unify(&self.tcx, lhs_ty, rhs_ty) {
            report_compound_assign_mismatch(self.display_cx(), err, span);
            return self.tcx.unit();
        }

        let operand = self.unifier.find_deep(&mut self.tcx, lhs_ty);
        let produced = self.check_operator(op, operand, lhs.owner, span);

        if let Err(err) = self.unifier.unify(&self.tcx, operand, produced) {
            report_compound_assign_result_mismatch(self.display_cx(), err, span);
        }
        self.tcx.unit()
    }

    pub(crate) fn check_borrow(
        &mut self,
        mutability: Mutability,
        operand: impl Into<HirId>,
        expected: Option<Ty>,
    ) -> Ty {
        let operand = operand.into();

        if mutability == Mutability::Mutable && !self.is_place_expr(operand) {
            report_not_assignable(self.session, self.hir.expr(operand).span);
        }

        let inner = expected.and_then(|expected| match *self.tcx.kind(expected) {
            TyKind::Ref {
                base,
                mutability: m,
            } if m == mutability => Some(base),
            _ => None,
        });
        let ty = self.ty_of_as_place_expecting(operand, inner);
        self.tcx.mk_ref(ty, mutability)
    }

    pub(crate) fn check_deref(
        &mut self,
        id: impl Into<HirId>,
        operand: impl Into<HirId>,
        span: SrcSpan,
    ) -> Ty {
        let (id, operand) = (id.into(), operand.into());
        self.check_deref_as(id, operand, span, DerefContext::Value)
    }

    pub(crate) fn check_deref_as(
        &mut self,
        id: impl Into<HirId>,
        operand: impl Into<HirId>,
        span: SrcSpan,
        ctx: DerefContext,
    ) -> Ty {
        let (id, operand) = (id.into(), operand.into());
        let operand_ty = self.ty_of_as_place(operand);
        let resolved = self.unifier.find_deep(&mut self.tcx, operand_ty);

        let (base, from_reference) = match *self.tcx.kind(resolved) {
            TyKind::Ref { base, .. } => (base, true),
            TyKind::Iso(base) => (base, false),
            TyKind::Error => return self.tcx.error(),
            _ => {
                report_deref_not_a_reference(self.display_cx(), resolved, span);
                return self.tcx.error();
            }
        };

        let mode = self.deref_mode(base, id.owner);
        self.types.record_deref(id, mode);

        if ctx == DerefContext::Value && from_reference && mode == DerefMode::Move {
            report_move_out_of_reference(self.display_cx(), base, span);
            return self.tcx.error();
        }

        base
    }

    fn deref_mode(&mut self, ty: Ty, owner: DefId) -> DerefMode {
        if matches!(
            self.tcx.kind(ty),
            TyKind::Var(InferVar::Int(_) | InferVar::Float(_))
        ) {
            return DerefMode::Copy;
        }
        if self.holds_lang_trait(LangItem::Drop, ty, owner) {
            DerefMode::Move
        } else if self.holds_lang_trait(LangItem::Copy, ty, owner) {
            DerefMode::Copy
        } else {
            DerefMode::Move
        }
    }

    fn holds_lang_trait(&mut self, item: LangItem, ty: Ty, owner: DefId) -> bool {
        let Some(def) = self.hir.lang_items().get(item) else {
            return false;
        };
        let goal = Goal::new(ty, def);
        let env = self.bounds_env(owner);
        matches!(self.implements(&goal, &env), Solution::Holds)
    }

    pub(crate) fn record_copyability(&mut self, ty: Ty, owner: DefId) {
        let resolved = self.unifier.find_deep(&mut self.tcx, ty);
        if self.holds_lang_trait(LangItem::Copy, resolved, owner) {
            self.tcx.mark_copy(resolved);
        }
    }

    pub(crate) fn check_index(
        &mut self,
        id: impl Into<HirId>,
        base: impl Into<HirId>,
        index: impl Into<HirId>,
    ) -> Ty {
        let (id, base, index) = (id.into(), base.into(), index.into());
        let span = self.hir.expr(id).span;
        let base_ty = self.ty_of_as_place(base);

        if matches!(self.tcx.kind(base_ty), TyKind::Error) {
            self.ty_of(index);
            return self.tcx.error();
        }
        if matches!(self.tcx.kind(base_ty), TyKind::Var(_)) {
            report_index_base_unknown(self.session, self.hir.expr(base).span);
            self.ty_of(index);
            return self.tcx.error();
        }

        let (peeled, _layers) = self.peel_receiver(base_ty);
        if let TyKind::Array { elem, .. } = *self.tcx.kind(peeled) {
            self.check_array_index_operand(index, span);
            return elem;
        }
        self.check_index_through_method(id, base, peeled, index, span)
    }

    fn check_array_index_operand(&mut self, index: HirId, span: SrcSpan) {
        let int = self.tcx.next_int_var();
        let index_ty = self.ty_of(index);
        if let Err(err) = self.unifier.unify(&self.tcx, int, index_ty) {
            report_index_not_int(self.display_cx(), err, span);
        }
    }

    fn check_index_through_method(
        &mut self,
        id: HirId,
        base: HirId,
        peeled: Ty,
        index: HirId,
        span: SrcSpan,
    ) -> Ty {
        let member = Ident {
            text: self.session.intern("index"),
            span,
        };
        if self
            .method_candidates(peeled, member.text, base.owner)
            .is_empty()
        {
            report_not_indexable(self.display_cx(), peeled, span);
            self.ty_of(index);
            return self.tcx.error();
        }
        self.check_method_call(id, base, member, &[ExprId::from(index)])
    }

    pub(crate) fn check_ctor(
        &mut self,
        id: impl Into<HirId>,
        path: Option<&'hir Path>,
        payload: &'hir [PayloadField],
        expected: Option<Ty>,
    ) -> Ty {
        let id = id.into();
        let (span, owner) = (self.hir.expr(id).span, id.owner);
        let Some(self_ty) = self.ctor_ty(path, expected, span, owner) else {
            self.check_fields_only(payload);
            return self.tcx.error();
        };
        let Some((struct_def, declared)) = self.struct_fields(self_ty) else {
            report_not_a_struct_literal(self.display_cx(), self_ty, span);
            self.check_fields_only(payload);
            return self.tcx.error();
        };

        let written = self.check_ctor_fields(struct_def, &declared, payload, self_ty, owner);
        self.report_missing_ctor_fields(&declared, &written, self_ty, span);
        self_ty
    }

    fn check_fields_only(&mut self, payload: &'hir [PayloadField]) {
        for field in payload {
            self.ty_of(field.value);
        }
    }

    fn check_ctor_fields(
        &mut self,
        struct_def: DefId,
        declared: &[(Ident, HirId, Ty)],
        payload: &'hir [PayloadField],
        self_ty: Ty,
        owner: DefId,
    ) -> HashSet<Symbol> {
        let struct_module = self.hir.module_of(struct_def);
        let mut written = HashSet::new();
        for field in payload {
            if !written.insert(field.name.text) {
                report_duplicate_field(self.session, field.name);
            }
            match declared
                .iter()
                .find(|(name, _, _)| name.text == field.name.text)
            {
                Some(&(_, field_id, want)) => {
                    let visibility = self.hir.field(field_id).visibility;
                    if !self.is_visible_from(struct_module, owner, visibility) {
                        report_private_field(self.session, field.name);
                    }
                    self.check_field_value(field.value, want);
                }
                None => {
                    report_no_field(self.display_cx(), field.name, self_ty);
                    self.ty_of(field.value);
                }
            }
        }
        written
    }

    fn check_field_value(&mut self, value: HirId, want: Ty) {
        let got = self.ty_of_expecting(value, Some(want));
        if let Err(err) = self.unifier.unify(&self.tcx, want, got) {
            report_field_type_mismatch(self.display_cx(), err, self.hir.expr(value).span);
        }
    }

    fn report_missing_ctor_fields(
        &self,
        declared: &[(Ident, HirId, Ty)],
        written: &HashSet<Symbol>,
        self_ty: Ty,
        span: SrcSpan,
    ) {
        let missing: Vec<&'static str> = declared
            .iter()
            .filter(|(name, _, _)| !written.contains(&name.text))
            .map(|(name, _, _)| self.session.resolve(name.text))
            .collect();
        if !missing.is_empty() {
            report_missing_fields(self.display_cx(), &missing, self_ty, span);
        }
    }

    fn ctor_ty(
        &mut self,
        path: Option<&Path>,
        expected: Option<Ty>,
        span: SrcSpan,
        owner: DefId,
    ) -> Option<Ty> {
        let Some(path) = path else {
            let expected = expected.map(|ty| self.unifier.find_deep(&mut self.tcx, ty));
            return match expected {
                Some(ty) if matches!(self.tcx.kind(ty), TyKind::Adt { .. }) => Some(ty),
                Some(ty) if matches!(self.tcx.kind(ty), TyKind::Error) => None,
                _ => {
                    report_elided_ctor_unknown(self.session, span);
                    None
                }
            };
        };

        match path.res {
            Res::Type(Type::Def(TyDef::Struct(def))) => {
                let struct_ = self.hir.struct_(def);
                let args: Vec<Ty> = struct_
                    .generics
                    .iter()
                    .map(|_| self.tcx.next_infer_var())
                    .collect();
                let ty = self.tcx.mk_adt(def, args);
                if let Some(expected) = expected {
                    let _ = self.unifier.unify(&self.tcx, expected, ty);
                }
                Some(ty)
            }
            Res::SelfTy(_) => Some(self.self_ty(owner, span)),
            Res::Err => None,
            _ => {
                report_ctor_not_a_struct(self.session, span);
                None
            }
        }
    }

    pub(crate) fn check_variant_expr(
        &mut self,
        variant: Ident,
        payload: &'hir Payload,
        expected: Option<Ty>,
        span: SrcSpan,
    ) -> Ty {
        let expected = expected.map(|ty| self.unifier.find_deep(&mut self.tcx, ty));
        let written = PayloadExprs::from_payload(payload);
        let self_ty = match expected {
            Some(ty) if matches!(self.tcx.kind(ty), TyKind::Error) => {
                self.check_payload_exprs_only(&written);
                return self.tcx.error();
            }
            Some(ty) if !matches!(self.tcx.kind(ty), TyKind::Var(_)) => ty,
            _ => {
                report_variant_enum_unknown(self.session, variant, span);
                self.check_payload_exprs_only(&written);
                return self.tcx.error();
            }
        };

        self.check_variant_of(self_ty, variant, written, span)
    }

    pub(crate) fn check_variant_of(
        &mut self,
        self_ty: Ty,
        variant: Ident,
        written: PayloadExprs<'hir>,
        span: SrcSpan,
    ) -> Ty {
        if matches!(self.tcx.kind(self_ty), TyKind::Error) {
            self.check_payload_exprs_only(&written);
            return self.tcx.error();
        }

        let Some(found) = self.resolve_variant(self_ty, variant.text) else {
            report_no_variant(self.display_cx(), variant, self_ty);
            self.check_payload_exprs_only(&written);
            return self.tcx.error();
        };

        match (&found.payload, &written) {
            (VariantTys::Unit, PayloadExprs::Unit) => {}
            (VariantTys::Single(want), PayloadExprs::Args(args)) if args.len() == 1 => {
                self.check_single_payload(*want, args[0]);
            }
            (VariantTys::Record(want), PayloadExprs::Record(fields)) => {
                let want = want.clone();
                self.check_variant_record(&want, fields, found.id);
            }
            _ => {
                let declared = found.payload.describe();
                report_variant_expr_payload_shape(
                    self.session,
                    self.hir,
                    variant,
                    span,
                    declared,
                    found.id,
                );
                self.check_payload_exprs_only(&written);
            }
        }

        self_ty
    }

    fn check_single_payload(&mut self, want: Ty, value: ExprId) {
        let got = self.ty_of_expecting(value, Some(want));
        if let Err(err) = self.unifier.unify(&self.tcx, want, got) {
            report_variant_payload_mismatch(self.display_cx(), err, self.hir.expr(value).span);
        }
    }

    pub(crate) fn named_type_of_base(&mut self, base: HirId) -> Option<Ty> {
        let expr = self.hir.expr(base);
        let ExprKind::Path(path) = &expr.kind else {
            return None;
        };
        let (res, span) = (path.res, expr.span);

        let ty = match res {
            Res::Type(Type::Prim(prim)) => self.tcx.mk_prim(prim),
            Res::Type(Type::Generic(param)) => self.tcx.mk_generic(param),
            Res::Type(Type::Def(TyDef::Struct(def) | TyDef::Enum(def))) => {
                let arity = match self.hir.def(def) {
                    OwnerNode::Struct(struct_) => struct_.generics.len(),
                    OwnerNode::Enum(enum_) => enum_.generics.len(),
                    _ => unreachable!("a TyDef::Struct/Enum always names a Struct/Enum owner"),
                };
                let args = (0..arity).map(|_| self.tcx.next_infer_var()).collect();
                self.tcx.mk_adt(def, args)
            }

            Res::Type(Type::Def(TyDef::Trait(_))) => {
                report_trait_as_ty(self.session, span);
                self.tcx.error()
            }
            Res::SelfTy(_) => self.self_ty(base.owner, span),
            Res::Local(_) | Res::Function(_) | Res::Err => return None,
        };
        Some(ty)
    }

    fn check_variant_record(
        &mut self,
        declared: &[(Ident, Ty)],
        written: &'hir [PayloadField],
        variant: HirId,
    ) {
        let mut seen = HashSet::new();
        for field in written {
            if !seen.insert(field.name.text) {
                report_duplicate_field(self.session, field.name);
            }
            match declared
                .iter()
                .find(|(name, _)| name.text == field.name.text)
            {
                Some(&(_, want)) => self.check_field_value(field.value, want),
                None => {
                    report_record_field_unknown(self.session, self.hir, field.name, variant);
                    self.ty_of(field.value);
                }
            }
        }

        let missing: Vec<&'static str> = declared
            .iter()
            .filter(|(name, _)| !seen.contains(&name.text))
            .map(|(name, _)| self.session.resolve(name.text))
            .collect();
        if !missing.is_empty() {
            report_variant_missing_fields(self.session, self.hir, variant, &missing);
        }
    }

    fn check_payload_exprs_only(&mut self, written: &PayloadExprs<'hir>) {
        match written {
            PayloadExprs::Unit => {}
            PayloadExprs::Args(args) => {
                for &arg in args {
                    self.ty_of(arg);
                }
            }
            PayloadExprs::Record(fields) => {
                for field in *fields {
                    self.ty_of(field.value);
                }
            }
        }
    }

    pub(crate) fn check_if(
        &mut self,
        cond: impl Into<HirId>,
        then_block: impl Into<HirId>,
        else_block: Option<impl Into<HirId>>,
        expected: Option<Ty>,
        span: SrcSpan,
    ) -> Ty {
        let (cond, then_block) = (cond.into(), then_block.into());
        let cond_ty = self.ty_of(cond);
        let bool_ty = self.tcx.mk_prim(PrimTy::Bool);
        if let Err(err) = self.unifier.unify(&self.tcx, bool_ty, cond_ty) {
            report_if_cond_not_bool(self.display_cx(), err, self.hir.expr(cond).span);
        }

        let then_ty = self.check_block_expecting(then_block, expected);
        let Some(else_block) = else_block else {
            let unit = self.tcx.unit();
            if let Err(err) = self.unifier.unify(&self.tcx, unit, then_ty) {
                report_if_no_else_mismatch(self.display_cx(), err, span);
            }
            return unit;
        };

        let else_ty = self.check_block_expecting(else_block, expected);
        if let Err(err) = self.unifier.unify(&self.tcx, then_ty, else_ty) {
            report_if_branches_mismatch(self.display_cx(), err, span);
            return self.tcx.error();
        }
        self.unifier.find_deep(&mut self.tcx, then_ty)
    }

    pub(crate) fn check_match(
        &mut self,
        scrutinee: impl Into<HirId>,
        arms: &'hir [ArmId],
        expected: Option<Ty>,
        span: SrcSpan,
    ) -> Ty {
        let scrutinee = scrutinee.into();
        let scrutinee_ty = self.ty_of(scrutinee);

        if arms.is_empty() {
            return self.tcx.never();
        }

        let result = match expected {
            Some(expected) => expected,
            None => self.tcx.next_infer_var(),
        };

        let mut pat_failed = false;
        for &arm in arms {
            pat_failed |= self.check_match_arm(arm, scrutinee_ty, result);
        }

        if !pat_failed {
            let (peeled, _, _) = self.peel_for_pattern(scrutinee_ty, BindingMode::Value);
            self.check_match_exhaustive(peeled, arms, span);
        }

        self.unifier.find_deep(&mut self.tcx, result)
    }

    fn check_match_arm(&mut self, arm: ArmId, scrutinee_ty: Ty, result: Ty) -> bool {
        let arm_node = self.hir.arm(arm);
        let (pat, guard, block, arm_span) = (
            arm_node.pat,
            arm_node.guard,
            arm_node.block,
            arm_node.span,
        );

        self.check_pat(pat, scrutinee_ty, BindingMode::Value);
        let pat_failed = self
            .types
            .ty(pat.into())
            .is_some_and(|ty| matches!(self.tcx.kind(ty), TyKind::Error));

        if let Some(guard) = guard {
            let guard_ty = self.ty_of(guard);
            let bool_ty = self.tcx.mk_prim(PrimTy::Bool);
            if let Err(err) = self.unifier.unify(&self.tcx, bool_ty, guard_ty) {
                report_match_guard_not_bool(self.display_cx(), err, self.hir.expr(guard).span);
            }
        }

        let body = self.check_block_expecting(block, Some(result));
        if let Err(err) = self.unifier.unify(&self.tcx, result, body) {
            report_match_arm_mismatch(self.display_cx(), err, arm_span);
        }
        pat_failed
    }

    pub(crate) fn check_assert(
        &mut self,
        cond: impl Into<HirId>,
        msg: Option<impl Into<HirId>>,
    ) -> Ty {
        let cond = cond.into();
        let cond_ty = self.ty_of(cond);
        let bool_ty = self.tcx.mk_prim(PrimTy::Bool);
        if let Err(err) = self.unifier.unify(&self.tcx, bool_ty, cond_ty) {
            report_assert_cond_not_bool(self.display_cx(), err, self.hir.expr(cond).span);
        }
        self.check_panic_message(msg);
        self.tcx.unit()
    }

    pub(crate) fn check_panic_message(&mut self, msg: Option<impl Into<HirId>>) -> Ty {
        let msg = msg.map(Into::into);
        if let Some(msg) = msg {
            let msg_ty = self.ty_of(msg);
            let str_ty = self.tcx.mk_prim(PrimTy::Str);
            if let Err(err) = self.unifier.unify(&self.tcx, str_ty, msg_ty) {
                report_panic_message_not_str(self.display_cx(), err, self.hir.expr(msg).span);
            }
        }
        self.tcx.never()
    }

    pub(crate) fn check_try(&mut self, id: impl Into<HirId>, operand: impl Into<HirId>) -> Ty {
        let (id, operand) = (id.into(), operand.into());
        let (span, owner) = (self.hir.expr(id).span, id.owner);
        let operand_ty = self.ty_of(operand);

        if matches!(self.tcx.kind(operand_ty), TyKind::Error) {
            return self.tcx.error();
        }
        if matches!(self.tcx.kind(operand_ty), TyKind::Var(_)) {
            report_try_operand_unknown(self.session, span);
            return self.tcx.error();
        }

        let TyKind::Adt { def, args } = self.tcx.kind(operand_ty).clone() else {
            report_not_try(self.display_cx(), operand_ty, span);
            return self.tcx.error();
        };

        let result = self.hir.lang_items().get(LangItem::Result);
        let option = self.hir.lang_items().get(LangItem::Option);

        if Some(def) == result && args.len() == 2 {
            self.check_try_return(operand_ty, LangItem::Result, Some(args[1]), span, owner);
            return args[0];
        }
        if Some(def) == option && args.len() == 1 {
            self.check_try_return(operand_ty, LangItem::Option, None, span, owner);
            return args[0];
        }

        report_not_try(self.display_cx(), operand_ty, span);
        self.tcx.error()
    }

    fn check_try_return(
        &mut self,
        operand_ty: Ty,
        item: LangItem,
        error_ty: Option<Ty>,
        span: SrcSpan,
        owner: DefId,
    ) {
        let Some(ret) = self.signature(owner).and_then(|(_, ret)| ret) else {
            report_try_outside(self.display_cx(), operand_ty, span);
            return;
        };
        if matches!(self.tcx.kind(ret), TyKind::Error) {
            return;
        }

        let expected_def = self.hir.lang_items().get(item);
        let TyKind::Adt { def, args } = self.tcx.kind(ret).clone() else {
            report_try_return_mismatch(self.display_cx(), operand_ty, ret, span);
            return;
        };
        if Some(def) != expected_def {
            report_try_return_mismatch(self.display_cx(), operand_ty, ret, span);
            return;
        }

        if let (Some(error_ty), Some(ret_error)) = (error_ty, args.get(1).copied())
            && let Err(err) = self.unifier.unify(&self.tcx, ret_error, error_ty)
        {
            report_try_error_mismatch(self.display_cx(), err, span);
        }
    }

    pub(crate) fn check_cast(
        &mut self,
        operand: impl Into<HirId>,
        ty: impl Into<HirId>,
        span: SrcSpan,
    ) -> Ty {
        let (operand, ty) = (operand.into(), ty.into());
        let target_ty = self.lower_ty(ty.into());
        let operand_ty = self.ty_of(operand);

        let target_resolved = self.unifier.find_deep(&mut self.tcx, target_ty);
        let operand_resolved = self.unifier.find_deep(&mut self.tcx, operand_ty);

        if matches!(
            self.tcx.kind(operand_resolved),
            TyKind::Primitive(PrimTy::Str)
        ) && self.is_byte_slice_ref(target_resolved)
        {
            return target_ty;
        }

        let target_kind = self.tcx.kind(target_resolved).clone();
        let TyKind::Primitive(to) = target_kind else {
            if !matches!(target_kind, TyKind::Error) {
                report_cast_target_not_primitive(
                    self.display_cx(),
                    target_resolved,
                    self.hir.ty(ty).span,
                );
            }
            return target_ty;
        };

        let operand_span = self.hir.expr(operand).span;
        let operand_kind = self.tcx.kind(operand_ty).clone();

        let from = match operand_kind {
            TyKind::Primitive(prim) => prim,
            TyKind::Error => return target_ty,
            TyKind::Var(InferVar::Int(_)) if to.is_integer() => {
                let _ = self.unifier.unify(&self.tcx, operand_ty, target_ty);
                return target_ty;
            }
            TyKind::Var(InferVar::Float(_)) if to.is_float() => {
                let _ = self.unifier.unify(&self.tcx, operand_ty, target_ty);
                return target_ty;
            }
            TyKind::Var(_) => {
                report_cast_operand_unknown(self.session, operand_span);
                return target_ty;
            }
            _ => {
                report_cast_source_not_primitive(self.display_cx(), operand_ty, operand_span);
                return target_ty;
            }
        };

        if let Err(reason) = cast::is_lossless_cast(from, to) {
            report_cast_not_allowed(self.display_cx(), operand_ty, target_resolved, reason, span);
        }

        target_ty
    }

    fn is_byte_slice_ref(&self, ty: Ty) -> bool {
        let TyKind::Ref { base, mutability } = *self.tcx.kind(ty) else {
            return false;
        };
        if mutability != Mutability::Immutable {
            return false;
        }
        let TyKind::Array { elem, len: None } = *self.tcx.kind(base) else {
            return false;
        };
        matches!(self.tcx.kind(elem), TyKind::Primitive(PrimTy::U8))
    }

    pub(crate) fn check_new(&mut self, operand: impl Into<HirId>) -> Ty {
        let operand = operand.into();
        let operand_ty = self.ty_of(operand);
        self.check_storable_in_iso(operand_ty, self.hir.expr(operand).span);
        self.tcx.mk_iso(operand_ty)
    }

    pub(crate) fn check_new_array(
        &mut self,
        elem: impl Into<HirId>,
        count: impl Into<HirId>,
    ) -> Ty {
        let (elem, count) = (elem.into(), count.into());
        let elem_ty = self.ty_of(elem);
        let count_ty = self.ty_of(count);
        let usize_ty = self.tcx.mk_prim(PrimTy::Usize);
        if let Err(err) = self.unifier.unify(&self.tcx, count_ty, usize_ty) {
            report_new_array_count_not_usize(self.display_cx(), err, self.hir.expr(count).span);
        }
        let elem_span = self.hir.expr(elem).span;
        self.check_storable_in_iso(elem_ty, elem_span);
        if self.tcx.needs_drop(elem_ty) {
            report_owned_element_in_new_array(self.display_cx(), elem_ty, elem_span);
        }
        let array_ty = self.tcx.mk_array(elem_ty, None);
        self.tcx.mk_iso(array_ty)
    }

    fn check_storable_in_iso(&mut self, ty: Ty, span: SrcSpan) {
        if self.tcx.contains_ref(ty) {
            report_reference_in_new(self.display_cx(), ty, span);
        }
        if self.tcx.contains_any(ty) {
            report_any_outside_signature(self.display_cx(), ty, span);
        }
    }

    pub(crate) fn check_closure(&mut self, def: DefId, expected: Option<Ty>) -> Ty {
        let hir: &'hir Hir = self.hir;
        let closure = hir.closure(def);

        let hint = self.closure_hint(expected, closure.params.len());
        let param_tys = self.check_closure_params(&closure.params, hint.as_ref());
        let ret_var = self.check_closure_return(closure.ret, hint.as_ref());
        self.types
            .record_def(def, self.tcx.mk_fun(param_tys.clone(), Some(ret_var)));

        let body = self.check_block_expecting(closure.block, Some(ret_var));
        if let Err(err) = self.unifier.unify(&self.tcx, ret_var, body) {
            report_closure_body_mismatch(self.display_cx(), err, closure.span);
        }

        let ret = self.unifier.find_deep(&mut self.tcx, ret_var);
        let ret = (ret != self.tcx.unit()).then_some(ret);
        let sig = self.tcx.mk_fun(param_tys, ret);
        self.types.record_def(def, sig);
        sig
    }

    fn closure_hint(
        &mut self,
        expected: Option<Ty>,
        arity: usize,
    ) -> Option<(Vec<Ty>, Option<Ty>)> {
        match self.tcx.kind(expected?).clone() {
            TyKind::Fun { params, ret } if params.len() == arity => Some((params, ret)),
            _ => None,
        }
    }

    fn check_closure_params(
        &mut self,
        params: &'hir [HirId],
        hint: Option<&(Vec<Ty>, Option<Ty>)>,
    ) -> Vec<Ty> {
        let mut param_tys = Vec::with_capacity(params.len());
        for (index, &id) in params.iter().enumerate() {
            let ty = match self.hir.closure_param(id).ty {
                Some(annotation) => self.lower_ty(annotation),
                None => match hint.map(|(params, _)| params[index]) {
                    Some(ty) => ty,
                    None => self.tcx.next_infer_var(),
                },
            };
            self.types.record(id, ty);
            param_tys.push(ty);
        }
        param_tys
    }

    fn check_closure_return(
        &mut self,
        declared: Option<crate::hir::TyId>,
        hint: Option<&(Vec<Ty>, Option<Ty>)>,
    ) -> Ty {
        let ret_var = self.tcx.next_infer_var();
        if let Some(declared) = declared.map(|ret| self.lower_ty(ret)) {
            let _ = self.unifier.unify(&self.tcx, declared, ret_var);
        } else if let Some(ret) = hint.and_then(|(_, ret)| *ret) {
            let _ = self.unifier.unify(&self.tcx, ret, ret_var);
        }
        ret_var
    }
}

#[cfg(test)]
mod tests {
    use crate::testing::{typeck_accepts as accepts, typeck_rejects as rejects};

    #[test]
    fn a_let_binding_takes_its_type_from_its_initializer() {
        accepts("fun f() -> i32 { let x = 1; return x; }");
        rejects(
            "fun f() -> bool { let x = 1; return x; }",
            "mismatched types",
        );
    }

    #[test]
    fn a_let_annotation_is_what_the_initializer_is_checked_against() {
        accepts("fun f() -> i64 { let x: i64 = 1; return x; }");
        rejects("fun f() { let x: bool = 1; }", "mismatched types");
    }

    #[test]
    fn a_tuple_pattern_binds_each_element_separately() {
        accepts(
            "fun f() -> bool {
                 let (a, b) = (1, true);
                 return b;
             }",
        );
        rejects(
            "fun f() -> bool {
                 let (a, b) = (1, true);
                 return a;
             }",
            "mismatched types",
        );
    }

    #[test]
    fn a_tuple_pattern_of_the_wrong_arity_is_reported() {
        rejects("fun f() { let (a, b, c) = (1, true); }", "mismatched types");
    }

    #[test]
    fn a_with_lend_binds_its_pattern_like_a_let() {
        accepts(
            "fun f() {
                 let x = 1;
                 with borrowed = &x { let y: &i32 = borrowed; }
             }",
        );
    }

    #[test]
    fn an_assignment_checks_the_value_against_the_place() {
        accepts("fun f() { let mut x = 1; x = 2; }");
        rejects("fun f() { let mut x = 1; x = true; }", "mismatched types");
    }

    #[test]
    fn assigning_to_something_that_is_not_a_place_is_reported() {
        rejects("fun f() { 1 = 2; }", "cannot be assigned to");
    }

    #[test]
    fn an_assignment_produces_no_value() {
        rejects(
            "fun f() { let mut x = 1; let g: fun() -> i32 = || { x = 2 }; }",
            "mismatched types",
        );
    }

    #[test]
    fn a_compound_assignment_needs_the_operators_trait() {
        accepts("fun f() { let mut x = 1; x += 2; }");
        rejects(
            "module core::ops;

             public trait Add { fun add(&self, other: &Self) -> Self; }

             struct Foo { x: i32 }

             fun f(a: Foo, b: Foo) { let mut c = a; c += b; }",
            "does not implement `Add`",
        );
    }

    #[test]
    fn a_borrow_produces_a_reference_to_its_operands_type() {
        accepts("fun f(x: i32) -> &i32 { return &x; }");
        rejects("fun f(x: i32) -> &bool { return &x; }", "mismatched types");
    }

    #[test]
    fn a_mutable_borrow_is_not_a_shared_one() {
        accepts("fun f(x: i32) -> &mut i32 { return &mut x; }");
        rejects(
            "fun f(x: i32) -> &mut i32 { return &x; }",
            "mismatched types",
        );
    }

    #[test]
    fn a_struct_literal_checks_each_field_against_its_declared_type() {
        accepts(
            "struct Pair { fst: i32, snd: bool }
             fun f() -> Pair { return Pair { fst: 1, snd: true }; }",
        );
        rejects(
            "struct Pair { fst: i32, snd: bool }
             fun f() -> Pair { return Pair { fst: true, snd: true }; }",
            "mismatched types",
        );
    }

    #[test]
    fn a_struct_literal_missing_a_field_is_reported() {
        rejects(
            "struct Pair { fst: i32, snd: bool }
             fun f() -> Pair { return Pair { fst: 1 }; }",
            "is missing field `snd`",
        );
    }

    #[test]
    fn a_struct_literal_with_a_field_that_is_not_declared_is_reported() {
        rejects(
            "struct Pair { fst: i32 }
             fun f() -> Pair { return Pair { fst: 1, third: 2 }; }",
            "no field `third`",
        );
    }

    #[test]
    fn a_struct_literal_field_can_elide_its_value() {
        accepts(
            "struct Pair { fst: i32, snd: bool }
             fun f(fst: i32, snd: bool) -> Pair { return Pair { fst, snd }; }",
        );
    }

    #[test]
    fn an_elided_struct_literal_field_that_is_not_declared_is_reported() {
        rejects(
            "struct Pair { fst: i32 }
             fun f(fst: i32, third: i32) -> Pair { return Pair { fst, third }; }",
            "no field `third`",
        );
    }

    #[test]
    fn an_elided_struct_literal_takes_its_struct_from_the_expectation() {
        accepts(
            "struct Pair { fst: i32, snd: bool }
             fun f() -> Pair { return .{ fst: 1, snd: true }; }",
        );
    }

    #[test]
    fn an_elided_struct_literal_with_nothing_expecting_it_is_reported() {
        rejects(
            "struct Pair { fst: i32 }
             fun f() { let p = .{ fst: 1 }; }",
            "names no struct",
        );
    }

    #[test]
    fn a_generic_struct_literals_arguments_come_from_the_annotation() {
        accepts(
            "struct Wrap<T> { inner: T }
             fun f() { let w: Wrap<i64> = Wrap { inner: 1 }; }",
        );
        rejects(
            "struct Wrap<T> { inner: T }
             fun f() { let w: Wrap<bool> = Wrap { inner: 1 }; }",
            "mismatched types",
        );
    }

    #[test]
    fn a_struct_literal_cannot_set_a_private_field_from_another_module() {
        assert_eq!(
            crate::testing::typeck_src_files(&[
                "module math; public struct Foo { count: i32, public label: i32 }",
                "module app;
                 import math::Foo;
                 fun f() -> Foo { return Foo { count: 1, label: 2 }; }",
            ]),
            ["field `count` is private"]
        );
    }

    #[test]
    fn a_struct_literal_may_set_a_public_field_from_another_module() {
        assert!(
            crate::testing::typeck_src_files(&[
                "module math; public struct Foo { public count: i32 }",
                "module app;
                 import math::Foo;
                 fun f() -> Foo { return Foo { count: 1 }; }",
            ])
            .is_empty()
        );
    }

    #[test]
    fn multiple_private_fields_in_one_struct_literal_are_each_reported() {
        assert_eq!(
            crate::testing::typeck_src_files(&[
                "module math; public struct Foo { a: i32, b: i32, public c: i32 }",
                "module app;
                 import math::Foo;
                 fun f() -> Foo { return Foo { a: 1, b: 2, c: 3 }; }",
            ]),
            ["field `a` is private", "field `b` is private"]
        );

        assert_eq!(
            crate::testing::typeck_src_files(&[
                "module math; public struct Foo { a: i32, b: i32, public c: i32 }",
                "module app;
                 import math::Foo;
                 fun f() -> Foo { return Foo { c: 3 }; }",
            ]),
            ["`Foo` is missing fields `a` and `b`"]
        );
    }

    #[test]
    fn a_variant_takes_its_enum_from_the_expectation() {
        accepts(
            "enum Shape { unit, circle: f64 }
             fun f() -> Shape { return .circle(1.0); }",
        );
    }

    #[test]
    fn a_variants_payload_carries_the_expectation_further_down() {
        accepts(
            "enum Option<T> { some: T, none }
             enum Result<T, E> { ok: T, err: E }
             fun f() -> Result<Option<i32>, bool> { return .ok(.none); }",
        );
    }

    #[test]
    fn a_variant_with_nothing_expecting_it_is_reported() {
        rejects(
            "enum Shape { unit }
             fun f() { let s = .unit; }",
            "the enum `.unit` belongs to is unknown",
        );
    }

    #[test]
    fn a_variant_the_enum_does_not_declare_is_reported() {
        rejects(
            "enum Shape { unit }
             fun f() -> Shape { return .square; }",
            "no variant `square`",
        );
    }

    #[test]
    fn a_variant_built_with_the_wrong_payload_shape_is_reported() {
        rejects(
            "enum Shape { unit, circle: f64 }
             fun f() -> Shape { return .unit(1.0); }",
            "carries no payload",
        );
    }

    #[test]
    fn a_record_variants_fields_are_checked_against_their_declarations() {
        accepts(
            "enum Shape { square: { l: f64 } }
             fun f() -> Shape { return .square { l: 1.0 }; }",
        );
        rejects(
            "enum Shape { square: { l: f64 } }
             fun f() -> Shape { return .square { l: true }; }",
            "mismatched types",
        );
    }

    #[test]
    fn a_variant_can_be_named_through_its_enum() {
        accepts(
            "enum Shape { unit, circle: f64, square: { l: f64 } }
             fun a() -> Shape { return Shape.unit; }
             fun b() -> Shape { return Shape.circle(1.0); }
             fun c() -> Shape { return Shape.square { l: 1.0 }; }",
        );
    }

    #[test]
    fn naming_the_enum_supplies_what_no_expectation_would() {
        accepts(
            "enum Shape { unit }
             fun f() { let s = Shape.unit; let t = s; }",
        );
    }

    #[test]
    fn a_generic_enums_arguments_are_inferred_at_the_qualified_variant() {
        accepts(
            "enum Option<T> { some: T, none }
             fun f() -> Option<i32> { return Option.some(1); }",
        );
        rejects(
            "enum Option<T> { some: T, none }
             fun f() -> Option<i32> { return Option.some(true); }",
            "mismatched types",
        );
    }

    #[test]
    fn self_names_the_enum_inside_an_extend_block() {
        accepts(
            "enum Shape { unit, circle: f64 }
             extend Shape { fun make() -> Shape { return Self.circle(2.0); } }",
        );
    }

    #[test]
    fn a_qualified_variant_the_enum_does_not_declare_is_reported() {
        rejects(
            "enum Shape { unit }
             fun f() -> Shape { return Shape.square; }",
            "no variant `square`",
        );
    }

    #[test]
    fn a_qualified_variant_built_with_the_wrong_payload_shape_is_reported() {
        rejects(
            "enum Shape { unit, circle: f64 }
             fun f() -> Shape { return Shape.unit(1.0); }",
            "carries no payload",
        );
        rejects(
            "enum Shape { unit, circle: f64 }
             fun f() -> Shape { return Shape.circle(1.0, 2.0); }",
            "carries a single value",
        );
    }

    #[test]
    fn a_variant_named_through_a_struct_is_reported() {
        rejects(
            "struct Point { x: i32 }
             fun f() { let p = Point.x; }",
            "no variant `x`",
        );
    }

    #[test]
    fn a_brace_payload_on_a_value_is_reported() {
        rejects(
            "enum Shape { square: { l: f64 } }
             fun f(s: Shape) { let t = s.square { l: 1.0 }; }",
            "only an enum can be named before",
        );
    }

    #[test]
    fn an_if_condition_has_to_be_a_bool() {
        accepts("fun f(c: bool) { if c {} }");
        rejects("fun f() { if 1 {} }", "mismatched types");
    }

    #[test]
    fn both_branches_of_an_if_expression_have_to_agree() {
        accepts("fun f(c: bool) -> i32 { return if c { 1 } else { 2 }; }");
        rejects(
            "fun f(c: bool) -> i32 { return if c { 1 } else { true }; }",
            "mismatched types",
        );
    }

    #[test]
    fn an_if_without_an_else_produces_nothing() {
        rejects("fun f(c: bool) { if c { 1 } }", "mismatched types");
    }

    #[test]
    fn every_arm_of_a_match_is_checked_against_the_scrutinee() {
        accepts(
            "enum Shape { unit, circle: f64 }
             fun f(s: Shape) -> i32 { return match s { .circle(r) => 1, .unit => 2, }; }",
        );
        rejects(
            "enum Shape { unit, circle: f64 }
             fun f(s: Shape) -> i32 { return match s { .square => 1, .unit => 2, }; }",
            "no variant `square`",
        );
    }

    #[test]
    fn every_arm_of_a_match_has_to_produce_the_same_type() {
        rejects(
            "enum Shape { unit, circle: f64 }
             fun f(s: Shape) { let m = match s { .circle(r) => 1, .unit => true, }; }",
            "mismatched types",
        );
    }

    #[test]
    fn a_match_guard_has_to_be_a_bool() {
        accepts(
            "enum Shape { unit, circle: f64 }
             fun f(s: Shape) -> i32 {
                 return match s { .circle(r) if r > 0.0 => 1, _ => 2, };
             }",
        );
        rejects(
            "enum Shape { unit, circle: f64 }
             fun f(s: Shape) -> i32 {
                 return match s { .circle(r) if r => 1, _ => 2, };
             }",
            "mismatched types",
        );
    }

    #[test]
    fn a_match_guard_sees_its_arms_pattern_bindings() {
        accepts(
            "enum Shape { unit, circle: f64 }
             fun f(s: Shape) -> i32 {
                 return match s { .circle(r) if r > 0.0 => 1, _ => 2, };
             }",
        );
    }

    #[test]
    fn a_variant_pattern_binds_its_payload_at_the_declared_type() {
        accepts(
            "enum Option<T> { some: T, none }
             fun f(o: Option<bool>) -> bool { return match o { .some(v) => v, .none => false, }; }",
        );
        rejects(
            "enum Option<T> { some: T, none }
             fun f(o: Option<bool>) -> i32 { return match o { .some(v) => v, .none => 0, }; }",
            "mismatched types",
        );
    }

    #[test]
    fn dereferencing_a_reference_returns_its_base_type() {
        accepts(
            "module core::ops;
             public trait Copy { fun copy(&self) -> Self; }
             extend i32 with Copy { fun copy(&self) -> Self { return *self; } }
             fun f(p: &i32) -> i32 { return *p; }",
        );
        accepts(
            "module core::ops;
             public trait Copy { fun copy(&self) -> Self; }
             extend i32 with Copy { fun copy(&self) -> Self { return *self; } }
             fun f(p: &mut i32) -> i32 { return *p; }",
        );
    }

    #[test]
    fn dereferencing_a_reference_to_a_reference_peels_one_layer_at_a_time() {
        accepts(
            "module core::ops;
             public trait Copy { fun copy(&self) -> Self; }
             extend i32 with Copy { fun copy(&self) -> Self { return *self; } }
             extend<T> &T with Copy { fun copy(&self) -> Self { return *self; } }
             fun f(p: &&i32) -> i32 { return **p; }",
        );
    }

    #[test]
    fn dereferencing_an_owned_pointer_returns_its_base_type() {
        accepts("fun f(p: iso i32) -> i32 { return *p; }");
    }

    #[test]
    fn dereferencing_a_non_reference_is_rejected() {
        rejects(
            "fun f(x: i32) -> i32 { return *x; }",
            "cannot be dereferenced",
        );
    }

    #[test]
    fn a_dereferenced_reference_is_assignable() {
        accepts("fun f(p: &mut i32) { *p = 1; }");
    }

    #[test]
    fn moving_a_non_copy_value_out_of_a_shared_reference_is_rejected() {
        rejects(
            "struct S { x: i32 }
             fun f(p: &S) -> S { return *p; }",
            "cannot move",
        );
    }

    #[test]
    fn moving_a_non_copy_value_out_of_a_mutable_reference_is_rejected() {
        rejects(
            "struct S { x: i32 }
             fun f(p: &mut S) -> S { return *p; }",
            "cannot move",
        );
    }

    #[test]
    fn moving_a_non_copy_value_out_of_an_owned_pointer_is_fine() {
        accepts(
            "struct S { x: i32 }
             fun f(p: iso S) -> S { return *p; }",
        );
    }

    #[test]
    fn copying_a_copy_value_through_a_reference_is_fine() {
        accepts(
            "module core::ops;
             public trait Copy { fun copy(&self) -> Self; }
             extend i32 with Copy { fun copy(&self) -> Self { return *self; } }
             fun f(p: &i32) -> i32 { return *p; }",
        );
        accepts(
            "module core::ops;
             public trait Copy { fun copy(&self) -> Self; }
             extend i32 with Copy { fun copy(&self) -> Self { return *self; } }
             fun f(p: &mut i32) -> i32 { return *p; }",
        );
    }

    #[test]
    fn reading_a_field_through_a_reference_to_a_non_copy_struct_is_fine() {
        accepts(
            "struct S { x: i32 }
             fun f(p: &S) -> i32 { return (*p).x; }",
        );
    }

    #[test]
    fn assigning_through_a_reference_to_a_non_copy_struct_is_fine() {
        accepts(
            "struct S { x: i32 }
             fun f(p: &mut S, s: S) { *p = s; }",
        );
    }

    #[test]
    fn a_ref_self_method_called_through_an_explicit_deref_is_fine() {
        accepts(
            "struct Counter { n: i32 }
             extend Counter { fun peek(&self) -> i32 { return self.n; } }
             fun f(p: &Counter) -> i32 { return (*p).peek(); }",
        );
    }

    #[test]
    fn a_mut_self_method_called_through_an_explicit_deref_is_fine() {
        accepts(
            "struct Counter { n: i32 }
             extend Counter { fun bump(&mut self) { self.n = self.n + 1; } }
             fun f(p: &mut Counter) { (*p).bump(); }",
        );
    }

    #[test]
    fn a_by_value_self_method_called_through_an_explicit_deref_is_rejected() {
        rejects(
            "struct Counter { n: i32 }
             extend Counter { fun consume(self) {} }
             fun f(p: &Counter) { (*p).consume(); }",
            "cannot move",
        );
    }

    #[test]
    fn moving_the_whole_pointee_out_of_a_mutable_reference_is_rejected() {
        rejects(
            "struct Inner { v: i32 }
             fun f(p: &mut Inner) -> Inner {
                 let q = *p;
                 return q;
             }",
            "cannot move",
        );
    }

    #[test]
    fn an_array_is_indexed_by_an_integer_and_produces_its_element() {
        accepts("fun f(a: [i32; 4]) -> i32 { return a[0]; }");
        rejects(
            "fun f(a: [i32; 4]) -> i32 { return a[true]; }",
            "mismatched types",
        );
    }

    #[test]
    fn a_type_with_an_index_impl_is_indexed_through_it() {
        accepts(
            "module core::ops;

             public trait Index<K, V> { fun index(&self, key: K) -> &V; }

             struct Map { value: bool }

             extend Map with Index<i32, bool> {
                 fun index(&self, key: i32) -> &bool { return &self.value; }
             }

             fun f(m: Map) -> &bool { return m[0]; }",
        );
    }

    #[test]
    fn indexing_a_type_with_no_index_impl_is_reported() {
        rejects(
            "struct Foo { x: i32 }
             fun f(a: Foo) { let b = a[0]; }",
            "cannot be indexed",
        );
    }

    #[test]
    fn try_on_a_result_produces_what_it_carries() {
        accepts(
            "module core::result;

             public enum Result<T, E> { ok: T, err: E }

             fun f(r: Result<i32, bool>) -> Result<i32, bool> {
                 let v = r?;
                 return .ok(v);
             }",
        );
    }

    #[test]
    fn try_checks_the_error_against_the_enclosing_return_type() {
        rejects(
            "module core::result;

             public enum Result<T, E> { ok: T, err: E }

             fun f(r: Result<i32, bool>) -> Result<i32, i32> {
                 let v = r?;
                 return .ok(v);
             }",
            "mismatched types",
        );
    }

    #[test]
    fn try_on_something_that_is_neither_a_result_nor_an_option_is_reported() {
        rejects(
            "struct Foo { x: i32 }
             fun f(a: Foo) { let b = a?; }",
            "`?` cannot be applied to `Foo`",
        );
    }

    #[test]
    fn a_closure_checks_to_a_function_type_of_its_parameters_and_body() {
        accepts(
            "module core::ops;
             public trait Add { fun add(&self, other: &Self) -> Self; }
             public trait Copy { fun copy(&self) -> Self; }
             extend i32 with Add { fun add(&self, other: &Self) -> Self { return *self + *other; } }
             extend i32 with Copy { fun copy(&self) -> Self { return *self; } }
             fun f() { let g: fun(i32) -> i32 = |x: i32| { x + 1 }; }",
        );
        rejects(
            "module core::ops;
             public trait Add { fun add(&self, other: &Self) -> Self; }
             public trait Copy { fun copy(&self) -> Self; }
             extend i32 with Add { fun add(&self, other: &Self) -> Self { return *self + *other; } }
             extend i32 with Copy { fun copy(&self) -> Self { return *self; } }
             fun f() { let g: fun(i32) -> bool = |x: i32| { x + 1 }; }",
            "mismatched types",
        );
    }

    #[test]
    fn an_unannotated_closure_parameter_takes_its_type_from_the_expectation() {
        accepts("fun f() { let g: fun(i64) -> i64 = |x| { x + 1 }; }");
    }

    #[test]
    fn a_closure_is_called_at_its_own_signature() {
        accepts("fun f() -> i32 { let g = |x: i32| { x + 1 }; return g(1); }");
        rejects(
            "fun f() -> i32 { let g = |x: i32| { x + 1 }; return g(true); }",
            "mismatched types",
        );
    }

    #[test]
    fn a_return_inside_a_closure_is_checked_against_the_closures_return_type() {
        accepts("fun f() { let g = |x: i32| -> i32 { return x; }; }");
        rejects(
            "fun f() { let g = |x: i32| -> bool { return x; }; }",
            "mismatched types",
        );
    }

    #[test]
    fn spawn_and_concurrent_run_their_blocks_and_produce_nothing() {
        accepts(
            "fun f() {
                 let mut x = 1;
                 spawn { x = 2; }
                 concurrent { x = 3; }
             }",
        );
        rejects(
            "fun f() { let mut x = 1; spawn { x = true; } }",
            "mismatched types",
        );
    }

    #[test]
    fn new_of_a_value_has_type_iso_of_that_values_type() {
        accepts("fun f() { let x: iso i32 = new 1; }");
    }

    #[test]
    fn new_array_requires_a_usize_count_and_yields_iso_of_unsized_array() {
        accepts("fun f(n: usize) { let buf: iso [u8] = new [0_u8; n]; }");
    }

    #[test]
    fn new_of_a_reference_is_rejected() {
        rejects(
            "fun f(x: &i32) { let y = new x; }",
            "`new` cannot store a reference",
        );
    }

    #[test]
    fn new_of_a_value_that_transitively_holds_a_reference_is_rejected() {
        rejects(
            "fun f(x: &i32) { let y = new (x, 1); }",
            "`new` cannot store a reference",
        );
    }

    #[test]
    fn new_array_with_a_reference_elem_is_rejected() {
        rejects(
            "fun f(x: &i32) { let y = new [x; 1]; }",
            "`new` cannot store a reference",
        );
    }

    #[test]
    fn new_array_with_an_owning_elem_is_rejected() {
        rejects(
            "fun f(n: usize) { let y = new [new 1; n]; }",
            "cannot repeat an owning element",
        );
    }

    #[test]
    fn new_array_with_an_elem_that_owns_a_field_is_rejected() {
        rejects(
            "struct Handle { owned: iso i32 }
             fun f(n: usize) { let y = new [Handle { owned: new 1 }; n]; }",
            "cannot repeat an owning element",
        );
    }

    #[test]
    fn new_array_of_a_plain_element_is_still_accepted() {
        accepts("fun f(n: usize) { let y = new [0_u8; n]; }");
    }

    #[test]
    fn new_of_an_any_typed_value_is_rejected() {
        rejects(
            "fun f(x: any i32) { let y = new x; }",
            "`any` may only appear in a parameter or return type",
        );
    }

    #[test]
    fn new_array_rejects_a_non_usize_count() {
        rejects(
            "fun f() { let buf = new [0_u8; true]; }",
            "mismatched types",
        );
    }

    #[test]
    fn an_unsuffixed_literal_unifies_with_usize() {
        accepts("fun f() { let n: usize = 0; }");
    }

    #[test]
    fn a_usize_suffixed_literal_checks() {
        accepts("fun f() { let n = 0_usize; }");
    }

    #[test]
    fn a_string_literal_has_type_str() {
        accepts("fun f() { let s: str = \"hi\"; }");
    }

    #[test]
    fn str_as_byte_slice_checks() {
        accepts("fun f(s: str) -> &[u8] { return s as &[u8]; }");
    }

    #[test]
    fn byte_slice_as_str_is_rejected() {
        rejects(
            "fun f(b: &[u8]) -> str { return b as str; }",
            "cannot cast a value of type",
        );
    }

    #[test]
    fn a_struct_field_that_is_str_is_rejected() {
        rejects(
            "struct Config { name: str }",
            "a field cannot hold a reference",
        );
    }

    #[test]
    fn an_enum_variant_payload_that_is_str_is_rejected() {
        rejects(
            "enum Opt { some: str, none }",
            "a field cannot hold a reference",
        );
    }

    #[test]
    fn an_array_field_of_str_is_rejected() {
        rejects(
            "struct Wrap { buf: [str; 2] }",
            "a field cannot hold a reference",
        );
    }

    #[test]
    fn instantiating_a_generic_struct_with_str_in_a_field_is_rejected() {
        rejects(
            "struct Boxed<T> { value: T }
             struct Outer { boxed: Boxed<str> }",
            "a field cannot hold a reference",
        );
    }

    #[test]
    fn instantiating_a_generic_struct_with_str_as_a_parameter_checks() {
        accepts(
            "struct Boxed<T> { value: T }
             fun f(b: Boxed<str>) {}",
        );
    }

    #[test]
    fn str_as_a_parameter_checks() {
        accepts("fun greet(name: str) {}");
    }

    #[test]
    fn str_as_a_local_checks() {
        accepts("fun f(s: str) { let t: str = s; }");
    }

    #[test]
    fn str_as_a_return_type_checks() {
        accepts("fun f(s: str) -> str { return s; }");
    }

    const OPTION_AND_RANGE: [&str; 3] = [
        "module core::option; public enum Option<T> { some: T, none }",
        "module core::prelude; import core::option::Option;",
        "module std::range;
         public struct Range<T> {
             public left: Option<T>,
             public right: Option<T>,
             public inclusive: bool,
         }",
    ];

    #[test]
    fn a_range_expr_checks_as_a_std_range() {
        let mut files = OPTION_AND_RANGE.to_vec();
        files.push("fun f() { let r: Range<i32> = 1..2; }");
        assert!(crate::testing::typeck_src_files(&files).is_empty());
    }

    #[test]
    fn an_open_range_bound_checks_as_none() {
        let mut files = OPTION_AND_RANGE.to_vec();
        files.push("fun f() { let r: Range<i32> = ..2; }");
        assert!(crate::testing::typeck_src_files(&files).is_empty());
    }

    #[test]
    fn mismatched_range_endpoints_are_reported() {
        let mut files = OPTION_AND_RANGE.to_vec();
        files.push("fun f() { let r = 1..2.0; }");
        let reported = crate::testing::typeck_src_files(&files);
        assert_eq!(reported.len(), 1, "{reported:?}");
        assert!(reported[0].contains("mismatched types"));
    }

    #[test]
    fn a_while_conditions_type_has_to_be_bool() {
        accepts(
            "module core::ops;
             public trait Not { fun not(&self) -> Self; }
             public trait Copy { fun copy(&self) -> Self; }
             extend bool with Not { fun not(&self) -> Self { return !*self; } }
             extend bool with Copy { fun copy(&self) -> Self { return *self; } }
             fun f(c: bool) { while c {} }",
        );
        rejects(
            "module core::ops;
             public trait Not { fun not(&self) -> Self; }
             public trait Copy { fun copy(&self) -> Self; }
             extend bool with Not { fun not(&self) -> Self { return !*self; } }
             extend bool with Copy { fun copy(&self) -> Self { return *self; } }
             fun f(x: i32) { while x {} }",
            "does not implement `Not`",
        );
    }

    #[test]
    fn a_loop_nested_inside_another_loop_checks() {
        accepts("fun f(a: bool, b: bool) { while a { while b {} } }");
    }

    #[test]
    fn a_while_let_matches_against_an_enum_and_binds_its_payload() {
        accepts(
            "enum Shape { unit, circle: f64 }
             fun f(s: Shape) {
                 while let .circle(r) = s {
                     let y: f64 = r;
                 }
             }",
        );
        rejects(
            "enum Shape { unit, circle: f64 }
             fun f(s: Shape) {
                 while let .circle(r) = s {
                     let y: bool = r;
                 }
             }",
            "mismatched types",
        );
    }

    #[test]
    fn a_for_loop_binds_its_pattern_to_the_iterators_item_type() {
        accepts(
            "module core::option;
             public enum Option<T> { some: T, none }
             struct Counter { n: i32 }
             extend Counter { fun next(&mut self) -> Option<i32> { return .none; } }
             fun f() {
                 let c = Counter { n: 0 };
                 for x in c { let y: i32 = x; }
             }",
        );
        rejects(
            "module core::option;
             public enum Option<T> { some: T, none }
             struct Counter { n: i32 }
             extend Counter { fun next(&mut self) -> Option<i32> { return .none; } }
             fun f() {
                 let c = Counter { n: 0 };
                 for x in c { let y: bool = x; }
             }",
            "mismatched types",
        );
    }

    #[test]
    fn method_calls_chain_left_to_right() {
        accepts(
            "struct A {}
             struct B {}
             struct C {}
             extend A { fun to_b(self) -> B { return .{}; } }
             extend B { fun to_c(self) -> C { return .{}; } }
             fun f(a: A) -> C { return a.to_b().to_c(); }",
        );
    }

    #[test]
    fn a_temporary_receiver_needing_a_place_is_rejected() {
        rejects(
            "struct A {}
             struct B {}
             extend A { fun to_b(self) -> B { return .{}; } }
             extend B { fun show(&self) {} }
             fun f(a: A) { a.to_b().show(); }",
            "receiver is a temporary",
        );
    }

    #[test]
    fn a_nested_generic_struct_literal_checks() {
        accepts(
            "struct Wrap<T> { inner: T }
             fun f() -> Wrap<Wrap<i32>> {
                 return Wrap { inner: Wrap { inner: 1 } };
             }",
        );
        rejects(
            "struct Wrap<T> { inner: T }
             fun f() -> Wrap<Wrap<i32>> {
                 return Wrap { inner: Wrap { inner: true } };
             }",
            "mismatched types",
        );
    }

    #[test]
    fn deeply_nested_if_and_match_expressions_share_one_result_type() {
        accepts(
            "enum Shape { unit, circle: f64 }
             fun f(s: Shape, c: bool) -> i32 {
                 return if c {
                     match s {
                         .unit => 1,
                         .circle(r) => if r > 0.0 { 2 } else { 3 },
                     }
                 } else {
                     4
                 };
             }",
        );
    }

    #[test]
    fn try_on_an_option_produces_what_it_carries() {
        accepts(
            "module core::option;
             public enum Option<T> { some: T, none }
             fun f(o: Option<i32>) -> Option<i32> {
                 let v = o?;
                 return .some(v);
             }",
        );
    }

    #[test]
    fn a_closure_may_capture_an_enclosing_closures_parameter() {
        accepts(
            "fun f() {
                 let make_adder = |x: i32| {
                     let adder = |y: i32| { x + y };
                     adder
                 };
             }",
        );
    }

    #[test]
    fn assignment_through_a_field_checks_against_the_fields_type() {
        accepts("struct P { x: i32 } fun f(p: P) { p.x = 1; }");
        rejects(
            "struct P { x: i32 } fun f(p: P) { p.x = true; }",
            "mismatched types",
        );
    }

    #[test]
    fn assignment_through_an_index_checks_against_the_elements_type() {
        accepts("fun f(a: [i32; 4]) { a[0] = 1; }");
        rejects("fun f(a: [i32; 4]) { a[0] = true; }", "mismatched types");
    }

    #[test]
    fn a_unit_struct_constructs_and_checks() {
        accepts("struct Unit {} fun f() -> Unit { return Unit {}; }");
    }

    #[test]
    fn a_widening_int_cast_is_accepted() {
        accepts("fun f() { let x: i8 = 1; let y = x as i64; }");
    }

    #[test]
    fn a_narrowing_int_cast_is_rejected() {
        rejects(
            "fun f() { let x: i64 = 1; let y = x as i8; }",
            "cannot cast",
        );
    }

    #[test]
    fn unsigned_to_a_strictly_wider_signed_type_is_accepted() {
        accepts("fun f() { let x = 1_u8; let y = x as i16; }");
    }

    #[test]
    fn unsigned_to_an_equal_width_signed_type_is_rejected() {
        rejects("fun f() { let x = 1_u8; let y = x as i8; }", "cannot cast");
    }

    #[test]
    fn signed_to_unsigned_is_always_rejected() {
        rejects(
            "fun f() { let x: i8 = 1; let y = x as u64; }",
            "cannot cast",
        );
    }

    #[test]
    fn a_narrow_int_casts_to_either_float() {
        accepts("fun f() { let x: i16 = 1; let y = x as f32; }");
    }

    #[test]
    fn a_32_bit_int_only_widens_to_f64() {
        accepts("fun f() { let x: i32 = 1; let y = x as f64; }");
        rejects(
            "fun f() { let x: i32 = 1; let y = x as f32; }",
            "cannot cast",
        );
    }

    #[test]
    fn a_64_bit_int_casts_to_no_float() {
        rejects(
            "fun f() { let x: i64 = 1; let y = x as f64; }",
            "cannot cast",
        );
    }

    #[test]
    fn f32_widens_to_f64_but_not_back() {
        accepts("fun f() { let x: f32 = 1.0; let y = x as f64; }");
        rejects(
            "fun f() { let x: f64 = 1.0; let y = x as f32; }",
            "cannot cast",
        );
    }

    #[test]
    fn float_to_int_is_always_rejected() {
        rejects(
            "fun f() { let x: f32 = 1.0; let y = x as i32; }",
            "cannot cast",
        );
    }

    #[test]
    fn bool_casts_to_a_numeric_type_but_not_back() {
        accepts("fun f() { let x = true; let y = x as i32; }");
        rejects(
            "fun f() { let x: i32 = 1; let y = x as bool; }",
            "cannot cast",
        );
    }

    #[test]
    fn char_casts_to_32_or_64_bit_integers_but_not_narrower() {
        accepts("fun f() { let x = 'a'; let y = x as i32; }");
        rejects("fun f() { let x = 'a'; let y = x as i8; }", "cannot cast");
    }

    #[test]
    fn only_u8_casts_to_char() {
        accepts("fun f() { let x = 1_u8; let y = x as char; }");
        rejects(
            "fun f() { let x: i32 = 1; let y = x as char; }",
            "cannot cast",
        );
    }

    #[test]
    fn an_identity_cast_is_accepted() {
        accepts("fun f() { let x: i32 = 1; let y = x as i32; }");
    }

    #[test]
    fn casting_to_a_non_primitive_type_is_rejected() {
        rejects(
            "struct Point { x: i32 } fun f() { let p: i32 = 1; let q = p as Point; }",
            "cannot cast",
        );
    }

    #[test]
    fn casting_a_non_primitive_value_is_rejected() {
        rejects(
            "struct Point { x: i32 } fun f(p: Point) { let q = p as i32; }",
            "cannot cast",
        );
    }

    #[test]
    fn an_unconstrained_int_literal_casts_directly_to_any_integer_type() {
        accepts("fun f() -> i64 { let x = 1; return x as i64; }");
    }

    #[test]
    fn an_unconstrained_int_literal_cannot_cast_across_families() {
        rejects(
            "fun f() { let x = 1; let y = x as f64; }",
            "type annotations needed",
        );
    }

    #[test]
    fn a_literal_suffix_lets_a_literal_cast_across_families() {
        accepts("fun f() { let y = 1_i32 as f64; }");
    }

    #[test]
    fn chained_casts_check_left_to_right() {
        accepts("fun f() { let x: i8 = 1; let y = x as i32 as i64; }");
    }

    #[test]
    fn assert_accepts_a_bool_condition() {
        accepts("fun f(x: bool) { assert(x); }");
    }

    #[test]
    fn assert_rejects_a_non_bool_condition() {
        rejects("fun f(x: i32) { assert(x); }", "mismatched types");
    }

    #[test]
    fn assert_accepts_a_str_message() {
        accepts(r#"fun f(x: bool) { assert(x, "x must hold"); }"#);
    }

    #[test]
    fn assert_rejects_a_non_str_message() {
        rejects("fun f(x: bool) { assert(x, 1); }", "mismatched types");
    }

    #[test]
    fn panic_rejects_a_non_str_message() {
        rejects("fun f() { panic(1); }", "mismatched types");
    }

    #[test]
    fn unreachable_rejects_a_non_str_message() {
        rejects("fun f() { unreachable(1); }", "mismatched types");
    }

    #[test]
    fn panic_unifies_with_any_expected_type() {
        accepts("fun f() -> i32 { if true { return 1; } panic(); }");
    }

    #[test]
    fn a_borrow_coerces_to_a_dyn_parameter() {
        accepts(
            "trait Sh { fun sh(&self) -> i32; }
                 struct W { v: i32 }
                 extend W with Sh { fun sh(&self) -> i32 { return self.v; } }
                 fun draw(s: &dyn Sh) -> i32 { return s.sh(); }
                 fun f() -> i32 { let w: W = W { v: 1 }; return draw(&w); }",
        );
    }

    #[test]
    fn a_borrow_coerces_to_dyn_for_a_let_binding_and_a_return() {
        accepts(
            "trait Sh { fun sh(&self) -> i32; }
                 struct W { v: i32 }
                 extend W with Sh { fun sh(&self) -> i32 { return self.v; } }
                 fun f() -> &dyn Sh {
                     let w: W = W { v: 1 };
                     let d: &dyn Sh = &w;
                     let g: &dyn Sh = d;
                     return d;
                 }",
        );
    }

    #[test]
    fn a_mutable_borrow_coerces_to_a_mutable_dyn() {
        accepts(
            "trait Sh { fun sh(&mut self) -> i32; }
                 struct W { v: i32 }
                 extend W with Sh { fun sh(&mut self) -> i32 { return self.v; } }
                 fun f(w: &mut W) -> i32 { let d: &mut dyn Sh = w; return d.sh(); }",
        );
        rejects(
            "trait Sh { fun sh(&self) -> i32; }
                 struct W { v: i32 }
                 extend W with Sh { fun sh(&self) -> i32 { return self.v; } }
                 fun f(w: &mut W) -> i32 { let d: &mut dyn Sh = &w; return d.sh(); }",
            "mismatched types",
        );
    }

    #[test]
    fn a_type_without_the_trait_does_not_coerce_to_dyn() {
        rejects(
            "trait Sh { fun sh(&self) -> i32; }
                 struct W { v: i32 }
                 fun draw(s: &dyn Sh) -> i32 { return s.sh(); }
                 fun f() -> i32 { let w: W = W { v: 1 }; return draw(&w); }",
            "mismatched types",
        );
    }

    #[test]
    fn a_dyn_receiver_cannot_call_a_method_taking_self_by_value() {
        let reported = crate::testing::typeck_src(
            "trait Sh { fun sh(self) -> i32; }
                 struct W { v: i32 }
                 extend W with Sh { fun sh(self) -> i32 { return self.v; } }
                 fun draw(s: &dyn Sh) -> i32 { return s.sh(); }",
        );
        assert!(
            reported
                .iter()
                .any(|message| message.contains("cannot be called through a `dyn` receiver")),
            "expected the dyn-receiver diagnostic, got {reported:?}"
        );
    }

    #[test]
    fn a_dyn_receiver_cannot_call_a_method_returning_self() {
        let reported = crate::testing::typeck_src(
            "trait Sh { fun sh(&self) -> Self; }
                 struct W { v: i32 }
                 extend W with Sh { fun sh(&self) -> Self { return *self; } }
                 fun draw(s: &dyn Sh) -> W { return s.sh(); }",
        );
        assert!(
            reported
                .iter()
                .any(|message| message.contains("cannot be called through a `dyn` receiver")),
            "expected the dyn-receiver diagnostic, got {reported:?}"
        );
    }

    #[test]
    fn the_two_sides_of_a_compound_assignment_must_have_the_same_type() {
        rejects("fun f() { let mut x = 1; x += true; }", "mismatched types");
    }

    #[test]
    fn a_compound_assignment_whose_operator_produces_another_type_is_reported() {
        rejects(
            "module core::ops;
             public trait Add { fun add(&self, other: &Self) -> Self; }
             struct N { v: i32 }
             extend N with Add { fun add(&self, other: &Self) -> Self { return N { v: self.v }; } }
             fun f(x: any N, y: any N) { x += y; }",
            "expected `any N`, found `N`",
        );
    }

    #[test]
    fn indexing_a_base_of_unknown_type_is_reported() {
        rejects(
            "fun f() { let g = |x| { x[0] }; }",
            "the type being indexed is still unknown",
        );
    }

    #[test]
    fn building_with_a_path_that_names_an_enum_is_reported() {
        rejects(
            "enum E { a }
             fun f() -> E { return E { x: 1 }; }",
            "only a struct can be built with `{ .. }`",
        );
    }

    #[test]
    fn an_elided_literal_expecting_an_enum_is_reported() {
        rejects(
            "enum E { a }
             fun f() -> E { return .{ x: 1 }; }",
            "`E` is not a struct",
        );
    }

    #[test]
    fn a_record_variant_field_that_is_not_declared_is_reported() {
        rejects(
            "enum E { v: { a: i32 } }
             fun f() -> E { return .v { a: 1, b: 2 }; }",
            "no field `b` on this variant",
        );
    }

    #[test]
    fn a_record_variant_missing_a_declared_field_is_reported() {
        rejects(
            "enum E { v: { a: i32, b: bool } }
             fun f() -> E { return .v { a: 1 }; }",
            "this variant's payload is missing field `b`",
        );
    }

    #[test]
    fn try_on_an_operand_of_unknown_type_is_reported() {
        rejects(
            "fun f() { let g = |x| { x? }; }",
            "the type `?` is applied to is still unknown",
        );
    }

    #[test]
    fn try_with_no_enclosing_return_type_is_reported() {
        rejects(
            "module core::result;
             public enum Result<T, E> { ok: T, err: E }
             fun f(r: Result<i32, bool>) { let v = r?; }",
            "has nowhere to propagate to",
        );
    }

    #[test]
    fn try_whose_enclosing_return_cannot_carry_it_is_reported() {
        rejects(
            "module core::result;
             public enum Result<T, E> { ok: T, err: E }
             fun f(r: Result<i32, bool>) -> i32 { let v = r?; return v; }",
            "cannot propagate out of a function returning `i32`",
        );
    }
}

impl<'hir> Typeck<'hir> {
    #[must_use]
    pub(crate) fn check_expr(&mut self, id: impl Into<HirId>, expected: Option<Ty>) -> Ty {
        let id = id.into();
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
                Res::Type(_) | Res::SelfTy(_) => unreachable!(
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
        if matches!(self.tcx.kind(self_ty), TyKind::Var(InferVar::Any(_))) {
            report_operand_has_unknown_type(self.session, span);
            return false;
        }
        let Some(def) = self.hir.lang_items().get(item) else {
            return false;
        };

        let goal = Goal::new(self_ty, def);
        let env = self.bounds_env(owner);
        match self.implements(&goal, &env) {
            Solution::Holds => true,
            Solution::DoesNotHold => {
                let name = crate::diagnostics::display::def_name(self.session, self.hir, def);
                report_operator_trait_missing(self.display_cx(), self_ty, name, span);
                false
            }
            Solution::Ambiguous | Solution::Error => false,
        }
    }

    fn is_undefaulted_numeric_var(&self, ty: Ty) -> bool {
        matches!(
            self.tcx.kind(ty),
            TyKind::Var(InferVar::Int(_) | InferVar::Float(_))
        )
    }

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
                Some(suffix) => match is_prim_ty(self.session, *suffix) {
                    Some(prim) if prim.is_integer() || prim.is_float() => self.tcx.mk_prim(prim),
                    _ => {
                        report_unknown_literal_suffix(self.session, *suffix, span);
                        self.tcx.error()
                    }
                },
            },
            Literal::Float { suffix, .. } => match suffix {
                None => self.tcx.next_float_var(),
                Some(suffix) => match is_prim_ty(self.session, *suffix) {
                    Some(prim) if prim.is_float() => self.tcx.mk_prim(prim),
                    Some(prim) if prim.is_integer() => {
                        report_int_suffix_on_float_literal(self.session, *suffix, span);
                        self.tcx.error()
                    }
                    _ => {
                        report_unknown_literal_suffix(self.session, *suffix, span);
                        self.tcx.error()
                    }
                },
            },
            Literal::Str(_) => self.tcx.mk_prim(PrimTy::Str),
        }
    }
}
