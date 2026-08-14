//! [`BodyLowerCtx::lower_item`], the entry point that lowers one whole definition -- a
//! function, a method, or a closure -- into its finished [`Body`]: setting up the return place
//! and every parameter's local, lowering the block, and closing off whichever basic block
//! lowering the block left open with an implicit `Return`.

use crate::ast::Mutability;
use crate::hir::{HirId, OwnerNode};
use crate::mir::lower::Task;
use crate::mir::lower::ctx::BodyLowerCtx;
use crate::mir::{AnyMode, Body, Place, TerminatorKind};
use crate::typeck::ty::{Ty, TyKind};

impl<'a> BodyLowerCtx<'a> {
    pub(crate) fn lower_item(&mut self, task: Task) -> Body {
        let any_mode = task.any_mode();
        match self.hir.def(self.def_id) {
            OwnerNode::Function(function) => {
                let self_param = function.self_param;
                let params = function.params.clone();
                let block = function
                    .block
                    .expect("mir::lower only seeds functions that have a body");
                let span = function.span;
                self.lower_function_like(self_param, &params, block, span, any_mode)
            }
            OwnerNode::Closure(closure) => {
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

    /// Shared by a free function and a method: both are an optional `self` parameter, an
    /// ordinary parameter list, and a block, with no environment to thread through the way a
    /// closure has.
    fn lower_function_like(
        &mut self,
        self_param: Option<HirId>,
        params: &[HirId],
        block: HirId,
        span: crate::driver::source::SrcSpan,
        any_mode: Option<AnyMode>,
    ) -> Body {
        let ret_ty = self.return_ty(any_mode);
        // Slot 0: the return place, by the convention every `Body` follows.
        self.new_local(ret_ty, Mutability::Mutable, None, span);

        if let Some(self_id) = self_param {
            let ty = self.resolve_any(
                self.types.ty(self_id).expect("self param is typed"),
                any_mode,
            );
            let local = self.new_local(ty, Mutability::Immutable, None, span);
            self.bind_local(self_id, local);
        }
        for &param_id in params {
            let ty = self.resolve_any(self.types.ty(param_id).expect("param is typed"), any_mode);
            let name = self.hir.param(param_id).name;
            let local = self.new_local(ty, Mutability::Immutable, Some(name), span);
            self.bind_local(param_id, local);
        }
        let arg_count = usize::from(self_param.is_some()) + params.len();

        self.lower_body_block(block, arg_count, span)
    }

    /// A closure's environment is always given an implicit local at the front of its declared
    /// parameters, per the spec's "Closures" section, whether or not this particular closure
    /// captures anything -- a uniform calling convention is simpler than a conditional one, and
    /// an empty environment costs nothing at the value level either (see `ReifyFnPointer`'s
    /// discussion of an absent environment pointer).
    fn lower_closure_body(
        &mut self,
        params: &[HirId],
        block: HirId,
        span: crate::driver::source::SrcSpan,
    ) -> Body {
        let ret_ty = self.return_ty(None);
        self.new_local(ret_ty, Mutability::Mutable, None, span);

        let captures = self.captures_of(self.def_id);
        let env_ty = self.environment_ty(&captures);
        let env_local = self.new_local(env_ty, Mutability::Immutable, None, span);

        for &param_id in params {
            let ty = self.types.ty(param_id).expect("closure param is typed");
            let name = self.hir.closure_param(param_id).name;
            let local = self.new_local(ty, Mutability::Immutable, Some(name), span);
            self.bind_local(param_id, local);
        }
        let arg_count = 1 + params.len();

        self.bind_environment(env_local, &captures);
        self.lower_body_block(block, arg_count, span)
    }

    /// Lowers `block` as this body's whole executable content, its trailing expression (if it
    /// has one) becoming the return place's value, then closes off whichever basic block that
    /// lowering left open with an implicit `Return`. `lower_block` (which this calls) always
    /// leaves the "current" block unterminated when it returns -- a diverging path inside it
    /// (`return`, `break`, `continue`) switches to a fresh open block of its own right after
    /// setting its own terminator -- so this can set `Return` unconditionally, without needing
    /// to check whether the block already ended some other way.
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

    /// This task's return type, with `any T` resolved per `any_mode` (or, absent one, resolved
    /// as the plain owned type `any` wraps -- see [`BodyLowerCtx::resolve_any`]).
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

    /// Resolves an `any`-marked type to what it concretely is under `any_mode`: `T` for
    /// `Owned`, `&T` for `Ref`, `&mut T` for `RefMut`. `any_mode` is `None` for a task that is
    /// not `any`-specialized at all, in which case an `any T` position resolves as the plain
    /// owned `T` it wraps -- the README's rule that `any` "has no effect" outside an
    /// `any`-returning definition. A type that is not `any` at all passes through unchanged.
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
