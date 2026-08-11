//! A user-facing rendering for the checker's types, meant for diagnostics: `i32`, `&mut
//! Foo<T>`, `(bool, char)`, `fun(i32) -> bool`, the same surface syntax the user would have
//! written. This is deliberately distinct from
//! [`emit_debug::print_typeck`](crate::driver::emit_debug::print_typeck)'s formatting, which
//! dumps a type's internal structure (`Adt { def: ..., args: [...] }`) for the `--debug` output
//! rather than reading like source.
//!
//! Rendering a [`Ty`] needs the [`TyCtx`] that interned it and the [`Hir`] that names the
//! definitions inside it. [`DisplayCx`] carries that pair once, so neither the trait nor its
//! implementations thread it by hand, and anything else worth showing to a user -- a
//! [`UnifyError`], say -- implements [`Pretty`] and prints the same way.

use std::fmt;

use crate::ast::Mutability;
use crate::ast::interner::Interner;
use crate::hir::{DefId, Hir, HirId, OwnerNode};
use crate::nameres::PrimTy;
use crate::typeck::ty::{Ty, TyKind, TyVar};
use crate::typeck::tyctx::TyCtx;
use crate::typeck::unify::UnifyError;

/// What is needed to turn the checker's internal handles back into the words the user wrote.
///
/// Borrows for only as long as it takes to format, so build one where it is printed rather than
/// holding on to it.
#[derive(Clone, Copy)]
pub struct DisplayCx<'a> {
    hir: &'a Hir,
    tcx: &'a TyCtx,
}

impl<'a> DisplayCx<'a> {
    pub fn new(hir: &'a Hir, tcx: &'a TyCtx) -> Self {
        DisplayCx { hir, tcx }
    }

    /// Wraps `value` so it can be printed: `format!("{}", cx.show(ty))`.
    pub fn show<T: Pretty>(&self, value: T) -> Show<'a, T> {
        Show { cx: *self, value }
    }
}

/// Something the user should see rendered as they would have written it.
///
/// Kept separate from [`fmt::Display`] because none of these can be printed on their own: a
/// [`Ty`] is an index into a [`TyCtx`] and means nothing without one.
pub trait Pretty {
    fn pretty(&self, f: &mut fmt::Formatter<'_>, cx: &DisplayCx<'_>) -> fmt::Result;
}

/// A [`Pretty`] value paired with the context needed to render it. Returned by
/// [`DisplayCx::show`].
pub struct Show<'a, T> {
    cx: DisplayCx<'a>,
    value: T,
}

impl<T: Pretty> fmt::Display for Show<'_, T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.value.pretty(f, &self.cx)
    }
}

impl Pretty for Ty {
    fn pretty(&self, f: &mut fmt::Formatter<'_>, cx: &DisplayCx<'_>) -> fmt::Result {
        let (hir, tcx) = (cx.hir, cx.tcx);
        match tcx.kind(*self) {
            // An inference variable that remained unbound after type checking cannot be
            // displayed as an internal handle. These placeholders match rustc's conventions
            // for rendering unsolved variables to users.
            TyKind::Var(TyVar::Any(_)) => write!(f, "_"),
            TyKind::Var(TyVar::Int(_)) => write!(f, "{{integer}}"),
            TyKind::Var(TyVar::Float(_)) => write!(f, "{{float}}"),

            TyKind::Primitive(prim) => write!(f, "{}", prim_name(*prim)),
            TyKind::Adt { def, args } => {
                write!(f, "{}", def_name(hir, *def))?;
                write_args(f, cx, args)
            }
            TyKind::Generic(hir_id) => write!(f, "{}", generic_name(hir, *hir_id)),
            // Only appears inside a trait's own body, where `Self` names no concrete type yet.
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
            TyKind::Unit => write!(f, "()"),
            TyKind::Tuple(elems) => {
                write!(f, "(")?;
                for (i, elem) in elems.iter().enumerate() {
                    if i > 0 {
                        write!(f, ", ")?;
                    }
                    elem.pretty(f, cx)?;
                }
                // `(T,)` disambiguates a one-element tuple from a merely parenthesized `T`,
                // exactly as in the language's own surface syntax.
                if elems.len() == 1 {
                    write!(f, ",")?;
                }
                write!(f, ")")
            }
            // `len` addresses an unevaluated constant expression rather than a number (see
            // `TyKind::Array`), so there's no value here yet to print.
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
                write!(f, "dyn {}", def_name(hir, *trait_))?;
                write_args(f, cx, args)
            }
            TyKind::Never => write!(f, "!"),
            TyKind::Error => write!(f, "{{error}}"),
        }
    }
}

/// Each variant's wording lives next to the variant it explains, so adding a way for
/// unification to fail comes with the sentence that reports it.
impl Pretty for UnifyError {
    fn pretty(&self, f: &mut fmt::Formatter<'_>, cx: &DisplayCx<'_>) -> fmt::Result {
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
            UnifyError::Infinite { var, ty } => write!(
                f,
                "cyclic type of infinite size: `{}` would have to contain itself, as `{}`",
                cx.show(var),
                cx.show(ty)
            ),
        }
    }
}

fn write_args(f: &mut fmt::Formatter<'_>, cx: &DisplayCx<'_>, args: &[Ty]) -> fmt::Result {
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

/// The keyword a primitive type is written with, exactly as
/// [`nameres::symbol_table::prim_ty`](crate::nameres::symbol_table::prim_ty) recognizes it.
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
    }
}

/// The name a `struct`, `enum`, or `trait` was declared with -- the only three `OwnerNode`s a
/// `TyKind::Adt` or `TyKind::Dyn` can point at.
///
/// `pub(crate)` because a trait in a diagnostic has no [`Ty`] to render through [`Pretty`]: a
/// trait is a `DefId`, and only [`TyKind::Dyn`] ever wraps one in a type.
/// [`require_extends`](crate::typeck::Typeck::require_extends) holds the bare `DefId` and needs
/// the declared name for its message.
pub(crate) fn def_name(hir: &Hir, def_id: DefId) -> &'static str {
    match hir.def(def_id) {
        OwnerNode::Struct(s) => Interner::resolve(s.name.text),
        OwnerNode::Enum(e) => Interner::resolve(e.name.text),
        OwnerNode::Trait(t) => Interner::resolve(t.name.text),
        _ => unreachable!("only a struct, enum, or trait def can appear in an Adt or Dyn type"),
    }
}

/// The name a generic parameter was declared with.
fn generic_name(hir: &Hir, hir_id: HirId) -> &'static str {
    Interner::resolve(hir.generic(hir_id).name.text)
}
