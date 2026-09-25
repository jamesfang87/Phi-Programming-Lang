use crate::ast::Mutability;
use crate::driver::source::SrcSpan;
use crate::hir::{AccessArgs, DefId, ExprId, ExprKind, HirId, Res};
use crate::mir::lower::ctx::BodyLowerCtx;
use crate::mir::lower::{Task, is_any_specialized};
use crate::mir::{
    AnyMode, ConstKind, Constant, FunRef, Operand, Place, Projection, Rvalue, TerminatorKind,
};
use crate::typeck::ty::{Ty, TyKind};

struct CallTarget {
    func: Operand,
    receiver: Option<ExprId>,
    arg_exprs: Vec<ExprId>,
    callee_def: Option<DefId>,
    dyn_dispatch: bool,
}

impl<'a> BodyLowerCtx<'a> {
    pub(crate) fn is_any_specialized_call(&self, expr_id: impl Into<HirId>) -> bool {
        let expr_id = expr_id.into();
        let Some(def) = self.find_call_target_def(expr_id) else {
            return false;
        };
        is_any_specialized(self.tcx, self.types, def)
    }

    fn find_call_target_def(&self, expr_id: impl Into<HirId>) -> Option<DefId> {
        let expr_id = expr_id.into();
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
        expr_id: impl Into<HirId>,
        dest: Place,
        mode: AnyMode,
        span: SrcSpan,
    ) {
        let expr_id = expr_id.into();
        let target = self.lower_call_target(expr_id, mode);
        let any_mode = self.select_call_any_mode(&target, mode);
        let args = self.lower_call_operand_list(&target, any_mode, span);
        self.emit_call(expr_id, target.func, args, dest, span);
    }

    fn lower_call_target(&mut self, expr_id: HirId, mode: AnyMode) -> CallTarget {
        match self.hir.expr(expr_id).kind.clone() {
            ExprKind::Call { callee, args } => CallTarget {
                func: self.lower_callee(expr_id, callee, mode),
                receiver: None,
                arg_exprs: args,
                callee_def: self.find_call_target_def(expr_id),
                dyn_dispatch: false,
            },
            ExprKind::Access {
                base,
                args: AccessArgs::Call(args),
                ..
            } => self.receiver_call_target(expr_id, base, args, mode),
            ExprKind::Index { base, index } => {
                self.receiver_call_target(expr_id, base, vec![index], mode)
            }
            _ => unreachable!("lower_call_like_into is only called for Call/Access/Index"),
        }
    }

    fn receiver_call_target(
        &mut self,
        expr_id: HirId,
        base: ExprId,
        args: Vec<ExprId>,
        mode: AnyMode,
    ) -> CallTarget {
        let resolved = self
            .types
            .call(expr_id)
            .unwrap_or_else(|| panic!("mir::lower: {expr_id:?} has no resolved call"))
            .clone();
        let (func, dyn_dispatch) = match self.find_dyn_receiver_ty(base) {
            Some(dyn_ty) => (
                self.dyn_fn_operand(resolved.def, resolved.all_args(), dyn_ty),
                true,
            ),
            None => (
                self.resolved_fn_operand(resolved.def, resolved.all_args(), resolved.self_ty, mode),
                false,
            ),
        };
        CallTarget {
            func,
            receiver: Some(base),
            arg_exprs: args,
            callee_def: Some(resolved.def),
            dyn_dispatch,
        }
    }

    fn select_call_any_mode(&self, target: &CallTarget, mode: AnyMode) -> Option<AnyMode> {
        match target.callee_def {
            Some(def) if !target.dyn_dispatch && is_any_specialized(self.tcx, self.types, def) => {
                Some(mode)
            }
            _ => None,
        }
    }

    fn lower_call_operand_list(
        &mut self,
        target: &CallTarget,
        any_mode: Option<AnyMode>,
        span: SrcSpan,
    ) -> Vec<Operand> {
        match (target.dyn_dispatch, target.callee_def, target.receiver) {
            (true, Some(def), Some(recv)) => {
                self.lower_dyn_call_operands(def, recv, &target.arg_exprs, span)
            }
            (_, Some(def), receiver) => {
                self.lower_call_args(def, any_mode, receiver, &target.arg_exprs, span)
            }
            (_, None, _) => {
                let mut operands = Vec::new();
                if let Some(recv) = target.receiver {
                    operands.push(self.lower_operand(recv));
                }
                operands.extend(target.arg_exprs.iter().map(|&a| self.lower_operand(a)));
                operands
            }
        }
    }

    fn lower_dyn_call_operands(
        &mut self,
        def: DefId,
        receiver: ExprId,
        arg_exprs: &[ExprId],
        span: SrcSpan,
    ) -> Vec<Operand> {
        let function = self.hir.function(def);
        let mut operands = vec![self.lower_operand(receiver)];
        for (i, &arg_expr) in arg_exprs.iter().enumerate() {
            let declared = function
                .params
                .get(i)
                .and_then(|&id| self.types.ty(id))
                .unwrap_or_else(|| self.tcx.error());
            operands.push(self.lower_arg_operand(arg_expr, declared, None, span));
        }
        operands
    }

    fn emit_call(
        &mut self,
        expr_id: HirId,
        func: Operand,
        args: Vec<Operand>,
        dest: Place,
        span: SrcSpan,
    ) {
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

    fn find_dyn_receiver_ty(&mut self, receiver_expr: impl Into<HirId>) -> Option<Ty> {
        let receiver_expr = receiver_expr.into();
        let receiver_ty = self.expr_ty(receiver_expr);
        let (peeled, _) = self.peel_refs(receiver_ty);
        let (peeled, _) = self.peel_any(peeled);
        match self.tcx.kind(peeled).clone() {
            TyKind::Dyn { .. } => Some(peeled),
            _ => None,
        }
    }

    fn dyn_fn_operand(&mut self, def: DefId, args: Vec<Ty>, dyn_ty: Ty) -> Operand {
        let TyKind::Dyn {
            trait_,
            args: trait_args,
        } = self.tcx.kind(dyn_ty).clone()
        else {
            unreachable!("mir::lower: a dyn call operand is built from a `dyn` type");
        };
        let declared = self.types.ty_of_def(def).unwrap_or_else(|| self.tcx.unit());
        let generics = self.hir.trait_(trait_).generics.clone();
        let subst = crate::typeck::ty::visitor::Subst {
            generics: generics
                .iter()
                .copied()
                .zip(trait_args.iter().copied())
                .collect(),
            self_ty: Some(dyn_ty),
        };
        let sig = crate::typeck::ty::visitor::subst_ty(self.tcx, declared, &subst);
        Operand::Constant(Constant {
            ty: sig,
            kind: ConstKind::FunDef(FunRef {
                def,
                args,
                any_mode: None,
                self_ty: Some(dyn_ty),
                trait_method: self.hir.trait_method(def),
            }),
        })
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
        call_expr_id: impl Into<HirId>,
        callee_id: impl Into<HirId>,
        mode: AnyMode,
    ) -> Operand {
        let (call_expr_id, callee_id) = (call_expr_id.into(), callee_id.into());
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
            self.resolved_fn_operand(resolved.def, resolved.all_args(), resolved.self_ty, mode)
        } else {
            let place = self.lower_place(callee_id);
            Operand::Copy(place)
        }
    }

    fn resolved_fn_operand(
        &mut self,
        def: DefId,
        args: Vec<Ty>,
        self_ty: Option<Ty>,
        mode: AnyMode,
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
            kind: ConstKind::FunDef(FunRef {
                def,
                args,
                any_mode,
                self_ty,
                trait_method: self.hir.trait_method(def),
            }),
        })
    }

    fn lower_call_args(
        &mut self,
        def: DefId,
        any_mode: Option<AnyMode>,
        receiver: Option<impl Into<HirId>>,
        arg_exprs: &[ExprId],
        span: SrcSpan,
    ) -> Vec<Operand> {
        let receiver: Option<HirId> = receiver.map(Into::into);
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

        if let TyKind::Any(_) = *self.tcx.kind(recv_ty) {
            let place = self.lower_place(expr_id);
            return match mutability {
                Mutability::Mutable => Operand::Move(place),
                Mutability::Immutable => Operand::Copy(place),
            };
        }
        let (peeled, derefs) = self.peel_refs(recv_ty);
        let mut place = self.lower_place(expr_id);
        for _ in 0..derefs {
            place.projections.push(Projection::Deref);
        }

        let temp_ty = self.tcx.mk_ref(peeled, mutability);
        let temp = self.new_temp(temp_ty, span);
        self.assign(
            Place::from_local(temp),
            Rvalue::Ref { mutability, place },
            span,
        );
        Operand::Move(Place::from_local(temp))
    }

    fn lower_arg_operand(
        &mut self,
        expr_id: impl Into<HirId>,
        declared_ty: Ty,
        any_mode: Option<AnyMode>,
        span: SrcSpan,
    ) -> Operand {
        let expr_id = expr_id.into();
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

    pub(crate) fn call_type_args(&self, expr_id: impl Into<HirId>) -> Vec<Ty> {
        let expr_id = expr_id.into();
        self.types
            .call(expr_id)
            .map(|c| c.all_args())
            .unwrap_or_default()
    }

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
            kind: ConstKind::FunDef(FunRef {
                def,
                args,
                any_mode,
                self_ty: None,
                trait_method: self.hir.trait_method(def),
            }),
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
