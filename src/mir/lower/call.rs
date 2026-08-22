//! Call lowering: uniform call syntax (a method's receiver becomes `args[0]`), the
//! `Res::Function`/indirect-place split a callee can be, `dyn` dispatch's deliberate panic
//! (its vtable/fat-pointer layout is a later pass, per the spec's own "Status" section), and
//! `any`-mode projection specialization.

use crate::ast::Mutability;
use crate::driver::source::SrcSpan;
use crate::hir::{AccessArgs, DefId, ExprKind, HirId, Res};
use crate::mir::lower::ctx::BodyLowerCtx;
use crate::mir::lower::{Task, is_any_specialized};
use crate::mir::{
    AnyMode, ConstKind, Constant, Operand, Place, Projection, Rvalue, StatementKind, TerminatorKind,
};
use crate::typeck::ty::{Ty, TyKind};

impl<'a> BodyLowerCtx<'a> {
    /// Whether `expr_id` is a call (a `Call`, a method-shaped `Access`, or an overloaded
    /// `Index`) whose resolved callee is specialized by `any`-mode -- used by `ExprKind::Borrow`
    /// to decide whether its operand needs the mode-aware call lowering below instead of an
    /// ordinary place-borrow.
    pub(crate) fn is_any_specialized_call(&self, expr_id: HirId) -> bool {
        let Some(def) = self.call_target_def(expr_id) else {
            return false;
        };
        is_any_specialized(self.tcx, self.types, def)
    }

    /// The statically-resolved callee `DefId` a `Call`/method-`Access`/overloaded-`Index`
    /// expression names, if any -- `None` for an indirect call through a `fun`-typed place.
    fn call_target_def(&self, expr_id: HirId) -> Option<DefId> {
        match &self.hir.expr(expr_id).kind {
            ExprKind::Call { callee, .. } => match &self.hir.expr(*callee).kind {
                ExprKind::Path(path) => match path.res {
                    Res::Function(_) => self.types.call(expr_id).map(|c| c.def),
                    _ => None,
                },
                _ => None,
            },
            ExprKind::Access {
                args: AccessArgs::Call(_),
                ..
            }
            | ExprKind::Index { .. } => self.types.call(expr_id).map(|c| c.def),
            _ => None,
        }
    }

    /// Lowers a `Call`, a method-shaped `Access`, or an overloaded `Index` into a `Call`
    /// terminator targeting `dest`, then continues in the fresh block the call returns to.
    /// `mode` only matters for a callee whose return type is `any T`; it is ignored otherwise.
    pub(crate) fn lower_call_like_into(
        &mut self,
        expr_id: HirId,
        dest: Place,
        mode: AnyMode,
        span: SrcSpan,
    ) {
        let expr_kind = self.hir.expr(expr_id).kind.clone();
        let (func, receiver, arg_exprs, def_for_args) = match expr_kind {
            ExprKind::Call { callee, args } => {
                let func = self.lower_callee(expr_id, callee, mode, span);
                let def = self.call_target_def(expr_id);
                (func, None, args, def)
            }
            ExprKind::Access {
                base,
                args: AccessArgs::Call(args),
                ..
            } => {
                self.check_not_dyn_dispatch(base, span);
                let resolved = self
                    .types
                    .call(expr_id)
                    .unwrap_or_else(|| panic!("mir::lower: {expr_id:?} has no resolved call"))
                    .clone();
                let func = self.resolved_fn_operand(resolved.def, resolved.args, mode, span);
                (func, Some(base), args, Some(resolved.def))
            }
            ExprKind::Index { base, index } => {
                self.check_not_dyn_dispatch(base, span);
                let resolved = self
                    .types
                    .call(expr_id)
                    .unwrap_or_else(|| panic!("mir::lower: {expr_id:?} has no resolved call"))
                    .clone();
                let func = self.resolved_fn_operand(resolved.def, resolved.args, mode, span);
                (func, Some(base), vec![index], Some(resolved.def))
            }
            _ => unreachable!("lower_call_like_into is only called for Call/Access/Index"),
        };

        let any_mode = match def_for_args {
            Some(def) if is_any_specialized(self.tcx, self.types, def) => Some(mode),
            _ => None,
        };
        let args = match def_for_args {
            Some(def) => self.lower_call_args(def, any_mode, receiver, &arg_exprs, span),
            None => {
                let mut operands = Vec::new();
                if let Some(recv) = receiver {
                    operands.push(self.lower_operand(recv));
                }
                operands.extend(arg_exprs.iter().map(|&a| self.lower_operand(a)));
                operands
            }
        };

        // Per the spec: `target` is `None` exactly when this call's own return type is `Never`
        // -- the runtime panic function, for instance, which never returns control to its
        // caller. There is still a fresh block to switch to afterward, the same "dead code after
        // a hard exit" block every other diverging terminator (`Return`, `Goto` after `break`/
        // `continue`) already gets, so `self.current` stays open for whatever the caller does
        // next, even though nothing ever actually reaches it.
        let call_ty = self.expr_ty(expr_id);
        let never_returns = matches!(self.tcx.kind(call_ty), TyKind::Never);
        let target = if never_returns {
            None
        } else {
            Some(self.new_block())
        };
        self.set_terminator(
            TerminatorKind::Call {
                func,
                args,
                destination: dest,
                target,
            },
            span,
        );
        let fresh = target.unwrap_or_else(|| self.new_block());
        self.switch_to(fresh);
    }

    /// Panics with a clear message if `receiver_expr`'s peeled type is `dyn Trait` -- vtable
    /// dispatch is deliberately not yet implemented; see the spec's own "Status" section, which
    /// defers a `dyn` value's fat-pointer layout to a later pass.
    fn check_not_dyn_dispatch(&mut self, receiver_expr: HirId, span: SrcSpan) {
        let receiver_ty = self.expr_ty(receiver_expr);
        let (peeled, _) = self.peel_refs(receiver_ty);
        let (peeled, _) = self.peel_any(peeled);
        if matches!(self.tcx.kind(peeled), TyKind::Dyn { .. }) {
            let _ = span;
            panic!(
                "mir::lower: a `dyn Trait` call is not yet implemented (its vtable/fat-pointer \
                 layout is a later pass, per the spec's own Status section)"
            );
        }
    }

    /// Strips `Any` layers off `ty`, the way `peel_refs` strips `Ref` layers.
    fn peel_any(&self, ty: Ty) -> (Ty, u32) {
        let mut current = ty;
        let mut count = 0;
        while let TyKind::Any(base) = *self.tcx.kind(current) {
            current = base;
            count += 1;
        }
        (current, count)
    }

    /// The callee of a `Call` expression specifically: a direct call to a named function, or an
    /// indirect call through whatever place a non-path (or non-function-path) callee expression
    /// addresses. Never reifies -- that coercion is for a function used as a *value*, and this
    /// position is precisely the one place a named function is not one.
    fn lower_callee(
        &mut self,
        call_expr_id: HirId,
        callee_id: HirId,
        mode: AnyMode,
        span: SrcSpan,
    ) -> Operand {
        let is_named_fn = matches!(
            &self.hir.expr(callee_id).kind,
            ExprKind::Path(path) if matches!(path.res, Res::Function(_))
        );
        if is_named_fn {
            let resolved = self
                .types
                .call(call_expr_id)
                .unwrap_or_else(|| panic!("mir::lower: {call_expr_id:?} has no resolved call"))
                .clone();
            self.resolved_fn_operand(resolved.def, resolved.args, mode, span)
        } else {
            let place = self.lower_place(callee_id);
            Operand::Move(place)
        }
    }

    /// Builds the `Operand::Constant(FnDef(..))` naming `def`, instantiated with `args`,
    /// discovering an `AnySpecialized` lowering task for `mode` when `def`'s return type is
    /// `any T`.
    fn resolved_fn_operand(
        &mut self,
        def: DefId,
        args: Vec<Ty>,
        mode: AnyMode,
        _span: SrcSpan,
    ) -> Operand {
        let any_mode = if is_any_specialized(self.tcx, self.types, def) {
            self.discover(Task::AnySpecialized(def, mode));
            Some(mode)
        } else {
            None
        };
        let fn_ty = self.types.ty_of_def(def).unwrap_or_else(|| self.tcx.unit());
        Operand::Constant(Constant {
            ty: fn_ty,
            kind: ConstKind::FunDef(def, args, any_mode),
        })
    }

    /// Builds a call's argument list, receiver first when there is one. An `any`-typed
    /// parameter's argument is auto-projected to match `any_mode` -- borrowed into a fresh
    /// temporary if the mode calls for a reference -- rather than requiring the caller to have
    /// written `&`/`&mut` explicitly, matching the README's own `min(a, b)` example.
    fn lower_call_args(
        &mut self,
        def: DefId,
        any_mode: Option<AnyMode>,
        receiver: Option<HirId>,
        arg_exprs: &[HirId],
        span: SrcSpan,
    ) -> Vec<Operand> {
        let function = self.hir.function(def);
        let self_param = function.self_param;
        let params = function.params.clone();

        let mut operands = Vec::new();
        if let Some(recv_expr) = receiver {
            let declared = self_param
                .and_then(|id| self.types.ty(id))
                .unwrap_or_else(|| self.tcx.error());
            operands.push(self.lower_receiver_operand(recv_expr, declared, any_mode, span));
        }
        for (i, &arg_expr) in arg_exprs.iter().enumerate() {
            let declared = params
                .get(i)
                .and_then(|&id| self.types.ty(id))
                .unwrap_or_else(|| self.tcx.error());
            operands.push(self.lower_arg_operand(arg_expr, declared, any_mode, span));
        }
        operands
    }

    /// Builds a method call's receiver operand, adjusting it to reach the shape `self`'s declared
    /// type asks for -- the receiver adjustment
    /// [`Typeck::peel_receiver`](crate::typeck::Typeck::peel_receiver)'s own docs describe,
    /// typeck having already confirmed it is legal. `SelfMode::Move` (a bare
    /// `Self` self-parameter) and `SelfMode::Any` need none of this and fall through to
    /// [`BodyLowerCtx::lower_arg_operand`]'s existing handling; this only widens the ordinary
    /// `SelfMode::Immutable`/`SelfMode::Mutable` case, a plain `&`/`&mut Self`.
    ///
    /// Every `&`/`&mut` layer the receiver expression's own type already carries is dereferenced
    /// away first -- however many there are, and regardless of their own mutability, since this
    /// compiler enforces no borrow checking on a reference already in hand (see
    /// `Typeck::place_mutable_root`'s docs) -- and a fresh reference at exactly the declared
    /// mutability is taken of what is left. A receiver with no layers at all (an ordinary place,
    /// `x.foo()` where `x: Foo` and `foo` takes `&mut self`) is the autoref case: the same write
    /// a `&mut` borrow anywhere else is, so it gets the same `CheckMutable` marker
    /// [`StatementKind::CheckMutable`]'s own docs describe, for `mir::checks::constck` to check.
    fn lower_receiver_operand(
        &mut self,
        expr_id: HirId,
        declared_ty: Ty,
        any_mode: Option<AnyMode>,
        span: SrcSpan,
    ) -> Operand {
        let TyKind::Ref { mutability, .. } = *self.tcx.kind(declared_ty) else {
            return self.lower_arg_operand(expr_id, declared_ty, any_mode, span);
        };

        let recv_ty = self.expr_ty(expr_id);
        if matches!(self.tcx.kind(recv_ty), TyKind::Any(_)) {
            panic!(
                "mir::lower: a receiver whose own type is `any T`, reaching a `&`/`&mut self` \
                 method, is not yet implemented"
            );
        }
        let (_, derefs) = self.peel_refs(recv_ty);
        let mut place = self.lower_place(expr_id);
        for _ in 0..derefs {
            place.projections.push(Projection::Deref);
        }
        if derefs == 0 && mutability == Mutability::Mutable {
            self.push_stmt(StatementKind::CheckMutable(place.clone()), span);
        }

        let temp = self.new_temp(declared_ty, span);
        self.assign(
            Place::from_local(temp),
            Rvalue::Ref { mutability, place },
            span,
        );
        Operand::Move(Place::from_local(temp))
    }

    fn lower_arg_operand(
        &mut self,
        expr_id: HirId,
        declared_ty: Ty,
        any_mode: Option<AnyMode>,
        span: SrcSpan,
    ) -> Operand {
        let (TyKind::Any(inner), Some(mode)) = (self.tcx.kind(declared_ty).clone(), any_mode)
        else {
            return self.lower_operand(expr_id);
        };
        match mode {
            AnyMode::Owned => self.lower_operand(expr_id),
            AnyMode::Ref | AnyMode::RefMut => {
                let place = self.lower_place(expr_id);
                let mutability = if mode == AnyMode::RefMut {
                    Mutability::Mutable
                } else {
                    Mutability::Immutable
                };
                let ref_ty = self.tcx.mk_ref(inner, mutability);
                let temp = self.new_temp(ref_ty, span);
                self.assign(
                    Place::from_local(temp),
                    Rvalue::Ref { mutability, place },
                    span,
                );
                Operand::Move(Place::from_local(temp))
            }
        }
    }

    /// The type arguments a value-position use of a named function was instantiated with, for
    /// `ReifyFnPointer` -- reusing the same resolved-call table a direct call already uses, since
    /// `callee_sig` records a non-generic function's call too (with an empty list).
    pub(crate) fn call_type_args(&self, expr_id: HirId) -> Vec<Ty> {
        self.types
            .call(expr_id)
            .map(|c| c.args.clone())
            .unwrap_or_default()
    }

    /// Materializes a named function as a `fun(T) -> U`-typed value: `Rvalue::Cast` with
    /// `CastKind::ReifyFnPointer`, into a fresh temporary, per the spec's "Operand and Rvalue"
    /// section.
    pub(crate) fn reify_fn_pointer(
        &mut self,
        def: DefId,
        args: Vec<Ty>,
        fn_value_ty: Ty,
        span: SrcSpan,
    ) -> Operand {
        let def_ty = self.types.ty_of_def(def).unwrap_or_else(|| self.tcx.unit());
        let operand = Operand::Constant(Constant {
            ty: def_ty,
            kind: ConstKind::FunDef(def, args, None),
        });
        let temp = self.new_temp(fn_value_ty, span);
        self.assign(
            Place::from_local(temp),
            Rvalue::Cast {
                operand,
                ty: fn_value_ty,
                kind: crate::mir::CastKind::ReifyFunPointer,
            },
            span,
        );
        Operand::Move(Place::from_local(temp))
    }
}
