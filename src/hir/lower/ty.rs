//! Lowers AST types.

use crate::ast;
use crate::hir::TyKind;
use crate::hir::ids::LocalId;
use crate::hir::lower::owner::OwnerLowerer;
use crate::lexer::src_span::SrcSpan;

impl OwnerLowerer<'_> {
    pub(super) fn lower_ty(&mut self, ty: &ast::Type) -> LocalId {
        self.lower_ty_kind(&ty.kind, ty.span)
    }

    pub(super) fn lower_ty_kind(&mut self, kind: &ast::Ty, span: SrcSpan) -> LocalId {
        self.synth_ty(span, |low, _id| match kind {
            ast::Ty::Base { base, args } => TyKind::Base {
                path: base.clone(),
                args: args.iter().map(|a| low.lower_ty_kind(a, span)).collect(),
            },
            ast::Ty::Ref { base, mutability } => TyKind::Ref {
                base: low.lower_ty_kind(base, span),
                mutability: *mutability,
            },
            ast::Ty::Any(base) => TyKind::Any(low.lower_ty_kind(base, span)),
            ast::Ty::Tuple(tys) => {
                TyKind::Tuple(tys.iter().map(|t| low.lower_ty_kind(t, span)).collect())
            }
            ast::Ty::Array { elem, len } => TyKind::Array {
                elem: low.lower_ty_kind(elem, span),
                len: len.as_ref().map(|e| low.lower_expr(e)),
            },
            ast::Ty::Fn { params, ret } => TyKind::Function {
                params: params.iter().map(|p| low.lower_ty_kind(p, span)).collect(),
                ret: ret.as_ref().map(|r| low.lower_ty_kind(r, span)),
            },
            ast::Ty::SelfType => TyKind::SelfType,
            ast::Ty::Dyn(path) => TyKind::Dyn(path.clone()),
            ast::Ty::Error => TyKind::Error,
        })
    }
}
