use crate::ast::Mutability;
use crate::driver::source::SrcSpan;
use crate::hir::{AccessArgs, DefId, ExprKind, HirId, Res};
use crate::mir::lower::ctx::BodyLowerCtx;
use crate::mir::lower::{Task, is_any_specialized};
use crate::mir::{
    AnyMode, ConstKind, Constant, Operand, Place, Projection, Rvalue, TerminatorKind,
};
use crate::typeck::ty::{Ty, TyKind};

impl<'a> BodyLowerCtx<'a> {
    pub(crate) fn is_any_specialized_call(&self, expr_id: HirId) -> bool {
        let Some(def) = self.call_target_def(expr_id) else {
            return false;
        };
        is_any_specialized(self.tcx, self.types, def)
    }

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

    fn peel_any(&self, ty: Ty) -> (Ty, u32) {
        let mut current = ty;
        let mut count = 0;
        while let TyKind::Any(base) = *self.tcx.kind(current) {
            current = base;
            count += 1;
        }
        (current, count)
    }

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
            self.push_stmt(crate::mir::StatementKind::CheckMutable(place.clone()), span);
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
    ///
    /// `def`'s own signature may still carry unresolved `any` positions here -- typeck does not
    /// resolve them when a named function is used as a bare value rather than called outright, so
    /// `fn_value_ty` (computed from that signature) can too. A call site picks `any`'s mode from
    /// how the call's own result is used, and a bare reference like this is not a call at all, so
    /// there is no such usage to consult. Rather than reject the reference, this pins it to
    /// `AnyMode::Owned` -- every `any` position becomes its plain `T` -- the same fallback
    /// `resolve_any` already gives a definition that is not `any`-specialized at all, so an
    /// indirect call through the resulting pointer always finds a compiled body.
    pub(crate) fn reify_fn_pointer(
        &mut self,
        def: DefId,
        args: Vec<Ty>,
        fn_value_ty: Ty,
        span: SrcSpan,
    ) -> Operand {
        let def_ty = self.types.ty_of_def(def).unwrap_or_else(|| self.tcx.unit());
        let any_mode = if is_any_specialized(self.tcx, self.types, def) {
            self.discover(Task::AnySpecialized(def, AnyMode::Owned));
            Some(AnyMode::Owned)
        } else {
            None
        };
        let fn_value_ty = match any_mode {
            Some(mode) => self.resolve_any_fn_ty(fn_value_ty, mode),
            None => fn_value_ty,
        };
        let operand = Operand::Constant(Constant {
            ty: def_ty,
            kind: ConstKind::FunDef(def, args, any_mode),
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

    /// Resolves every `any` position in a `fun(..) -> ..`-shaped type under `mode`, the same way
    /// [`BodyLowerCtx::resolve_any`] resolves one position at a time for a parameter or return
    /// type when lowering a definition's own body. A reified function pointer's type is built
    /// from the same signature a body is lowered from, so it needs the same treatment applied
    /// across every parameter and the return type at once.
    fn resolve_any_fn_ty(&mut self, fn_ty: Ty, mode: AnyMode) -> Ty {
        let TyKind::Fun { params, ret } = self.tcx.kind(fn_ty).clone() else {
            return fn_ty;
        };
        let params = params
            .into_iter()
            .map(|p| self.resolve_any(p, Some(mode)))
            .collect();
        let ret = ret.map(|r| self.resolve_any(r, Some(mode)));
        self.tcx.mk_fun(params, ret)
    }
}
