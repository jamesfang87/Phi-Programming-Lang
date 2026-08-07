//! Lowers AST types.

use crate::ast;
use crate::ast::interner::Interner;
use crate::driver::source::SrcSpan;
use crate::hir::lower::owner::OwnerLowerer;
use crate::hir::{HirId, TyKind};

impl OwnerLowerer<'_> {
    pub(super) fn lower_ty(&mut self, ty: &ast::Ty) -> HirId {
        self.lower_ty_kind(&ty.kind, ty.span)
    }

    pub(super) fn lower_ty_kind(&mut self, kind: &ast::TyKind, span: SrcSpan) -> HirId {
        self.synth_ty(span, |low, _id| match kind {
            // The AST parses `Self` as an ordinary single-segment path (so the AST resolver can
            // treat it like any other name), but the HIR still gives it its own `TyKind`: the
            // rest of the HIR pipeline -- name resolution's `self_tys` table, `typeck::self_ty`'s
            // generic-substitution logic -- keys off `Self` being unmistakable at this level, and
            // `Self` can never collide with a real path segment since the lexer always tokenizes
            // the text `Self` as the reserved `UpperSelfKw`, never as an `Identifier`.
            ast::TyKind::Path { path, args } if is_self_path(path) => {
                debug_assert!(args.is_empty(), "the parser never gives `Self` arguments");
                TyKind::SelfType
            }
            ast::TyKind::Path { path, args } => TyKind::Path {
                path: path.clone(),
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
            ast::TyKind::Dyn { path, args } => TyKind::Dyn {
                path: path.clone(),
                args: args.iter().map(|a| low.lower_ty(a)).collect(),
            },
            ast::TyKind::Error => TyKind::Error,
        })
    }
}

/// Whether `path` is exactly the single-segment `Self` path the parser produces for the `Self`
/// keyword -- never true of a user-written path, since the lexer tokenizes `Self` as the
/// reserved `UpperSelfKw`, not as an `Identifier`, so no ordinary path segment can ever carry
/// that text.
fn is_self_path(path: &ast::Path) -> bool {
    match path.segments.as_slice() {
        [segment] => Interner::resolve(segment.text) == "Self",
        _ => false,
    }
}
