use crate::ast::{BinaryOp, Ident, Literal, Mutability, UnaryOp};
use crate::driver::source::SrcSpan;
use crate::hir::{AccessArgs, ExprKind, HirId, Local as HirLocal, Payload, Res};
use crate::mir::lower::ctx::BodyLowerCtx;
use crate::mir::{
    AggregateKind, AssertMessage, CastKind, ConstKind, Constant, Operand, Place, Projection,
    Rvalue, StatementKind, TerminatorKind,
};
use crate::nameres::PrimTy;
use crate::typeck::results::DerefMode;
use crate::typeck::ty::{Ty, TyKind};

impl<'a> BodyLowerCtx<'a> {
    // -----------------------------------------------------------------
    // Entry points
    // -----------------------------------------------------------------

    /// Lowers `expr_id` to an [`Operand`] without a redundant temporary when it is already one.
    pub(crate) fn lower_operand(&mut self, expr_id: HirId) -> Operand {
        if let Some(target) = self.unsize_target_for(expr_id) {
            return self.lower_coerced_operand(expr_id, target);
        }
        self.lower_operand_without_coercion(expr_id)
    }

    /// The unsize coercion recorded for `expr_id`, if it has not been lowered yet. Whoever
    /// reaches the expression first claims the coercion; a claim is never revisited, because an
    /// expression is lowered exactly once.
    fn unsize_target_for(&mut self, expr_id: HirId) -> Option<Ty> {
        if self.coerced.contains(&expr_id) {
            return None;
        }
        let target = self.types.unsize(expr_id)?;
        self.coerced.insert(expr_id);
        Some(target)
    }

    /// Lowers a coercion-recorded expression as the fat `&dyn Trait` pointer it was checked to
    /// coerce into: the inner expression is lowered as itself (its own thin pointer), and the
    /// vtable word comes from the concrete type's impl at codegen time.
    fn lower_coerced_operand(&mut self, expr_id: HirId, target: Ty) -> Operand {
        let span = self.hir.expr(expr_id).span;
        let TyKind::Ref { base, .. } = self.tcx.kind(target).clone() else {
            unreachable!(
                "mir::lower: a recorded unsize target is always a reference to a `dyn` type"
            );
        };
        let TyKind::Dyn { trait_, .. } = self.tcx.kind(base).clone() else {
            unreachable!(
                "mir::lower: a recorded unsize target is always a reference to a `dyn` type"
            );
        };
        let inner = self.lower_operand_without_coercion(expr_id);
        let temp = self.new_temp(target, span);
        self.assign(
            Place::from_local(temp),
            Rvalue::Unsize {
                operand: inner,
                trait_,
            },
            span,
        );
        Operand::Move(Place::from_local(temp))
    }

    fn lower_operand_without_coercion(&mut self, expr_id: HirId) -> Operand {
        let expr_kind_is_trivial = matches!(
            self.hir.expr(expr_id).kind,
            ExprKind::Literal(_) | ExprKind::Path(_)
        );
        if !expr_kind_is_trivial {
            let ty = self.expr_ty(expr_id);
            let span = self.hir.expr(expr_id).span;
            let temp = self.new_temp(ty, span);
            let dest = Place::from_local(temp);
            self.lower_expr_into(expr_id, dest.clone());
            return self.operand_for_place(dest, ty);
        }

        let ty = self.expr_ty(expr_id);
        let span = self.hir.expr(expr_id).span;
        match self.hir.expr(expr_id).kind.clone() {
            ExprKind::Literal(lit) => Operand::Constant(self.lower_literal(lit, ty)),
            ExprKind::Path(path) => match path.res {
                Res::Local(local) => {
                    let place = self.place_for(hir_local_id(local));
                    self.operand_for_place(place, ty)
                }
                Res::Function(def) => {
                    let args = self.call_type_args(expr_id);
                    let fn_ty = ty;
                    self.reify_fn_pointer(def, args, fn_ty, span)
                }
                other => unreachable!("mir::lower: a value-position path resolves to {other:?}"),
            },
            _ => unreachable!("handled by the fast-path check above"),
        }
    }

    /// Lowers `expr_id` to a [`Place`]: a location this pass can read, write, or take a
    /// reference to. A path, a field access, an index, or a dereference already is one; anything
    /// else (a call's result, an aggregate literal, ...) needs a fresh temporary first.
    pub(crate) fn lower_place(&mut self, expr_id: HirId) -> Place {
        let expr = self.hir.expr(expr_id);
        let span = expr.span;
        match expr.kind.clone() {
            ExprKind::Path(path) => match path.res {
                Res::Local(local) => self.place_for(hir_local_id(local)),
                other => unreachable!("mir::lower: a place-position path resolves to {other:?}"),
            },
            ExprKind::Access {
                base,
                member,
                args: crate::hir::AccessArgs::None,
            } => self.lower_field_place(base, member),
            ExprKind::Index { base, index } => self.lower_index_place(base, index, span),
            ExprKind::Unary {
                op: UnaryOp::Deref,
                operand,
            } => self.lower_deref_place(operand),
            ExprKind::Unary {
                op: UnaryOp::Not, ..
            } => unreachable!("`!` is never a place"),
            _ => {
                let ty = self.expr_ty(expr_id);
                let temp = self.new_temp(ty, span);
                let dest = Place::from_local(temp);
                self.lower_expr_into(expr_id, dest.clone());
                dest
            }
        }
    }

    /// Lowers `expr_id` purely for its side effects, discarding its value.
    pub(crate) fn lower_expr_discarding(&mut self, expr_id: HirId) {
        let expr = self.hir.expr(expr_id);
        let span = expr.span;
        match expr.kind.clone() {
            ExprKind::Path(_) | ExprKind::Access { .. } | ExprKind::Index { .. } => {
                let place = self.lower_place(expr_id);
                self.push_stmt(StatementKind::PlaceMention(place), span);
            }
            ExprKind::Literal(_) => {}
            ExprKind::Assign { lhs, rhs } => {
                self.lower_assign_effect(lhs, rhs);
            }
            ExprKind::AssignOp { op, lhs, rhs } => {
                self.lower_assign_op_effect(op, lhs, rhs, span);
            }
            _ => {
                let ty = self.expr_ty(expr_id);
                let temp = self.new_temp(ty, span);
                self.lower_expr_into(expr_id, Place::from_local(temp));
            }
        }
    }

    fn lower_assign_effect(&mut self, lhs: HirId, rhs: HirId) {
        let place = self.lower_place(lhs);
        self.lower_expr_into(rhs, place);
    }

    fn lower_assign_op_effect(&mut self, op: BinaryOp, lhs: HirId, rhs: HirId, span: SrcSpan) {
        let place = self.lower_place(lhs);
        let lhs_ty = self.expr_ty(lhs);
        let lhs_operand = self.operand_for_place(place.clone(), lhs_ty);
        let rhs_operand = self.lower_operand(rhs);
        let result_local = self.new_temp(lhs_ty, span);
        self.lower_binary_op_into(
            op,
            lhs_operand,
            rhs_operand,
            Place::from_local(result_local),
            lhs_ty,
            span,
        );
        self.assign(
            place,
            Rvalue::Use(Operand::Move(Place::from_local(result_local))),
            span,
        );
    }

    /// The main dispatcher. Lowers `expr_id`'s value into `dest`, ending with `dest` holding the
    /// result -- every `ExprKind` funnels through here, including the control-flow ones (`if`,
    /// `match`, a bare block), which use `dest` as their shared join-point destination.
    pub(crate) fn lower_expr_into(&mut self, expr_id: HirId, dest: Place) {
        let expr = self.hir.expr(expr_id);
        let span = expr.span;
        let ty = self.expr_ty(expr_id);

        if let Some(target) = self.unsize_target_for(expr_id) {
            let operand = self.lower_coerced_operand(expr_id, target);
            self.assign(dest, Rvalue::Use(operand), span);
            return;
        }

        match expr.kind.clone() {
            ExprKind::Literal(_) | ExprKind::Path(_) => {
                let operand = self.lower_operand(expr_id);
                self.assign(dest, Rvalue::Use(operand), span);
            }
            ExprKind::Unary {
                op: UnaryOp::Deref,
                operand,
            } => {
                let place = self.lower_deref_place(operand);
                let operand = match self.types.deref_mode(expr_id) {
                    DerefMode::Copy => Operand::Copy(place),
                    DerefMode::Move => Operand::Move(place),
                };
                self.assign(dest, Rvalue::Use(operand), span);
            }
            ExprKind::Unary { op, operand } => {
                let operand = self.lower_operand(operand);
                self.assign(dest, Rvalue::UnaryOp(op, operand), span);
            }
            ExprKind::Binary { op, lhs, rhs } => {
                self.lower_binary_into(op, lhs, rhs, dest, span);
            }
            ExprKind::Assign { lhs, rhs } => {
                let place = self.lower_place(lhs);
                self.lower_expr_into(rhs, place);
                self.assign_unit(dest, span);
            }
            ExprKind::AssignOp { op, lhs, rhs } => {
                let place = self.lower_place(lhs);
                let lhs_ty = self.expr_ty(lhs);
                let lhs_operand = self.operand_for_place(place.clone(), lhs_ty);
                let rhs_operand = self.lower_operand(rhs);
                let result_local = self.new_temp(lhs_ty, span);
                self.lower_binary_op_into(
                    op,
                    lhs_operand,
                    rhs_operand,
                    Place::from_local(result_local),
                    lhs_ty,
                    span,
                );
                self.assign(
                    place,
                    Rvalue::Use(Operand::Move(Place::from_local(result_local))),
                    span,
                );
                self.assign_unit(dest, span);
            }
            ExprKind::Borrow {
                mutability,
                operand,
            } => {
                if self.is_any_specialized_call(operand) {
                    let mode = if mutability == Mutability::Mutable {
                        crate::mir::AnyMode::RefMut
                    } else {
                        crate::mir::AnyMode::Ref
                    };
                    self.lower_call_like_into(operand, dest, mode, span);
                } else {
                    let place = self.lower_place(operand);
                    if mutability == Mutability::Mutable {}
                    self.assign(dest, Rvalue::Ref { mutability, place }, span);
                }
            }
            ExprKind::Call { .. } => {
                self.lower_call_like_into(expr_id, dest, crate::mir::AnyMode::Owned, span);
            }
            // A variant reached through its enum, such as `Shape.circle(1.0)`. It reads as an
            // access, but it builds a value rather than reaching into one, so it is split off
            // ahead of the field and method arms exactly as typeck splits it off.
            ExprKind::Access { base, member, args } if self.hir.names_a_type(base) => {
                let payload = variant_payload_of(&args);
                self.lower_variant_into(member, ty, &payload, dest, span);
            }
            ExprKind::Access {
                args: crate::hir::AccessArgs::Call(_),
                ..
            } => {
                self.lower_call_like_into(expr_id, dest, crate::mir::AnyMode::Owned, span);
            }
            // A record payload on a base that names a value is rejected by typeck, so the only
            // `Access` left holding one would have been caught by the qualified-variant arm.
            ExprKind::Access {
                args: crate::hir::AccessArgs::Record(_),
                ..
            } => unreachable!("a record payload only ever builds a variant, handled above"),
            ExprKind::Access {
                args: crate::hir::AccessArgs::None,
                ..
            } => {
                let place = self.lower_place(expr_id);
                let operand = self.operand_for_place(place, ty);
                self.assign(dest, Rvalue::Use(operand), span);
            }

            ExprKind::Index { .. } => {
                if self.types.call(expr_id).is_some() {
                    self.lower_call_like_into(expr_id, dest, crate::mir::AnyMode::Owned, span);
                } else {
                    let place = self.lower_place(expr_id);
                    let operand = self.operand_for_place(place, ty);
                    self.assign(dest, Rvalue::Use(operand), span);
                }
            }
            ExprKind::Ctor { payload, .. } => {
                self.lower_ctor_into(ty, &payload, dest, span);
            }
            ExprKind::Variant { variant, payload } => {
                self.lower_variant_into(variant, ty, &payload, dest, span);
            }
            ExprKind::Tuple(elems) => {
                let operands = elems.iter().map(|&elem| self.lower_operand(elem)).collect();
                self.assign(
                    dest,
                    Rvalue::Aggregate(Box::new(AggregateKind::Tuple), operands),
                    span,
                );
            }
            ExprKind::Try(inner) => self.lower_try_into(inner, ty, dest, span),
            ExprKind::If {
                cond,
                then_block,
                else_block,
            } => self.lower_if_into(cond, then_block, else_block, dest, span),
            ExprKind::Match { scrutinee, arms } => self.lower_match_into(scrutinee, &arms, dest),
            ExprKind::Loop { block, .. } => self.lower_loop_into(block, dest, span),
            ExprKind::Block(block) => self.lower_block(block, Some(dest)),
            ExprKind::Closure(def_id) => self.lower_closure_literal_into(def_id, dest, span),
            ExprKind::Cast { expr, ty: ty_id } => {
                let _ = ty_id;
                let operand = self.lower_operand(expr);
                self.assign(
                    dest,
                    Rvalue::Cast {
                        operand,
                        ty,
                        kind: CastKind::Primitive,
                    },
                    span,
                );
            }
            ExprKind::New(operand) => {
                let operand = self.lower_operand(operand);
                self.assign(dest, Rvalue::New(operand), span);
            }
            ExprKind::NewArray { elem, count } => {
                let elem = self.lower_operand(elem);
                let count = self.lower_operand(count);
                self.assign(dest, Rvalue::NewArray { elem, count }, span);
            }
            ExprKind::Spawn(_) => panic!(
                "mir::lower: `spawn` is not yet implemented (the runtime nursery API is illustrative only)"
            ),
            ExprKind::Concurrent(_) => panic!(
                "mir::lower: `concurrent` is not yet implemented (the runtime nursery API is illustrative only)"
            ),
            ExprKind::Assert { cond, msg } => {
                self.lower_trap_into(Some(cond), msg, AssertMessage::Assert, dest, span);
            }
            ExprKind::Panic { msg } => {
                self.lower_trap_into(None, msg, AssertMessage::Panic, dest, span);
            }
            ExprKind::Unreachable { msg } => {
                self.lower_trap_into(None, msg, AssertMessage::Unreachable, dest, span);
            }
            ExprKind::Error => {
                unreachable!("a fully type-checked body contains no ExprKind::Error")
            }
        }
    }

    fn lower_trap_into(
        &mut self,
        cond: Option<HirId>,
        msg: Option<HirId>,
        make_msg: fn(Option<Operand>) -> AssertMessage,
        dest: Place,
        span: SrcSpan,
    ) {
        let msg_operand = msg.map(|m| self.lower_operand(m));
        let (cond_operand, expected) = match cond {
            Some(cond) => (self.lower_operand(cond), true),
            None => {
                let bool_ty = self.tcx.mk_prim(PrimTy::Bool);
                let operand = Operand::Constant(Constant {
                    ty: bool_ty,
                    kind: ConstKind::Bool(true),
                });
                (operand, false)
            }
        };
        let target = self.new_block();
        self.set_terminator(
            TerminatorKind::Assert {
                cond: cond_operand,
                expected,
                msg: make_msg(msg_operand),
                target,
            },
            span,
        );
        self.switch_to(target);
        if cond.is_some() {
            self.assign_unit(dest, span);
        }
    }

    // -----------------------------------------------------------------
    // Helpers
    // -----------------------------------------------------------------

    /// `expr_id`'s recorded type, resolved through this body's own `any_mode` -- see the field's
    /// doc comment on [`BodyLowerCtx`] for why every type this pass reads goes through this
    /// uniformly rather than only the parameter/return positions `mir::lower::item` sets up
    /// directly.
    pub(crate) fn expr_ty(&mut self, expr_id: HirId) -> Ty {
        let ty = self
            .types
            .ty(expr_id)
            .unwrap_or_else(|| panic!("mir::lower: {expr_id:?} has no recorded type"));
        self.resolve_any(ty, self.any_mode)
    }

    pub(crate) fn assign(&mut self, dest: Place, rvalue: Rvalue, span: SrcSpan) {
        self.push_stmt(StatementKind::Assign(dest, rvalue), span);
    }

    /// `Copy` for a trivially copyable place -- a primitive, or a shared `&T` -- `Move`
    /// otherwise, with no liveness analysis: the classification follows only from `ty`'s own
    /// shape. A shared reference grants no exclusive access, so reading the same place holding
    /// one twice is exactly as sound as reading a primitive twice; a `&mut T` grants exclusive
    /// access, so duplicating it would defeat the point and it still falls to `Move`.
    pub(crate) fn operand_for_place(&self, place: Place, ty: Ty) -> Operand {
        let trivially_copyable = matches!(
            self.tcx.kind(ty),
            TyKind::Primitive(_)
                | TyKind::Ref {
                    mutability: Mutability::Immutable,
                    ..
                }
        );
        if trivially_copyable {
            Operand::Copy(place)
        } else {
            Operand::Move(place)
        }
    }

    /// Assigns the unit value into `dest`. Represented as a zero-element tuple aggregate rather
    /// than a dedicated constant -- `()` is exactly the 0-arity tuple mathematically, and
    /// `AggregateKind::Tuple` already covers it without needing a new `ConstKind` variant.
    pub(crate) fn assign_unit(&mut self, dest: Place, span: SrcSpan) {
        self.assign(
            dest,
            Rvalue::Aggregate(Box::new(AggregateKind::Tuple), Vec::new()),
            span,
        );
    }

    fn lower_literal(&mut self, lit: Literal, ty: Ty) -> Constant {
        let kind = match lit {
            Literal::Int { .. } => {
                let text = literal_text(lit);
                ConstKind::Int(text.parse().unwrap_or_else(|_| {
                    panic!("mir::lower: integer literal {text:?} does not parse as i128")
                }))
            }
            Literal::Float { .. } => {
                let text = literal_text(lit);
                ConstKind::Float(text.parse().unwrap_or_else(|_| {
                    panic!("mir::lower: float literal {text:?} does not parse as f64")
                }))
            }
            Literal::Str(s) => ConstKind::Str(s),
            Literal::Bool(b) => ConstKind::Bool(b),
            Literal::Char(c) => ConstKind::Char(c),
        };
        Constant { ty, kind }
    }

    fn lower_deref_place(&mut self, operand: HirId) -> Place {
        let operand_ty = self.expr_ty(operand);
        let mut place = self.lower_place(operand);
        if matches!(
            self.tcx.kind(operand_ty),
            TyKind::Ref { .. } | TyKind::Iso(_)
        ) {
            place.projections.push(Projection::Deref);
        }
        place
    }

    fn lower_field_place(&mut self, base: HirId, member: crate::ast::Ident) -> Place {
        let base_ty = self.expr_ty(base);
        let mut place = self.lower_place(base);
        let (peeled_ty, derefs) = self.peel_refs(base_ty);
        for _ in 0..derefs {
            place.projections.push(Projection::Deref);
        }
        let index = self.field_index(peeled_ty, member.text);
        place.projections.push(Projection::Field(index));
        place
    }

    /// Strips every `&`/`&mut` layer off `ty`, returning the base type and how many layers came
    /// off -- how many `Deref` projections a place reaching through it needs.
    pub(crate) fn peel_refs(&self, ty: Ty) -> (Ty, u32) {
        let mut current = ty;
        let mut count = 0;
        while let TyKind::Ref { base, .. } = *self.tcx.kind(current) {
            current = base;
            count += 1;
        }
        (current, count)
    }

    /// The declared field index of `member` on struct type `ty`, by name -- nominal, so no
    /// typeck help is needed, exactly as `planning/mir.md`'s `rect.l` example describes. For a
    /// tuple type, `member` is instead the tuple index written in source (`.0`, `.1`, ...),
    /// already checked in range by typeck.
    fn field_index(&self, ty: Ty, member: crate::ast::interner::Symbol) -> u32 {
        if matches!(self.tcx.kind(ty), TyKind::Tuple(_)) {
            return crate::ast::interner::Interner::resolve(member)
                .parse::<u32>()
                .unwrap_or_else(|_| panic!("mir::lower: tuple field index is not a number"));
        }

        let TyKind::Adt { def, .. } = *self.tcx.kind(ty) else {
            panic!("mir::lower: field access on a non-struct, non-tuple type")
        };
        let s = self.hir.struct_(def);
        s.fields
            .iter()
            .position(|&f| self.hir.field(f).name.text == member)
            .unwrap_or_else(|| panic!("mir::lower: struct has no field matching {member:?}"))
            as u32
    }

    fn lower_index_place(&mut self, base: HirId, index: HirId, span: SrcSpan) -> Place {
        let base_ty = self.expr_ty(base);
        let (peeled, derefs) = self.peel_refs(base_ty);
        let mut place = self.lower_place(base);
        for _ in 0..derefs {
            place.projections.push(Projection::Deref);
        }

        match self.tcx.kind(peeled).clone() {
            TyKind::Array { .. } => {
                let index_ty = self.expr_ty(index);

                // A compile-time-constant index (a bare integer literal, not yet folded through
                // any arithmetic) projects through `ConstantIndex` instead of `Index`: unlike a
                // runtime `Local`, a constant offset lets borrowck's `register_of` (see
                // `mir::checks::borrowck::register_of`) tell `a[0]` and `a[1]` apart as disjoint
                // sub-registers of `a` rather than treating any index into `a` as touching the
                // whole array.
                let projection = if let ExprKind::Literal(lit @ Literal::Int { .. }) =
                    self.hir.expr(index).kind.clone()
                {
                    let text = literal_text(lit);
                    let offset: u32 = text.parse().unwrap_or_else(|_| {
                        panic!("mir::lower: array index {text:?} does not fit a u32")
                    });
                    Projection::ConstantIndex(offset)
                } else {
                    let index_operand = self.lower_operand(index);
                    let index_local = self.new_temp(index_ty, span);
                    self.assign(
                        Place::from_local(index_local),
                        Rvalue::Use(index_operand),
                        span,
                    );
                    Projection::Index(index_local)
                };

                let len_local = self.new_temp(index_ty, span);
                self.assign(
                    Place::from_local(len_local),
                    Rvalue::Len(place.clone()),
                    span,
                );
                let index_operand = match projection {
                    Projection::ConstantIndex(offset) => Operand::Constant(Constant {
                        ty: index_ty,
                        kind: ConstKind::Int(offset as i128),
                    }),
                    Projection::Index(index_local) => Operand::Copy(Place::from_local(index_local)),
                    _ => unreachable!("projection is always ConstantIndex or Index here"),
                };
                let assert_target = self.new_block();
                let bool_ty = self.tcx.mk_prim(PrimTy::Bool);
                self.set_terminator(
                    TerminatorKind::Assert {
                        cond: Operand::Constant(Constant {
                            ty: bool_ty,
                            kind: ConstKind::Bool(true),
                        }),
                        expected: true,
                        msg: AssertMessage::BoundsCheck {
                            len: Operand::Copy(Place::from_local(len_local)),
                            index: index_operand,
                        },
                        target: assert_target,
                    },
                    span,
                );
                self.switch_to(assert_target);
                place.projections.push(projection);
                place
            }
            // An overloaded `Index`/`IndexSet` receiver: `check_index` already resolved this as
            // a method call (see `TypeResolutions::call`), so it is not a plain projection at
            // all -- lowering it as a place means calling `index`/`index_set` into a temporary
            // and treating that as the place, which `lower_call_like_into`'s general call
            // handling already does for a method call used as an operand. There is no direct
            // `Place` for a user-defined index today (it would need a projection kind this MIR
            // does not have, one that runs a method call), so this position is not yet
            // implemented.
            _ => panic!(
                "mir::lower: an overloaded `Index`/`IndexSet` used as a place is not yet implemented"
            ),
        }
    }

    fn lower_binary_into(
        &mut self,
        op: BinaryOp,
        lhs: HirId,
        rhs: HirId,
        dest: Place,
        span: SrcSpan,
    ) {
        // `&&`/`||` short-circuit, so they are control flow, not a plain `Rvalue::BinaryOp`.
        if matches!(op, BinaryOp::And | BinaryOp::Or) {
            return self.lower_short_circuit_into(op, lhs, rhs, dest, span);
        }
        let lhs_ty = self.expr_ty(lhs);
        let lhs_operand = self.lower_operand(lhs);
        let rhs_operand = self.lower_operand(rhs);
        self.lower_binary_op_into(op, lhs_operand, rhs_operand, dest, lhs_ty, span);
    }

    fn lower_short_circuit_into(
        &mut self,
        op: BinaryOp,
        lhs: HirId,
        rhs: HirId,
        dest: Place,
        span: SrcSpan,
    ) {
        let lhs_operand = self.lower_operand(lhs);
        let rhs_block = self.new_block();
        let short_circuit_block = self.new_block();
        let join_block = self.new_block();

        let (true_target, false_target) = match op {
            BinaryOp::And => (rhs_block, short_circuit_block),
            BinaryOp::Or => (short_circuit_block, rhs_block),
            _ => unreachable!("only `&&`/`||` short-circuit"),
        };
        self.set_terminator(
            TerminatorKind::SwitchInt {
                discr: lhs_operand,
                targets: crate::mir::SwitchTargets {
                    values: vec![(1, true_target)],
                    otherwise: false_target,
                },
            },
            span,
        );

        self.switch_to(short_circuit_block);
        let short_value = matches!(op, BinaryOp::Or);
        let bool_ty = self.tcx.mk_prim(PrimTy::Bool);
        self.assign(
            dest.clone(),
            Rvalue::Use(Operand::Constant(Constant {
                ty: bool_ty,
                kind: ConstKind::Bool(short_value),
            })),
            span,
        );
        self.set_terminator(TerminatorKind::Goto { target: join_block }, span);

        self.switch_to(rhs_block);
        self.lower_expr_into(rhs, dest);
        self.set_terminator(TerminatorKind::Goto { target: join_block }, span);

        self.switch_to(join_block);
    }

    /// Lowers the arithmetic itself, once both operands are already `Operand`s: a checked
    /// operation with an overflow `Assert` for integer `+`/`-`/`*` in a debug-profile body, an
    /// unconditional zero-check `Assert` for `/`/`%`, and a plain operation otherwise.
    fn lower_binary_op_into(
        &mut self,
        op: BinaryOp,
        lhs: Operand,
        rhs: Operand,
        dest: Place,
        operand_ty: Ty,
        span: SrcSpan,
    ) {
        let is_int = matches!(self.tcx.kind(operand_ty), TyKind::Primitive(p) if p.is_integer());
        let is_flt = matches!(self.tcx.kind(operand_ty), TyKind::Primitive(p) if p.is_float());

        if is_int && matches!(op, BinaryOp::Div | BinaryOp::Rem) {
            let assert_msg = if op == BinaryOp::Div {
                AssertMessage::DivisionByZero(rhs.clone())
            } else {
                AssertMessage::RemainderByZero(rhs.clone())
            };
            let zero = Operand::Constant(Constant {
                ty: operand_ty,
                kind: ConstKind::Int(0),
            });
            let bool_ty = self.tcx.mk_prim(PrimTy::Bool);
            let ne_zero_local = self.new_temp(bool_ty, span);
            self.assign(
                Place::from_local(ne_zero_local),
                Rvalue::BinaryOp(BinaryOp::Ne, rhs.clone(), zero),
                span,
            );
            let target = self.new_block();
            self.set_terminator(
                TerminatorKind::Assert {
                    cond: Operand::Copy(Place::from_local(ne_zero_local)),
                    expected: true,
                    msg: assert_msg,
                    target,
                },
                span,
            );
            self.switch_to(target);
        }

        let checked = is_int
            && matches!(op, BinaryOp::Add | BinaryOp::Sub | BinaryOp::Mul)
            && self.mode == crate::driver::cli::Mode::Debug;

        if checked {
            let bool_ty = self.tcx.mk_prim(PrimTy::Bool);
            let pair_ty = self.tcx.mk_tuple(vec![operand_ty, bool_ty]);
            let pair_local = self.new_temp(pair_ty, span);
            self.assign(
                Place::from_local(pair_local),
                Rvalue::CheckedBinaryOp(op, lhs.clone(), rhs.clone()),
                span,
            );
            let overflowed = Place {
                local: pair_local,
                projections: vec![Projection::Field(1)],
            };
            let bool_ty = self.tcx.mk_prim(PrimTy::Bool);
            let not_overflowed_local = self.new_temp(bool_ty, span);
            self.assign(
                Place::from_local(not_overflowed_local),
                Rvalue::UnaryOp(UnaryOp::Not, Operand::Copy(overflowed)),
                span,
            );
            let target = self.new_block();
            self.set_terminator(
                TerminatorKind::Assert {
                    cond: Operand::Copy(Place::from_local(not_overflowed_local)),
                    expected: true,
                    msg: AssertMessage::Overflow(op, lhs, rhs),
                    target,
                },
                span,
            );
            self.switch_to(target);
            let result = Place {
                local: pair_local,
                projections: vec![Projection::Field(0)],
            };
            self.assign(dest, Rvalue::Use(Operand::Move(result)), span);
        } else {
            let _ = is_flt;
            self.assign(dest, Rvalue::BinaryOp(op, lhs, rhs), span);
        }
    }

    // -----------------------------------------------------------------
    // Aggregates: struct literals, enum variants
    // -----------------------------------------------------------------

    fn lower_ctor_into(
        &mut self,
        ty: Ty,
        payload: &[crate::hir::PayloadField],
        dest: Place,
        span: SrcSpan,
    ) {
        let TyKind::Adt { def, .. } = *self.tcx.kind(ty) else {
            panic!("mir::lower: a struct literal's type is not an Adt")
        };
        let s = self.hir.struct_(def);
        let fields = s.fields.clone();
        let operands = fields
            .iter()
            .map(|&field_id| {
                let name = self.hir.field(field_id).name.text;
                let written = payload
                    .iter()
                    .find(|f| f.name.text == name)
                    .unwrap_or_else(|| {
                        panic!("mir::lower: struct literal is missing a declared field")
                    });
                self.lower_operand(written.value)
            })
            .collect();
        self.assign(
            dest,
            Rvalue::Aggregate(
                Box::new(AggregateKind::Adt {
                    def,
                    variant: crate::mir::VariantIdx::from_usize(0),
                }),
                operands,
            ),
            span,
        );
    }

    fn lower_variant_into(
        &mut self,
        variant: Ident,
        ty: Ty,
        payload: &crate::hir::Payload,
        dest: Place,
        span: SrcSpan,
    ) {
        let (def, variant_idx) = self.variant_idx_for(ty, variant.text);
        let operands = self.lower_variant_payload_operands(def, variant_idx, payload);
        self.assign(
            dest,
            Rvalue::Aggregate(
                Box::new(AggregateKind::Adt {
                    def,
                    variant: variant_idx,
                }),
                operands,
            ),
            span,
        );
    }

    fn lower_variant_payload_operands(
        &mut self,
        def: crate::hir::DefId,
        variant_idx: crate::mir::VariantIdx,
        payload: &crate::hir::Payload,
    ) -> Vec<Operand> {
        match payload {
            crate::hir::Payload::None => Vec::new(),
            crate::hir::Payload::Single(id) => vec![self.lower_operand(*id)],
            crate::hir::Payload::Record(fields) => {
                let e = self.hir.enum_(def);
                let variant_hir_id = e.variants[variant_idx.index()];
                let crate::hir::VariantPayload::Record(declared) =
                    &self.hir.variant(variant_hir_id).payload
                else {
                    panic!("mir::lower: a record payload's variant is not declared as a record")
                };
                let declared = declared.clone();
                declared
                    .iter()
                    .map(|&field_id| {
                        let name = self.hir.field(field_id).name.text;
                        let written =
                            fields
                                .iter()
                                .find(|f| f.name.text == name)
                                .unwrap_or_else(|| {
                                    panic!("mir::lower: record payload is missing a declared field")
                                });
                        self.lower_operand(written.value)
                    })
                    .collect()
            }
        }
    }

    // -----------------------------------------------------------------
    // `?`
    // -----------------------------------------------------------------

    /// `expr?`, per the spec's own worked example: a `SwitchInt` on the scrutinee's
    /// discriminant, an `ok` arm that reads the payload through `Downcast(ok).Field(0)` and
    /// continues, and an `err` arm that builds the enclosing function's own `Result::err`
    /// variant from the moved error payload and returns it immediately.
    fn lower_try_into(&mut self, inner: HirId, ok_ty: Ty, dest: Place, span: SrcSpan) {
        let scrutinee_ty = self.expr_ty(inner);
        let scrutinee_local = self.new_temp(scrutinee_ty, span);
        let scrutinee_place = Place::from_local(scrutinee_local);
        self.lower_expr_into(inner, scrutinee_place.clone());

        let result_def = self
            .hir
            .lang_items()
            .get(crate::langitems::LangItem::Result)
            .expect("`?` requires the `Result` lang item");
        let ok_idx = self.variant_idx_by_name(result_def, "ok");
        let err_idx = self.variant_idx_by_name(result_def, "err");

        let i32_ty = self.tcx.mk_prim(PrimTy::I32);
        let discr_local = self.new_temp(i32_ty, span);
        self.assign(
            Place::from_local(discr_local),
            Rvalue::Discriminant(scrutinee_place.clone()),
            span,
        );

        let ok_block = self.new_block();
        let err_block = self.new_block();
        self.set_terminator(
            TerminatorKind::SwitchInt {
                discr: Operand::Copy(Place::from_local(discr_local)),
                targets: crate::mir::SwitchTargets {
                    values: vec![(ok_idx.index() as u128, ok_block)],
                    otherwise: err_block,
                },
            },
            span,
        );

        self.switch_to(err_block);
        let mut err_place = scrutinee_place.clone();
        err_place.projections.push(Projection::Downcast(err_idx));
        err_place.projections.push(Projection::Field(0));
        let err_ty = match self.tcx.kind(scrutinee_ty).clone() {
            TyKind::Adt { args, .. } if args.len() == 2 => args[1],
            _ => panic!("mir::lower: `?`'s operand is not a two-argument Result"),
        };
        let err_operand = self.operand_for_place(err_place, err_ty);
        self.assign(
            Place::from_local(crate::mir::Local::RETURN_PLACE),
            Rvalue::Aggregate(
                Box::new(AggregateKind::Adt {
                    def: result_def,
                    variant: err_idx,
                }),
                vec![err_operand],
            ),
            span,
        );
        let obligations = self.obligations_for_return();
        self.replay_obligations(&obligations);
        self.set_terminator(TerminatorKind::Return, span);
        let dead = self.new_block();
        self.switch_to(dead);
        self.set_terminator(TerminatorKind::Unreachable, span);

        self.switch_to(ok_block);
        let mut ok_place = scrutinee_place;
        ok_place.projections.push(Projection::Downcast(ok_idx));
        ok_place.projections.push(Projection::Field(0));
        let ok_operand = self.operand_for_place(ok_place, ok_ty);
        self.assign(dest, Rvalue::Use(ok_operand), span);
    }

    fn variant_idx_by_name(&self, def: crate::hir::DefId, name: &str) -> crate::mir::VariantIdx {
        let e = self.hir.enum_(def);
        let sym = crate::ast::interner::Interner::intern(name);
        let index = e
            .variants
            .iter()
            .position(|&v| self.hir.variant(v).name.text == sym)
            .unwrap_or_else(|| panic!("mir::lower: enum has no variant named {name:?}"));
        crate::mir::VariantIdx::from_usize(index)
    }
}

pub(super) fn hir_local_id(local: HirLocal) -> HirId {
    match local {
        HirLocal::Param(id) | HirLocal::SelfParam(id) | HirLocal::Variable(id) => id,
    }
}

fn literal_text(lit: Literal) -> String {
    match lit {
        Literal::Int { value, .. } | Literal::Float { value, .. } => {
            crate::ast::interner::Interner::resolve(value).to_string()
        }
        _ => unreachable!("literal_text is only called for Int/Float"),
    }
}

/// Reads a qualified variant's payload (`Shape.circle(1.0)`) as the payload the elided form
/// (`.circle(1.0)`) would have carried, so both spellings lower through one path.
///
/// Typeck rejects an argument list that is not exactly one value, so by the time MIR runs a
/// `Call` payload holds exactly one argument.
fn variant_payload_of(args: &AccessArgs) -> Payload {
    match args {
        AccessArgs::None => Payload::None,
        AccessArgs::Call(args) => match args.as_slice() {
            [value] => Payload::Single(*value),
            args => unreachable!(
                "typeck rejects a qualified variant carrying {} payload arguments",
                args.len()
            ),
        },
        AccessArgs::Record(fields) => Payload::Record(fields.clone()),
    }
}
