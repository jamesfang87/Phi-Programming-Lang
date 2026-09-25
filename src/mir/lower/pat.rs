use crate::ast::{BinaryOp, Ident, Literal, Mutability};
use crate::driver::source::SrcSpan;
use crate::hir::{ArmId, BindingMode, ExprId, HirId, PatId, PatKind, Payload};
use crate::mir::lower::ctx::{BodyLowerCtx, ExitObligation};
use crate::mir::{
    BasicBlock, ConstKind, Constant, Operand, Place, Projection, Rvalue, StatementKind,
    SwitchTargets, TerminatorKind, VariantIdx,
};
use crate::nameres::PrimTy;
use crate::typeck::ty::{Ty, TyKind};

impl<'a> BodyLowerCtx<'a> {
    pub(crate) fn lower_if_into(
        &mut self,
        cond: impl Into<HirId>,
        then_block: impl Into<HirId>,
        else_block: Option<impl Into<HirId>>,
        dest: Place,
        span: SrcSpan,
    ) {
        let (cond, then_block) = (cond.into(), then_block.into());
        let else_block = else_block.map(Into::into);
        let cond_operand = self.lower_operand(cond);
        let then_start = self.new_block();
        let else_start = self.new_block();
        let join = self.new_block();
        self.set_terminator(
            TerminatorKind::SwitchInt {
                discr: cond_operand,
                targets: SwitchTargets {
                    values: vec![(1, then_start)],
                    otherwise: else_start,
                },
            },
            span,
        );

        self.switch_to(then_start);
        self.lower_block(then_block, Some(dest.clone()));
        self.set_terminator(TerminatorKind::Goto { target: join }, span);

        self.switch_to(else_start);
        match else_block {
            Some(else_id) => self.lower_block(else_id, Some(dest.clone())),
            None => self.assign_unit(dest.clone(), span),
        }
        self.set_terminator(TerminatorKind::Goto { target: join }, span);

        self.switch_to(join);
    }

    pub(crate) fn lower_loop_into(&mut self, block: impl Into<HirId>, dest: Place, span: SrcSpan) {
        let block = block.into();
        let body_start = self.new_block();
        let break_target = self.new_block();

        self.set_terminator(TerminatorKind::Goto { target: body_start }, span);

        self.switch_to(body_start);
        self.push_loop(break_target, body_start);
        self.lower_block(block, None);
        self.pop_loop();
        self.set_terminator(TerminatorKind::Goto { target: body_start }, span);

        self.switch_to(break_target);
        self.assign_unit(dest, span);
    }

    pub(crate) fn lower_match_into(
        &mut self,
        scrutinee: impl Into<HirId>,
        arms: &[ArmId],
        dest: Place,
    ) {
        let scrutinee = scrutinee.into();
        let span = self.hir.expr(scrutinee).span;
        let scrutinee_ty = self.expr_ty(scrutinee);
        let scrutinee_local = self.new_temp(scrutinee_ty, span);
        let scrutinee_place = Place::from_local(scrutinee_local);
        let operand = self.lower_operand(scrutinee);
        self.assign(scrutinee_place.clone(), Rvalue::Use(operand), span);

        let join = self.new_block();
        let starts: Vec<BasicBlock> = arms.iter().map(|_| self.new_block()).collect();
        let no_match = self.new_block();

        let first = starts.first().copied().unwrap_or(no_match);
        self.set_terminator(TerminatorKind::Goto { target: first }, span);

        for (i, &arm_id) in arms.iter().enumerate() {
            self.switch_to(starts[i]);
            let next = starts.get(i + 1).copied().unwrap_or(no_match);
            self.lower_match_arm(arm_id, scrutinee_place.clone(), dest.clone(), next, join);
        }

        self.switch_to(no_match);
        self.set_terminator(TerminatorKind::Unreachable, span);

        self.switch_to(join);
    }

    /// Lowers one match arm: tests its pattern, binds its names, runs its guard and body, and
    /// jumps to `join`. A failed test or guard jumps to `next`, the arm after it.
    fn lower_match_arm(
        &mut self,
        arm_id: ArmId,
        scrutinee_place: Place,
        dest: Place,
        next: BasicBlock,
        join: BasicBlock,
    ) {
        let arm = self.hir.arm(arm_id);
        let (pat, guard, body, arm_span) = (arm.pat, arm.guard, arm.block, arm.span);

        self.test_pat(pat, scrutinee_place.clone(), next);
        self.push_block_scope();
        self.bind_pat(pat, scrutinee_place);

        if let Some(guard_id) = guard {
            self.lower_match_guard(guard_id, next);
        }

        self.lower_block(body, Some(dest));
        let obligations = self.pop_block_scope();
        self.replay_obligations(&obligations);
        self.set_terminator(TerminatorKind::Goto { target: join }, arm_span);
    }

    /// Lowers an arm guard, jumping to `next` when it evaluates false.
    fn lower_match_guard(&mut self, guard_id: ExprId, next: BasicBlock) {
        let guard_span = self.hir.expr(guard_id).span;
        let cond = self.lower_operand(guard_id);
        let guard_ok = self.new_block();
        let guard_fail = self.new_block();
        self.set_terminator(
            TerminatorKind::SwitchInt {
                discr: cond,
                targets: SwitchTargets {
                    values: vec![(1, guard_ok)],
                    otherwise: guard_fail,
                },
            },
            guard_span,
        );

        self.switch_to(guard_fail);
        let peeked = self.peek_block_scope();
        self.replay_obligations(&peeked);
        self.set_terminator(TerminatorKind::Goto { target: next }, guard_span);

        self.switch_to(guard_ok);
    }

    pub(crate) fn test_pat(&mut self, pat_id: impl Into<HirId>, place: Place, fail: BasicBlock) {
        let pat_id = pat_id.into();
        let pat = self.hir.pat(pat_id);
        let span = pat.span;
        let place = self.place_with_pat_adjust(pat_id, place);
        match &pat.kind {
            PatKind::Wildcard | PatKind::Binding { .. } => {}
            PatKind::Literal(lit) => self.test_literal_pat(pat_id, *lit, place, fail, span),
            PatKind::Variant { variant, payload } => {
                self.test_variant_pat(pat_id, *variant, payload, place, fail, span)
            }
            PatKind::Tuple(elems) => self.test_tuple_pat(elems, place, fail),
            PatKind::Error => unreachable!("a fully type-checked body contains no PatKind::Error"),
        }
    }

    fn test_literal_pat(
        &mut self,
        pat_id: HirId,
        lit: Literal,
        place: Place,
        fail: BasicBlock,
        span: SrcSpan,
    ) {
        let ty = self.pat_ty(pat_id);
        let constant = self.lower_pat_literal(lit, ty);
        let operand = self.operand_for_place(place, ty);
        let bool_ty = self.tcx.mk_prim(PrimTy::Bool);
        let eq_local = self.new_temp(bool_ty, span);
        self.assign(
            Place::from_local(eq_local),
            Rvalue::BinaryOp(BinaryOp::Eq, operand, Operand::Constant(constant)),
            span,
        );
        let cont = self.new_block();
        self.set_terminator(
            TerminatorKind::SwitchInt {
                discr: Operand::Copy(Place::from_local(eq_local)),
                targets: SwitchTargets {
                    values: vec![(1, cont)],
                    otherwise: fail,
                },
            },
            span,
        );
        self.switch_to(cont);
    }

    fn test_variant_pat(
        &mut self,
        pat_id: HirId,
        variant: Ident,
        payload: &'a Payload,
        place: Place,
        fail: BasicBlock,
        span: SrcSpan,
    ) {
        let ty = self.pat_ty(pat_id);
        let (_, variant_idx) = self.variant_idx_for(ty, variant.text);
        let i32_ty = self.tcx.mk_prim(PrimTy::I32);
        let discr_local = self.new_temp(i32_ty, span);
        self.assign(
            Place::from_local(discr_local),
            Rvalue::Discriminant(place.clone()),
            span,
        );
        let cont = self.new_block();
        self.set_terminator(
            TerminatorKind::SwitchInt {
                discr: Operand::Copy(Place::from_local(discr_local)),
                targets: SwitchTargets {
                    values: vec![(variant_idx.index() as u128, cont)],
                    otherwise: fail,
                },
            },
            span,
        );
        self.switch_to(cont);

        let mut payload_place = place;
        payload_place
            .projections
            .push(Projection::Downcast(variant_idx));
        self.test_payload(ty, variant_idx, payload, payload_place, fail);
    }

    fn test_tuple_pat(&mut self, elems: &[PatId], place: Place, fail: BasicBlock) {
        for (i, &elem) in elems.iter().enumerate() {
            let mut elem_place = place.clone();
            elem_place.projections.push(Projection::Field(i as u32));
            self.test_pat(elem, elem_place, fail);
        }
    }

    fn test_payload(
        &mut self,
        enum_ty: Ty,
        variant_idx: VariantIdx,
        payload: &Payload,
        base: Place,
        fail: BasicBlock,
    ) {
        match payload {
            Payload::None => {}
            Payload::Single(pat_id) => {
                let mut field_place = base;
                field_place.projections.push(Projection::Field(0));
                self.test_pat(*pat_id, field_place, fail);
            }
            Payload::Record(fields) => {
                for field in fields {
                    let index = self.find_record_field_index(enum_ty, variant_idx, field.name.text);
                    let mut field_place = base.clone();
                    field_place.projections.push(Projection::Field(index));
                    self.test_pat(field.value, field_place, fail);
                }
            }
        }
    }

    pub(crate) fn bind_pat(&mut self, pat_id: impl Into<HirId>, place: Place) {
        let pat_id = pat_id.into();
        let pat = self.hir.pat(pat_id);
        let span = pat.span;
        let mode = self.types.pat_adjust(pat_id).mode;
        let place = self.place_with_pat_adjust(pat_id, place);
        match &pat.kind {
            PatKind::Wildcard | PatKind::Literal(_) => {}
            PatKind::Binding { name, .. } => {
                self.bind_binding_pat(pat_id, *name, mode, place, span)
            }
            PatKind::Variant { variant, payload } => {
                self.bind_variant_pat(pat_id, *variant, payload, place)
            }
            PatKind::Tuple(elems) => self.bind_tuple_pat(elems, place),
            PatKind::Error => unreachable!("a fully type-checked body contains no PatKind::Error"),
        }
    }

    fn bind_binding_pat(
        &mut self,
        pat_id: HirId,
        name: Ident,
        mode: BindingMode,
        place: Place,
        span: SrcSpan,
    ) {
        let ty = self.pat_ty(pat_id);
        let local = self.new_local(ty, Some(name), span);
        self.push_stmt(StatementKind::StorageLive(local), span);
        let rvalue = match mode {
            BindingMode::Ref => Rvalue::Ref {
                mutability: Mutability::Immutable,
                place,
            },
            BindingMode::RefMut => Rvalue::Ref {
                mutability: Mutability::Mutable,
                place,
            },
            BindingMode::Value => Rvalue::Use(self.operand_for_place(place, ty)),
        };
        self.assign(Place::from_local(local), rvalue, span);
        self.bind_local(pat_id, local);
        self.register_exit_obligation(ExitObligation::StorageDead(local));
    }

    fn bind_variant_pat(
        &mut self,
        pat_id: HirId,
        variant: Ident,
        payload: &'a Payload,
        place: Place,
    ) {
        let ty = self.pat_ty(pat_id);
        let (_, variant_idx) = self.variant_idx_for(ty, variant.text);
        let mut payload_place = place;
        payload_place
            .projections
            .push(Projection::Downcast(variant_idx));
        self.bind_payload(ty, variant_idx, payload, payload_place);
    }

    fn bind_tuple_pat(&mut self, elems: &[PatId], place: Place) {
        for (i, &elem) in elems.iter().enumerate() {
            let mut elem_place = place.clone();
            elem_place.projections.push(Projection::Field(i as u32));
            self.bind_pat(elem, elem_place);
        }
    }

    /// Returns `place` with the dereferences `pat_id`'s binding adjustment strips from its
    /// matched scrutinee.
    fn place_with_pat_adjust(&self, pat_id: HirId, place: Place) -> Place {
        let derefs = self.types.pat_adjust(pat_id).derefs;
        let mut adjusted = place;
        for _ in 0..derefs {
            adjusted.projections.push(Projection::Deref);
        }
        adjusted
    }

    fn bind_payload(
        &mut self,
        enum_ty: Ty,
        variant_idx: VariantIdx,
        payload: &Payload,
        base: Place,
    ) {
        match payload {
            Payload::None => {}
            Payload::Single(pat_id) => {
                let mut field_place = base;
                field_place.projections.push(Projection::Field(0));
                self.bind_pat(*pat_id, field_place);
            }
            Payload::Record(fields) => {
                for field in fields {
                    let index = self.find_record_field_index(enum_ty, variant_idx, field.name.text);
                    let mut field_place = base.clone();
                    field_place.projections.push(Projection::Field(index));
                    self.bind_pat(field.value, field_place);
                }
            }
        }
    }

    pub(crate) fn pat_ty(&mut self, pat_id: impl Into<HirId>) -> Ty {
        let pat_id = pat_id.into();
        let ty = self
            .types
            .ty(pat_id)
            .unwrap_or_else(|| panic!("mir::lower: {pat_id:?} has no recorded type"));
        self.resolve_any(ty, self.any_mode)
    }

    fn lower_pat_literal(&mut self, lit: Literal, ty: Ty) -> Constant {
        let kind = match lit {
            Literal::Int { value, .. } => {
                ConstKind::Int(self.session.resolve(value).parse().unwrap_or_else(|_| {
                    panic!("mir::lower: integer pattern literal does not parse")
                }))
            }
            Literal::Float { value, .. } => ConstKind::Float(
                self.session
                    .resolve(value)
                    .parse()
                    .unwrap_or_else(|_| panic!("mir::lower: float pattern literal does not parse")),
            ),

            Literal::Str(_) => unreachable!("typeck rejects string literal patterns"),
            Literal::Bool(b) => ConstKind::Bool(b),
            Literal::Char(c) => ConstKind::Char(c),
        };
        Constant { ty, kind }
    }

    pub(crate) fn variant_idx_for(
        &self,
        ty: Ty,
        name: crate::ast::interner::Symbol,
    ) -> (crate::hir::DefId, VariantIdx) {
        let TyKind::Adt { def, .. } = *self.tcx.kind(ty) else {
            panic!("mir::lower: a variant pattern/expression's type is not an enum")
        };
        let e = self.hir.enum_(def);
        let index = e
            .variants
            .iter()
            .position(|&v| self.hir.variant(v).name.text == name)
            .unwrap_or_else(|| panic!("mir::lower: enum has no variant matching {name:?}"));
        (def, VariantIdx::from_usize(index))
    }

    fn find_record_field_index(
        &self,
        enum_ty: Ty,
        variant_idx: VariantIdx,
        name: crate::ast::interner::Symbol,
    ) -> u32 {
        let TyKind::Adt { def, .. } = *self.tcx.kind(enum_ty) else {
            panic!("mir::lower: a record payload's enum type is not an Adt")
        };
        let e = self.hir.enum_(def);
        let variant_hir_id = e.variants[variant_idx.index()];
        let crate::hir::VariantPayload::Record(fields) = &self.hir.variant(variant_hir_id).payload
        else {
            panic!("mir::lower: a record payload pattern's variant is not declared as a record")
        };
        fields
            .iter()
            .position(|&f| self.hir.field(f).name.text == name)
            .unwrap_or_else(|| panic!("mir::lower: variant record has no field matching {name:?}"))
            as u32
    }
}
