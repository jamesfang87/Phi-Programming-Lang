//! `if`/`match`/`loop` lowering and the pattern decision-tree compiler.
//!
//! A `match` (and `if`, its two-arm cousin already split apart at the HIR level) compiles to a
//! sequence of *candidates*, one per arm, tested in source order: [`BodyLowerCtx::test_pat`]
//! only tests a pattern's structure, and [`BodyLowerCtx::bind_pat`] -- run only once a
//! candidate's structure has already fully matched -- is a separate walk over the same pattern
//! that performs the actual bindings. Splitting the two avoids ever having to unwind a partial
//! binding when a *later* part of the same pattern goes on to refute: nothing binds until
//! everything already matched.
//!
//! This is the first version's straightforward *sequential* candidate chain (test arm 1's tests
//! in full; on any refutation, fall to arm 2; and so on), not rustc's shared/merged decision
//! tree that reorders and shares common prefix tests across arms for efficiency.

use crate::ast::{BinaryOp, Literal};
use crate::driver::source::SrcSpan;
use crate::hir::{HirId, PatKind, Payload};
use crate::mir::lower::ctx::{BodyLowerCtx, ExitObligation};
use crate::mir::{
    BasicBlock, ConstKind, Constant, Operand, Place, PlaceElem, Rvalue, StatementKind,
    SwitchTargets, TerminatorKind, VariantIdx,
};
use crate::nameres::PrimTy;
use crate::typeck::ty::{Ty, TyKind};

impl<'a> BodyLowerCtx<'a> {
    // -----------------------------------------------------------------
    // `if`
    // -----------------------------------------------------------------

    pub(crate) fn lower_if_into(
        &mut self,
        cond: HirId,
        then_block: HirId,
        else_block: Option<HirId>,
        dest: Place,
        span: SrcSpan,
    ) {
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

    // -----------------------------------------------------------------
    // `loop`
    // -----------------------------------------------------------------

    pub(crate) fn lower_loop_into(&mut self, block: HirId, dest: Place, span: SrcSpan) {
        let body_start = self.new_block();
        let break_target = self.new_block();

        self.set_terminator(TerminatorKind::Goto { target: body_start }, span);

        self.switch_to(body_start);
        self.push_loop(break_target, body_start);
        self.lower_block(block, None);
        self.pop_loop();
        self.set_terminator(TerminatorKind::Goto { target: body_start }, span);

        // Every `break` jumps here directly; `Break` carries no value, so the loop's own value
        // (when it is used as one) is always unit, assigned once here rather than at every
        // `break` site.
        self.switch_to(break_target);
        self.assign_unit(dest, span);
    }

    // -----------------------------------------------------------------
    // `match`
    // -----------------------------------------------------------------

    pub(crate) fn lower_match_into(&mut self, scrutinee: HirId, arms: &[HirId], dest: Place) {
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
            let arm = self.hir.arm(arm_id);
            let (pat, guard, body, arm_span) = (arm.pat, arm.guard, arm.block, arm.span);
            let next = starts.get(i + 1).copied().unwrap_or(no_match);

            self.test_pat(pat, scrutinee_place.clone(), next);

            self.push_block_scope();
            // A `match` arm's binding has no `mut` syntax of its own, so, like a `for` binding
            // or a `with` lend, it is left unrestricted; see `StatementKind::CheckMutable`'s own
            // docs.
            self.bind_pat(
                pat,
                scrutinee_place.clone(),
                crate::ast::Mutability::Mutable,
            );

            if let Some(guard_id) = guard {
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

            self.lower_block(body, Some(dest.clone()));
            let obligations = self.pop_block_scope();
            self.replay_obligations(&obligations);
            self.set_terminator(TerminatorKind::Goto { target: join }, arm_span);
        }

        // typeck already proved the match exhaustive, so this is never actually reached; every
        // reserved block still needs a terminator.
        self.switch_to(no_match);
        self.set_terminator(TerminatorKind::Unreachable, span);

        self.switch_to(join);
    }

    // -----------------------------------------------------------------
    // Pattern testing (structure only, no binding)
    // -----------------------------------------------------------------

    /// Tests `pat`'s structure against `place`, falling through on a full match and jumping to
    /// `fail` on any refutation. Binds nothing -- see the module docs for why binding is a
    /// separate walk, run only by the caller once this returns having matched.
    pub(crate) fn test_pat(&mut self, pat_id: HirId, place: Place, fail: BasicBlock) {
        let pat = self.hir.pat(pat_id);
        let span = pat.span;
        match &pat.kind {
            PatKind::Wildcard | PatKind::Binding { .. } => {}
            PatKind::Literal(lit) => {
                let lit = *lit;
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
            PatKind::Variant { variant, payload } => {
                let variant = *variant;
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
                    .projection
                    .push(PlaceElem::Downcast(variant_idx));
                self.test_payload(ty, variant_idx, payload, payload_place, fail, span);
            }
            PatKind::Tuple(elems) => {
                let elems = elems.clone();
                for (i, &elem) in elems.iter().enumerate() {
                    let mut elem_place = place.clone();
                    elem_place.projection.push(PlaceElem::Field(i as u32));
                    self.test_pat(elem, elem_place, fail);
                }
            }
            PatKind::Error => unreachable!("a fully type-checked body contains no PatKind::Error"),
        }
    }

    fn test_payload(
        &mut self,
        enum_ty: Ty,
        variant_idx: VariantIdx,
        payload: &Payload,
        base: Place,
        fail: BasicBlock,
        span: SrcSpan,
    ) {
        match payload {
            Payload::None => {}
            Payload::Single(pat_id) => {
                let mut field_place = base;
                field_place.projection.push(PlaceElem::Field(0));
                self.test_pat(*pat_id, field_place, fail);
            }
            Payload::Record(fields) => {
                for field in fields {
                    let index = self.record_field_index(enum_ty, variant_idx, field.name.text);
                    let mut field_place = base.clone();
                    field_place.projection.push(PlaceElem::Field(index));
                    self.test_pat(field.value, field_place, fail);
                }
            }
        }
        let _ = span;
    }

    // -----------------------------------------------------------------
    // Pattern binding (run only once a pattern is known to fully match)
    // -----------------------------------------------------------------

    /// Binds every name a pattern known to already match introduces. Used both for an
    /// irrefutable `let`/`with` pattern (called directly, with no preceding [`test_pat`]) and
    /// for a `match`/`if let` candidate that has already passed [`test_pat`]. `mutability` is
    /// the `mut`-ness every `Binding` leaf this walk reaches is given, per
    /// `StatementKind::CheckMutable`'s own docs: a `let`'s own declared mutability at a `let`
    /// call site, and always [`Mutability::Mutable`] (unrestricted) at a `match`/`for`/`with`
    /// one, none of which has `mut` syntax of its own.
    pub(crate) fn bind_pat(
        &mut self,
        pat_id: HirId,
        place: Place,
        mutability: crate::ast::Mutability,
    ) {
        let pat = self.hir.pat(pat_id);
        let span = pat.span;
        match &pat.kind {
            PatKind::Wildcard => {}
            PatKind::Binding { name, .. } => {
                let name = *name;
                let ty = self.pat_ty(pat_id);
                let local = self.new_local(ty, mutability, Some(name), span);
                self.push_stmt(StatementKind::StorageLive(local), span);
                let operand = self.operand_for_place(place, ty);
                self.assign(Place::from_local(local), Rvalue::Use(operand), span);
                self.bind_local(pat_id, local);
                self.register_exit_obligation(ExitObligation::StorageDead(local));
            }
            PatKind::Literal(_) => {}
            PatKind::Variant { variant, payload } => {
                let variant = *variant;
                let ty = self.pat_ty(pat_id);
                let (_, variant_idx) = self.variant_idx_for(ty, variant.text);
                let mut payload_place = place;
                payload_place
                    .projection
                    .push(PlaceElem::Downcast(variant_idx));
                self.bind_payload(ty, variant_idx, payload, payload_place, mutability);
            }
            PatKind::Tuple(elems) => {
                let elems = elems.clone();
                for (i, &elem) in elems.iter().enumerate() {
                    let mut elem_place = place.clone();
                    elem_place.projection.push(PlaceElem::Field(i as u32));
                    self.bind_pat(elem, elem_place, mutability);
                }
            }
            PatKind::Error => unreachable!("a fully type-checked body contains no PatKind::Error"),
        }
    }

    fn bind_payload(
        &mut self,
        enum_ty: Ty,
        variant_idx: VariantIdx,
        payload: &Payload,
        base: Place,
        mutability: crate::ast::Mutability,
    ) {
        match payload {
            Payload::None => {}
            Payload::Single(pat_id) => {
                let mut field_place = base;
                field_place.projection.push(PlaceElem::Field(0));
                self.bind_pat(*pat_id, field_place, mutability);
            }
            Payload::Record(fields) => {
                for field in fields {
                    let index = self.record_field_index(enum_ty, variant_idx, field.name.text);
                    let mut field_place = base.clone();
                    field_place.projection.push(PlaceElem::Field(index));
                    self.bind_pat(field.value, field_place, mutability);
                }
            }
        }
    }

    // -----------------------------------------------------------------
    // Shared helpers
    // -----------------------------------------------------------------

    /// `pat_id`'s recorded type, resolved through this body's own `any_mode` -- see
    /// [`BodyLowerCtx::expr_ty`], its expression-level counterpart.
    pub(crate) fn pat_ty(&mut self, pat_id: HirId) -> Ty {
        let ty = self
            .types
            .ty(pat_id)
            .unwrap_or_else(|| panic!("mir::lower: {pat_id:?} has no recorded type"));
        self.resolve_any(ty, self.any_mode)
    }

    fn lower_pat_literal(&mut self, lit: Literal, ty: Ty) -> Constant {
        let kind = match lit {
            Literal::Int { value, .. } => ConstKind::Int(
                crate::ast::interner::Interner::resolve(value)
                    .parse()
                    .unwrap_or_else(|_| {
                        panic!("mir::lower: integer pattern literal does not parse")
                    }),
            ),
            Literal::Float { .. } => {
                panic!("mir::lower: a float pattern literal is not yet implemented")
            }
            Literal::Str(_) => {
                panic!("mir::lower: a string pattern literal is not yet implemented")
            }
            Literal::Bool(b) => ConstKind::Bool(b),
            Literal::Char(c) => ConstKind::Char(c),
        };
        Constant { ty, kind }
    }

    /// The enum `DefId` and declared-order [`VariantIdx`] a variant pattern or variant
    /// expression names, re-derived from pure HIR plus `ty` (the enum's own recorded `Adt`
    /// type) -- cheap, and needs no typeck-side state, since a variant is nominal and never
    /// ambiguous the way a method call is. See `planning/mir.md`'s "Pattern matching" section.
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

    fn record_field_index(
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
