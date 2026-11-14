use std::collections::HashMap;

use crate::ast::{BinaryOp, Mutability, UnaryOp};
use crate::driver::source::SrcSpan;
use crate::hir::{AccessArgs, DefId, ExprId, ExprKind, HirId, Res};
use crate::langitems::LangItem;
use crate::mir::lower::ctx::BodyLowerCtx;
use crate::mir::lower::{Task, is_any_specialized};
use crate::mir::{
    AnyMode, CastKind, ConstKind, Constant, FunRef, LibcFunction, Operand, Place, Projection,
    Rvalue, TerminatorKind,
};
use crate::nameres::PrimTy;
use crate::typeck::ty::visitor::{self, Subst};
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
        if self.lower_len_intrinsic(expr_id, dest.clone(), span)
            || self.lower_memory_intrinsic(expr_id, dest.clone(), span)
            || self.lower_llvm_memory_intrinsic(expr_id, dest.clone(), span)
            || self.lower_libc_intrinsic(expr_id, dest.clone(), span)
            || self.lower_llvm_operator_intrinsic(expr_id, dest.clone(), span)
            || self.lower_llvm_cast_intrinsic(expr_id, dest.clone(), span)
            || self.lower_llvm_utf8_intrinsic(expr_id, dest.clone(), span)
        {
            return;
        }
        let target = self.lower_call_target(expr_id, mode);
        let any_mode = self.select_call_any_mode(&target, mode);
        let args = self.lower_call_operand_list(&target, any_mode, span);
        self.emit_call(expr_id, target.func, args, dest, span);
    }

    pub(crate) fn lower_operator_call_into(
        &mut self,
        expr_id: impl Into<HirId>,
        receiver: ExprId,
        args: Vec<ExprId>,
        dest: Place,
        span: SrcSpan,
    ) {
        let expr_id = expr_id.into();
        let target = self.receiver_call_target(expr_id, receiver, args, AnyMode::Owned);
        let any_mode = self.select_call_any_mode(&target, AnyMode::Owned);
        let operands = self.lower_call_operand_list(&target, any_mode, span);
        self.emit_call(expr_id, target.func, operands, dest, span);
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
                self.resolved_fn_operand(
                    resolved.def,
                    resolved.all_args(),
                    resolved.trait_args,
                    resolved.self_ty,
                    mode,
                ),
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

    /// Lowers `target[key] = value` into a call to `IndexSet::index_set`, using the resolved call
    /// recorded on the assignment's own expression. Returns whether `lhs` was such an assignment.
    pub(crate) fn lower_index_set_assign(
        &mut self,
        expr_id: HirId,
        lhs: ExprId,
        rhs: ExprId,
        span: SrcSpan,
    ) -> bool {
        let ExprKind::Index { base, index } = self.hir.expr(lhs).kind.clone() else {
            return false;
        };
        let Some(resolved) = self.types.call(expr_id).cloned() else {
            return false;
        };

        let func = self.resolved_fn_operand(
            resolved.def,
            resolved.all_args(),
            resolved.trait_args,
            resolved.self_ty,
            AnyMode::Owned,
        );
        let target = CallTarget {
            func,
            receiver: Some(base),
            arg_exprs: vec![index, rhs],
            callee_def: Some(resolved.def),
            dyn_dispatch: false,
        };
        let any_mode = self.select_call_any_mode(&target, AnyMode::Owned);
        let args = self.lower_call_operand_list(&target, any_mode, span);
        let unit_ty = self.tcx.unit();
        let dest = Place::from_local(self.new_temp(unit_ty, span));
        self.emit_call(expr_id, target.func, args, dest, span);
        true
    }

    /// Lowers a call to one of the length intrinsics into `Rvalue::Len` over the argument's
    /// place. Returns whether `expr_id` named a length intrinsic at all.
    fn lower_len_intrinsic(&mut self, expr_id: HirId, dest: Place, span: SrcSpan) -> bool {
        let Some(def) = self.find_call_target_def(expr_id) else {
            return false;
        };
        let is_len = self.hir.lang_items().get(LangItem::SliceLen) == Some(def)
            || self.hir.lang_items().get(LangItem::StrLen) == Some(def);
        if !is_len {
            return false;
        }

        let ExprKind::Call { args, .. } = self.hir.expr(expr_id).kind.clone() else {
            return false;
        };
        let Some(&arg) = args.first() else {
            return false;
        };

        let place = self.operand_place(arg);
        self.assign(dest, Rvalue::Len(place), span);
        true
    }

    /// Lowers `mem::swap` and `mem::replace` into the rvalues that write through their
    /// references. Returns whether `expr_id` named one of them.
    fn lower_memory_intrinsic(&mut self, expr_id: HirId, dest: Place, span: SrcSpan) -> bool {
        let Some(def) = self.find_call_target_def(expr_id) else {
            return false;
        };
        let is_swap = self.hir.lang_items().get(LangItem::Swap) == Some(def);
        let is_replace = self.hir.lang_items().get(LangItem::Replace) == Some(def);
        if !is_swap && !is_replace {
            return false;
        }

        let ExprKind::Call { args, .. } = self.hir.expr(expr_id).kind.clone() else {
            return false;
        };
        if args.len() != 2 {
            return false;
        }
        let (first, second) = (args[0], args[1]);

        let rvalue = if is_swap {
            Rvalue::Swap {
                left: self.deref_operand_place(first),
                right: self.deref_operand_place(second),
            }
        } else {
            Rvalue::Replace {
                place: self.deref_operand_place(first),
                value: self.lower_operand(second),
            }
        };
        self.assign(dest, rvalue, span);
        true
    }

    /// Lowers `core::llvm`'s operators into the same rvalues the operators produce, so they keep
    /// the checked arithmetic the operators have. Returns whether `expr_id` named one.
    fn lower_llvm_operator_intrinsic(
        &mut self,
        expr_id: HirId,
        dest: Place,
        span: SrcSpan,
    ) -> bool {
        let Some(def) = self.find_call_target_def(expr_id) else {
            return false;
        };

        let binary = [
            (LangItem::LlvmAdd, BinaryOp::Add),
            (LangItem::LlvmSub, BinaryOp::Sub),
            (LangItem::LlvmMul, BinaryOp::Mul),
            (LangItem::LlvmDiv, BinaryOp::Div),
            (LangItem::LlvmRem, BinaryOp::Rem),
            (LangItem::LlvmEq, BinaryOp::Eq),
            (LangItem::LlvmLessThan, BinaryOp::Lt),
        ]
        .into_iter()
        .find(|(item, _)| self.hir.lang_items().get(*item) == Some(def))
        .map(|(_, op)| op);

        if let Some(op) = binary {
            let ExprKind::Call { args, .. } = self.hir.expr(expr_id).kind.clone() else {
                return false;
            };
            if args.len() != 2 {
                return false;
            }
            let operand_ty = self.expr_ty(args[0]);
            let lhs = self.lower_operand(args[0]);
            let rhs = self.lower_operand(args[1]);
            self.lower_binary_op_into(op, lhs, rhs, dest, operand_ty, span);
            return true;
        }

        if self.hir.lang_items().get(LangItem::LlvmNeg) == Some(def) {
            let ExprKind::Call { args, .. } = self.hir.expr(expr_id).kind.clone() else {
                return false;
            };
            let Some(&arg) = args.first() else {
                return false;
            };
            let ty = self.expr_ty(arg);
            let operand = self.lower_operand(arg);
            let is_int = matches!(self.tcx.kind(ty), TyKind::Primitive(prim) if prim.is_integer());
            if self.mode == crate::driver::cli::Mode::Debug && is_int {
                let zero = Operand::Constant(Constant {
                    ty,
                    kind: ConstKind::Int(0),
                });
                self.lower_binary_op_into(BinaryOp::Sub, zero, operand, dest, ty, span);
            } else {
                self.assign(dest, Rvalue::UnaryOp(UnaryOp::Neg, operand), span);
            }
            return true;
        }

        if self.hir.lang_items().get(LangItem::LlvmNot) == Some(def) {
            let ExprKind::Call { args, .. } = self.hir.expr(expr_id).kind.clone() else {
                return false;
            };
            let Some(&arg) = args.first() else {
                return false;
            };
            let operand = self.lower_operand(arg);
            self.assign(dest, Rvalue::UnaryOp(UnaryOp::Not, operand), span);
            return true;
        }

        false
    }

    /// Lowers the LLVM numeric conversions that need an explicit cast. Returns whether `expr_id`
    /// named one of them.
    fn lower_llvm_cast_intrinsic(&mut self, expr_id: HirId, dest: Place, span: SrcSpan) -> bool {
        let Some(def) = self.find_call_target_def(expr_id) else {
            return false;
        };
        let target = [
            (LangItem::LlvmF64ToI64, PrimTy::I64),
            (LangItem::LlvmI64ToF64, PrimTy::F64),
        ]
        .into_iter()
        .find_map(|(item, prim)| (self.hir.lang_items().get(item) == Some(def)).then_some(prim));
        let Some(prim) = target else {
            return false;
        };

        let ExprKind::Call { args, .. } = self.hir.expr(expr_id).kind.clone() else {
            return false;
        };
        let Some(&arg) = args.first() else {
            return false;
        };
        let operand = self.lower_operand(arg);
        let ty = self.tcx.mk_prim(prim);
        self.assign(
            dest,
            Rvalue::Cast {
                operand,
                ty,
                kind: CastKind::Primitive,
            },
            span,
        );
        true
    }

    /// Lowers the UTF-8 encoder. Returns whether `expr_id` named it.
    fn lower_llvm_utf8_intrinsic(&mut self, expr_id: HirId, dest: Place, span: SrcSpan) -> bool {
        let Some(def) = self.find_call_target_def(expr_id) else {
            return false;
        };
        if self.hir.lang_items().get(LangItem::LlvmEncodeUtf8) != Some(def) {
            return false;
        }

        let ExprKind::Call { args, .. } = self.hir.expr(expr_id).kind.clone() else {
            return false;
        };
        if args.len() != 2 {
            return false;
        }
        let value = self.lower_operand(args[0]);
        let buffer = self.lower_operand(args[1]);
        self.assign(dest, Rvalue::EncodeUtf8 { value, buffer }, span);
        true
    }

    /// Lowers the LLVM memory intrinsics that fill, read, and borrow `MaybeUninit` storage.
    /// Returns whether `expr_id` named one of them.
    fn lower_llvm_memory_intrinsic(&mut self, expr_id: HirId, dest: Place, span: SrcSpan) -> bool {
        let Some(def) = self.find_call_target_def(expr_id) else {
            return false;
        };
        let Some(item) = [
            LangItem::WriteUninit,
            LangItem::ReadUninit,
            LangItem::AssumeInitRef,
        ]
        .into_iter()
        .find(|item| self.hir.lang_items().get(*item) == Some(def)) else {
            return false;
        };

        let ExprKind::Call { args, .. } = self.hir.expr(expr_id).kind.clone() else {
            return false;
        };

        let rvalue = match item {
            LangItem::WriteUninit => {
                if args.len() != 2 {
                    return false;
                }
                Rvalue::WriteUninit {
                    place: self.deref_operand_place(args[0]),
                    value: self.lower_operand(args[1]),
                }
            }
            LangItem::ReadUninit | LangItem::AssumeInitRef => {
                let Some(&slot) = args.first() else {
                    return false;
                };
                let place = self.deref_operand_place(slot);
                if item == LangItem::ReadUninit {
                    Rvalue::ReadUninit(place)
                } else {
                    Rvalue::AssumeInitRef(place)
                }
            }
            _ => unreachable!("the match only selects uninitialized-storage intrinsics"),
        };
        self.assign(dest, rvalue, span);
        true
    }

    /// Lowers calls to the C library and to the type-layout queries. Returns whether `expr_id`
    /// named one of them.
    fn lower_libc_intrinsic(&mut self, expr_id: HirId, dest: Place, span: SrcSpan) -> bool {
        let Some(def) = self.find_call_target_def(expr_id) else {
            return false;
        };
        let function = [
            (LangItem::LibcMalloc, LibcFunction::Malloc),
            (LangItem::LibcFree, LibcFunction::Free),
            (LangItem::LibcRealloc, LibcFunction::Realloc),
            (LangItem::LibcMemcpy, LibcFunction::Memcpy),
            (LangItem::LibcMemset, LibcFunction::Memset),
            (LangItem::LibcMemcmp, LibcFunction::Memcmp),
            (LangItem::LibcRead, LibcFunction::Read),
            (LangItem::LibcWrite, LibcFunction::Write),
        ]
        .into_iter()
        .find(|(item, _)| self.hir.lang_items().get(*item) == Some(def))
        .map(|(_, function)| function);

        let ExprKind::Call { args, .. } = self.hir.expr(expr_id).kind.clone() else {
            return false;
        };

        if let Some(function) = function {
            let args = args.iter().map(|&arg| self.lower_operand(arg)).collect();
            self.assign(dest, Rvalue::LibcCall { function, args }, span);
            return true;
        }

        let is_size = self.hir.lang_items().get(LangItem::LibcSizeof) == Some(def);
        let is_align = self.hir.lang_items().get(LangItem::LibcAlignof) == Some(def);
        if !is_size && !is_align {
            return false;
        }
        let Some(&arg) = args.first() else {
            return false;
        };
        let arg_ty = self.expr_ty(arg);
        let (ty, _) = self.peel_refs(arg_ty);
        let rvalue = if is_size {
            Rvalue::SizeOf(ty)
        } else {
            Rvalue::AlignOf(ty)
        };
        self.assign(dest, rvalue, span);
        true
    }

    /// Returns the place a reference-valued expression points at.
    fn deref_operand_place(&mut self, expr_id: ExprId) -> Place {
        let mut place = self.operand_place(expr_id);
        place.projections.push(Projection::Deref);
        place
    }

    /// Returns the place `expr_id` is stored in, materializing a temporary when it is an rvalue.
    fn operand_place(&mut self, expr_id: ExprId) -> Place {
        let operand = self.lower_operand(expr_id);
        match operand {
            Operand::Copy(place) | Operand::Move(place) => place,
            operand @ Operand::Constant(_) => {
                let ty = self.expr_ty(expr_id);
                let span = self.hir.expr(expr_id).span;
                let place = Place::from_local(self.new_temp(ty, span));
                self.assign(place.clone(), Rvalue::Use(operand), span);
                place
            }
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
        let mut peeled = receiver_ty;
        loop {
            peeled = match *self.tcx.kind(peeled) {
                TyKind::Ref { base, .. } | TyKind::Any(base) | TyKind::Iso(base) => base,
                _ => break,
            };
        }
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
                trait_args,
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
            self.resolved_fn_operand(
                resolved.def,
                resolved.all_args(),
                resolved.trait_args,
                resolved.self_ty,
                mode,
            )
        } else {
            let place = self.lower_place(callee_id);
            Operand::Copy(place)
        }
    }

    fn resolved_fn_operand(
        &mut self,
        def: DefId,
        args: Vec<Ty>,
        trait_args: Vec<Ty>,
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
                trait_args,
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
        let recv_ty = self.expr_ty(expr_id);

        let TyKind::Ref { mutability, .. } = *self.tcx.kind(declared_ty) else {
            // A by-value method on an owned pointer takes the pointee, not the pointer.
            if matches!(self.tcx.kind(recv_ty), TyKind::Iso(_)) {
                let mut place = self.lower_place(expr_id);
                place.projections.push(Projection::Deref);
                return self.operand_for_place(place, declared_ty);
            }
            return self.lower_arg_operand(expr_id, declared_ty, any_mode, span);
        };

        if let TyKind::Any(_) = *self.tcx.kind(recv_ty) {
            let place = self.lower_place(expr_id);
            return match mutability {
                Mutability::Mutable => Operand::Move(place),
                Mutability::Immutable => Operand::Copy(place),
            };
        }
        if mutability == Mutability::Mutable && self.is_overloaded_index(expr_id) {
            panic!(
                "mir::lower: a mutable method call through an overloaded `Index` is not yet \
                 implemented"
            );
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

        // An operator's synthesized call passes the right-hand side by value, while the trait
        // method it targets declares `other: &Self`, so borrow the value for that parameter.
        // Ordinary calls already write the borrow themselves, and the borrow's type is a
        // reference, so this only fires for the synthesized operator calls.
        let arg_ty = self.expr_ty(expr_id);
        if matches!(self.tcx.kind(declared_ty), TyKind::Ref { .. })
            && !matches!(self.tcx.kind(arg_ty), TyKind::Ref { .. })
        {
            return self.lower_receiver_operand(expr_id, declared_ty, any_mode, span);
        }

        let (TyKind::Any(inner), Some(mode)) = (self.tcx.kind(declared_ty).clone(), any_mode)
        else {
            return self.lower_operand(expr_id);
        };
        match mode {
            AnyMode::Owned => self.lower_operand(expr_id),
            AnyMode::Ref | AnyMode::RefMut => {
                if matches!(self.tcx.kind(arg_ty), TyKind::Ref { .. }) {
                    return self.lower_operand(expr_id);
                }
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
        let declared_ty = self.types.ty_of_def(def).unwrap_or_else(|| self.tcx.unit());
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
        let args = self.function_value_args(def, declared_ty, fn_value_ty, args);
        let constant_ty = self.subst_function_generics(def, declared_ty, &args);
        let operand = Operand::Constant(Constant {
            ty: constant_ty,
            kind: ConstKind::FunDef(FunRef {
                def,
                args,
                any_mode,
                self_ty: None,
                trait_args: Vec::new(),
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

    /// Returns the types to instantiate `def`'s generic parameters with. It uses the arguments the
    /// checker recorded when there are as many as the function declares, and otherwise recovers
    /// them by matching the declared signature against the type of the function value.
    fn function_value_args(
        &mut self,
        def: DefId,
        declared_ty: Ty,
        value_ty: Ty,
        recorded: Vec<Ty>,
    ) -> Vec<Ty> {
        let generics = &self.hir.function(def).generics;
        if recorded.len() == generics.len() {
            return recorded;
        }

        let bindings = infer_generic_bindings(self.tcx, declared_ty, value_ty);
        generics
            .iter()
            .map(|&param| {
                bindings
                    .get(&param)
                    .copied()
                    .unwrap_or_else(|| self.tcx.mk_generic(param))
            })
            .collect()
    }

    fn subst_function_generics(&mut self, def: DefId, ty: Ty, args: &[Ty]) -> Ty {
        let generics = self.hir.function(def).generics.clone();
        let subst = Subst {
            generics: generics
                .into_iter()
                .zip(args.iter().copied())
                .collect::<HashMap<_, _>>(),
            self_ty: None,
        };
        visitor::subst_ty(self.tcx, ty, &subst)
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

/// Returns the bindings that make `pattern`'s generic parameters match `concrete`. Parameters
/// absent from `pattern` are left unbound.
fn infer_generic_bindings(
    tcx: &mut crate::typeck::ty::ctx::TyCtx,
    pattern: Ty,
    concrete: Ty,
) -> HashMap<HirId, Ty> {
    let mut bindings = HashMap::new();
    let mut pending = vec![(pattern, concrete)];

    while let Some((pattern, concrete)) = pending.pop() {
        if let TyKind::Generic(param) = *tcx.kind(pattern) {
            bindings.insert(param, concrete);
            continue;
        }
        if let Some(components) = visitor::decompose(tcx, pattern, concrete) {
            pending.extend(components);
        }
    }

    bindings
}
