use std::fmt;

use crate::ast::Mutability;
use crate::diagnostics::Diagnostic;
use crate::diagnostics::codes;
use crate::driver::source::SrcSpan;
use crate::hir::{DefId, Hir, OwnerNode};
use crate::nameres::PrimTy;
use crate::session::Session;
use crate::typeck::ty::ctx::TyCtx;
use crate::typeck::ty::unify::UnifyError;
use crate::typeck::ty::{InferVar, Ty, TyKind};

#[derive(Clone, Copy)]
pub struct DisplayCtx<'a> {
    tcx: &'a TyCtx,
    session: &'a Session,
    hir: &'a Hir,
}

impl<'a> DisplayCtx<'a> {
    pub fn new(session: &'a Session, hir: &'a Hir, tcx: &'a TyCtx) -> Self {
        DisplayCtx { tcx, session, hir }
    }

    pub fn emit(&self, diagnostic: Diagnostic) {
        self.session.emit(diagnostic);
    }

    pub fn emit_unify(&self, err: UnifyError, span: SrcSpan, label: impl Into<String>) {
        let diagnostic = Diagnostic::error(self.show(err).to_string(), span)
            .with_code(unify_code(&err))
            .with_label(label);
        self.emit(diagnostic);
    }

    pub fn resolve(&self, symbol: crate::ast::interner::Symbol) -> &'static str {
        self.session.resolve(symbol)
    }

    pub fn session(&self) -> &'a Session {
        self.session
    }

    pub fn show<T: Pretty>(&self, value: T) -> Show<'a, T> {
        Show { cx: *self, value }
    }
}

pub trait Pretty {
    fn pretty(&self, f: &mut fmt::Formatter<'_>, cx: &DisplayCtx<'_>) -> fmt::Result;
}

pub struct Show<'a, T> {
    cx: DisplayCtx<'a>,
    value: T,
}

impl<T: Pretty> fmt::Display for Show<'_, T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.value.pretty(f, &self.cx)
    }
}

impl Pretty for Ty {
    fn pretty(&self, f: &mut fmt::Formatter<'_>, cx: &DisplayCtx<'_>) -> fmt::Result {
        let tcx = cx.tcx;
        match tcx.kind(*self) {
            TyKind::Var(InferVar::Any(_)) => write!(f, "_"),
            TyKind::Var(InferVar::Int(_)) => write!(f, "{{integer}}"),
            TyKind::Var(InferVar::Float(_)) => write!(f, "{{float}}"),

            TyKind::Primitive(prim) => write!(f, "{}", prim_name(*prim)),
            TyKind::Adt { def, args } => {
                write!(f, "{}", def_name(cx.session, cx.hir, *def))?;
                write_args(f, cx, args)
            }
            TyKind::Generic(hir_id) => write!(
                f,
                "{}",
                cx.session.resolve(cx.hir.generic(*hir_id).name.text)
            ),

            TyKind::SelfTy(_) => write!(f, "Self"),
            TyKind::Ref { base, mutability } => {
                match mutability {
                    Mutability::Immutable => write!(f, "&")?,
                    Mutability::Mutable => write!(f, "&mut ")?,
                }
                base.pretty(f, cx)
            }
            TyKind::Any(base) => {
                write!(f, "any ")?;
                base.pretty(f, cx)
            }
            TyKind::Iso(base) => {
                write!(f, "iso ")?;
                base.pretty(f, cx)
            }
            TyKind::Unit => write!(f, "()"),
            TyKind::Tuple(elems) => {
                write!(f, "(")?;
                for (i, elem) in elems.iter().enumerate() {
                    if i > 0 {
                        write!(f, ", ")?;
                    }
                    elem.pretty(f, cx)?;
                }

                if elems.len() == 1 {
                    write!(f, ",")?;
                }
                write!(f, ")")
            }
            TyKind::Array { elem, .. } => {
                write!(f, "[")?;
                elem.pretty(f, cx)?;
                write!(f, "; _]")
            }
            TyKind::Fun { params, ret } => {
                write!(f, "fun(")?;
                for (i, param) in params.iter().enumerate() {
                    if i > 0 {
                        write!(f, ", ")?;
                    }
                    param.pretty(f, cx)?;
                }
                write!(f, ")")?;
                if let Some(ret) = ret {
                    write!(f, " -> ")?;
                    ret.pretty(f, cx)?;
                }
                Ok(())
            }
            TyKind::Dyn { trait_, args } => {
                write!(f, "dyn {}", def_name(cx.session, cx.hir, *trait_))?;
                write_args(f, cx, args)
            }
            TyKind::Never => write!(f, "!"),
            TyKind::Error => write!(f, "{{error}}"),
        }
    }
}

impl Pretty for UnifyError {
    fn pretty(&self, f: &mut fmt::Formatter<'_>, cx: &DisplayCtx<'_>) -> fmt::Result {
        match *self {
            UnifyError::Mismatch { expected, found } => write!(
                f,
                "mismatched types: expected `{}`, found `{}`",
                cx.show(expected),
                cx.show(found)
            ),
            UnifyError::ExpectedInteger { found, .. } => write!(
                f,
                "mismatched types: expected an integer type, found `{}`",
                cx.show(found)
            ),
            UnifyError::ExpectedFloat { found, .. } => write!(
                f,
                "mismatched types: expected a float type, found `{}`",
                cx.show(found)
            ),
            UnifyError::Infinite { var, found } => write!(
                f,
                "cyclic type of infinite size: `{}` would have to contain itself, as `{}`",
                cx.show(var),
                cx.show(found)
            ),
        }
    }
}

fn unify_code(err: &UnifyError) -> &'static str {
    match err {
        UnifyError::Mismatch { .. } => codes::MISMATCHED_TYPES,
        UnifyError::ExpectedInteger { .. } => codes::EXPECTED_INTEGER,
        UnifyError::ExpectedFloat { .. } => codes::EXPECTED_FLOAT,
        UnifyError::Infinite { .. } => codes::INFINITE_TYPE,
    }
}

fn write_args(f: &mut fmt::Formatter<'_>, cx: &DisplayCtx<'_>, args: &[Ty]) -> fmt::Result {
    if args.is_empty() {
        return Ok(());
    }
    write!(f, "<")?;
    for (i, arg) in args.iter().enumerate() {
        if i > 0 {
            write!(f, ", ")?;
        }
        arg.pretty(f, cx)?;
    }
    write!(f, ">")
}

fn prim_name(prim: PrimTy) -> &'static str {
    match prim {
        PrimTy::I8 => "i8",
        PrimTy::I16 => "i16",
        PrimTy::I32 => "i32",
        PrimTy::I64 => "i64",
        PrimTy::U8 => "u8",
        PrimTy::U16 => "u16",
        PrimTy::U32 => "u32",
        PrimTy::U64 => "u64",
        PrimTy::F32 => "f32",
        PrimTy::F64 => "f64",
        PrimTy::Bool => "bool",
        PrimTy::Char => "char",
        PrimTy::Usize => "usize",
        PrimTy::Str => "str",
    }
}

pub(crate) fn def_name(session: &Session, hir: &Hir, def_id: DefId) -> &'static str {
    match hir.def(def_id) {
        OwnerNode::Struct(s) => session.resolve(s.name.text),
        OwnerNode::Enum(e) => session.resolve(e.name.text),
        OwnerNode::Trait(t) => session.resolve(t.name.text),
        _ => unreachable!("only a struct, enum, or trait def can appear in an Adt or Dyn type"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::nameres::PrimTy;
    use crate::testing::{session, typecheck_only};

    #[test]
    fn a_float_mismatch_names_the_expected_kind() {
        let (hir, mut tcx, _types) = typecheck_only("fun f() {}");
        let var = tcx.next_infer_var();
        let found = tcx.mk_prim(PrimTy::Bool);
        let cx = DisplayCtx::new(session(), &hir, &tcx);
        session().clear_diagnostics();

        cx.emit_unify(
            UnifyError::ExpectedFloat { var, found },
            SrcSpan::new(0, 1),
            "expected a float",
        );

        assert_eq!(
            session().messages(),
            ["mismatched types: expected a float type, found `bool`"]
        );
    }

    #[test]
    fn an_infinite_type_is_rendered_with_both_types() {
        let (hir, mut tcx, _types) = typecheck_only("fun f() {}");
        let var = tcx.mk_prim(PrimTy::I32);
        let found = tcx.mk_tuple(vec![var]);
        let cx = DisplayCtx::new(session(), &hir, &tcx);
        session().clear_diagnostics();

        cx.emit_unify(
            UnifyError::Infinite { var, found },
            SrcSpan::new(0, 1),
            "cyclic",
        );

        assert_eq!(
            session().messages(),
            ["cyclic type of infinite size: `i32` would have to contain itself, as `(i32,)`"]
        );
    }

    #[test]
    fn every_primitive_type_has_a_name() {
        let (hir, mut tcx, _types) = typecheck_only("fun f() {}");
        let tys: Vec<Ty> = [
            PrimTy::I8,
            PrimTy::I16,
            PrimTy::I32,
            PrimTy::I64,
            PrimTy::U8,
            PrimTy::U16,
            PrimTy::U32,
            PrimTy::U64,
            PrimTy::F32,
            PrimTy::F64,
            PrimTy::Bool,
            PrimTy::Char,
            PrimTy::Usize,
            PrimTy::Str,
        ]
        .into_iter()
        .map(|prim| tcx.mk_prim(prim))
        .collect();
        let cx = DisplayCtx::new(session(), &hir, &tcx);
        let names: Vec<String> = tys.into_iter().map(|ty| cx.show(ty).to_string()).collect();

        assert_eq!(
            names,
            [
                "i8", "i16", "i32", "i64", "u8", "u16", "u32", "u64", "f32", "f64", "bool", "char",
                "usize", "str"
            ]
        );
    }
}
