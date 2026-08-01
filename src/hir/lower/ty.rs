//! Lowers AST types.

use crate::ast;
use crate::hir::ids::LocalId;
use crate::hir::lower::owner::OwnerLowerer;
use crate::hir::TyKind;
use crate::lexer::src_span::SrcSpan;

impl OwnerLowerer<'_> {
    pub(super) fn lower_ty(&mut self, ty: &ast::Ty) -> LocalId {
        self.lower_ty_kind(&ty.kind, ty.span)
    }

    pub(super) fn lower_ty_kind(&mut self, kind: &ast::TyKind, span: SrcSpan) -> LocalId {
        self.synth_ty(span, |low, _id| match kind {
            ast::TyKind::Base { base, args } => TyKind::Base {
                path: base.clone(),
                args: args.iter().map(|a| low.lower_ty(a)).collect(),
            },
            ast::TyKind::Ref { base, mutability } => TyKind::Ref {
                base: low.lower_ty(base),
                mutability: *mutability,
            },
            ast::TyKind::Any(base) => TyKind::Any(low.lower_ty(base)),
            ast::TyKind::Tuple(tys) => TyKind::Tuple(tys.iter().map(|t| low.lower_ty(t)).collect()),
            ast::TyKind::Array { elem, len } => TyKind::Array {
                elem: low.lower_ty(elem),
                len: len.as_ref().map(|e| low.lower_expr(e)),
            },
            ast::TyKind::Function { params, ret } => TyKind::Function {
                params: params.iter().map(|p| low.lower_ty(p)).collect(),
                ret: ret.as_ref().map(|r| low.lower_ty(r)),
            },
            ast::TyKind::SelfType => TyKind::SelfType,
            ast::TyKind::Dyn(path) => TyKind::Dyn(path.clone()),
            ast::TyKind::Error => TyKind::Error,
        })
    }
}
