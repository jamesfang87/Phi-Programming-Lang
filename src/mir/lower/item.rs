use crate::ast::Mutability;
use crate::hir::{DefId, Hir, HirId, OwnerNode};
use crate::mir::lower::Task;
use crate::mir::lower::ctx::BodyLowerCtx;
use crate::mir::{AnyMode, Body, BodyKind, Place, TerminatorKind};
use crate::typeck::ty::{Ty, TyKind};

impl<'a> BodyLowerCtx<'a> {
    pub(crate) fn lower_item(&mut self, task: Task) -> Body {
        let any_mode = task.any_mode();
        self.generics = instance_generics(self.hir, self.def_id);
        match self.hir.def(self.def_id) {
            OwnerNode::Function(function) => {
                self.kind = BodyKind::Function;
                let self_param = function.self_param;
                let params = function.params.clone();
                let block = function
                    .block
                    .expect("mir::lower only seeds functions that have a body");
                let span = function.span;
                self.lower_function_like(self_param, &params, block, span, any_mode)
            }
            OwnerNode::Closure(closure) => {
                self.kind = BodyKind::Closure;
                let params = closure.params.clone();
                let block = closure.block;
                let span = closure.span;
                self.lower_closure_body(&params, block, span)
            }
            other => unreachable!(
                "mir::lower only seeds functions and closures as tasks, found {:?}",
                other.kind_name()
            ),
        }
    }

    fn lower_function_like(
        &mut self,
        self_param: Option<HirId>,
        params: &[HirId],
        block: impl Into<HirId>,
        span: crate::driver::source::SrcSpan,
        any_mode: Option<AnyMode>,
    ) -> Body {
        let block = block.into();
        let ret_ty = self.return_ty(any_mode);

        self.new_local(ret_ty, None, span);

        if let Some(self_id) = self_param {
            let ty = self.resolve_any(
                self.types.ty(self_id).expect("self param is typed"),
                any_mode,
            );
            let local = self.new_local(ty, None, span);
            self.bind_local(self_id, local);
        }
        for &param_id in params {
            let ty = self.resolve_any(self.types.ty(param_id).expect("param is typed"), any_mode);
            let param = self.hir.param(param_id);
            let local = self.new_local(ty, Some(param.name), param.span);
            self.bind_local(param_id, local);
        }
        let arg_count = usize::from(self_param.is_some()) + params.len();

        self.lower_body_block(block, arg_count, span)
    }

    fn lower_closure_body(
        &mut self,
        params: &[HirId],
        block: impl Into<HirId>,
        span: crate::driver::source::SrcSpan,
    ) -> Body {
        let block = block.into();
        let ret_ty = self.return_ty(None);
        self.new_local(ret_ty, None, span);

        let captures = self.collect_closure_captures(self.def_id);
        let env_ty = self.build_environment_ty(&captures);
        let env_local = self.new_local(env_ty, None, span);

        for &param_id in params {
            let ty = self.types.ty(param_id).expect("closure param is typed");
            let param = self.hir.closure_param(param_id);
            let local = self.new_local(ty, Some(param.name), param.span);
            self.bind_local(param_id, local);
        }
        let arg_count = 1 + params.len();

        self.bind_environment(env_local, &captures);
        self.lower_body_block(block, arg_count, span)
    }

    fn lower_body_block(
        &mut self,
        block: HirId,
        arg_count: usize,
        span: crate::driver::source::SrcSpan,
    ) -> Body {
        let dest = Place::from_local(crate::mir::Local::RETURN_PLACE);
        self.lower_block(block, Some(dest));
        self.set_terminator(TerminatorKind::Return, span);
        self.finish(arg_count, span)
    }

    fn return_ty(&mut self, any_mode: Option<AnyMode>) -> Ty {
        let sig = self
            .types
            .ty_of_def(self.def_id)
            .expect("a signature is recorded before the body it belongs to is lowered");
        let TyKind::Fun { ret, .. } = self.tcx.kind(sig).clone() else {
            unreachable!("a function's or closure's own signature always lowers to TyKind::Fun");
        };
        let ret = ret.unwrap_or_else(|| self.tcx.unit());
        self.resolve_any(ret, any_mode)
    }

    pub(crate) fn resolve_any(&mut self, ty: Ty, any_mode: Option<AnyMode>) -> Ty {
        let TyKind::Any(inner) = *self.tcx.kind(ty) else {
            return ty;
        };
        match any_mode {
            None | Some(AnyMode::Owned) => inner,
            Some(AnyMode::Ref) => self.tcx.mk_ref(inner, Mutability::Immutable),
            Some(AnyMode::RefMut) => self.tcx.mk_ref(inner, Mutability::Mutable),
        }
    }
}

fn instance_generics(hir: &Hir, def_id: DefId) -> Vec<HirId> {
    let mut generics = Vec::new();
    if let Some(parent) = hir.parent(def_id) {
        match hir.def(parent) {
            OwnerNode::Extend(extend) => generics.extend(extend.extend_generics.iter().copied()),
            OwnerNode::Trait(trait_) => generics.extend(trait_.generics.iter().copied()),
            _ => {}
        }
    }
    if let OwnerNode::Function(function) = hir.def(def_id) {
        generics.extend(function.generics.iter().copied());
    }
    generics
}
