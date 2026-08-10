//! `hir::Path` and the `Res` family it carries: the `HirId`/`DefId`-addressed analogue of
//! `nameres::Res`.
//!
//! Resolution runs on the AST, before lowering (see `crate::nameres`). Rather than re-key that
//! answer into a HIR-side side table -- which would need a second key alongside `HirId` for the
//! one case a per-node table can't express, `Extend`'s `adt_path` and `trait_path`, two paths on
//! one node with no `HirId` of their own (`src/hir/items.rs`) -- every `hir::Path` simply carries
//! its own answer inline. That is what gives `Extend`'s second path somewhere to live, and it is
//! the entire reason this migration is shaped as "a new field on `Path`" rather than "a new
//! table."

use crate::ast::Ident;
use crate::driver::source::SrcSpan;
use crate::hir::ids::{DefId, HirId};
use crate::nameres::PrimTy;

/// A path written in the HIR, with the answer AST-level name resolution gave it already
/// attached.
///
/// `segments` and `span` are kept, not discarded once `res` is known, because diagnostics and
/// the debug dump still need the written name and its source location -- `res` augments what a
/// `Path` carries, it doesn't replace the rest of it.
#[derive(Debug)]
pub struct Path {
    pub segments: Vec<Ident>,
    pub span: SrcSpan,
    pub res: Res,
}

/// What one written path named, mirroring `nameres::Res` arm for arm but addressing HIR nodes:
/// a `DefId` for a nominal item or function, since those *are* definitions, and a `HirId` for a
/// local or a generic, since those are arena nodes with no `DefId` of their own
/// (`src/hir/ids.rs`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Res {
    Type(Type),
    Local(Local),
    Function(DefId),
    Module(DefId),
    /// The path was written as the keyword `Self`, which resolved to the struct, enum, or trait
    /// that introduces it.
    ///
    /// Kept apart from `Type(Type::Def(_))`, which is what `Self` resolves to at the AST level
    /// (see `SymbolTable::lookup_type_path`), because a `Self` and an ordinary named type are
    /// not interchangeable once lowering reaches type-checking: `Self` may be written with no
    /// argument list even inside `struct Foo<T>`, and is legal on its own inside a trait body
    /// where a bare trait name in type position is not. `lower_base`'s `Def` arm enforces both
    /// of those for an ordinary path; its `SelfTy` arm -- moved across unchanged from what used
    /// to be `self_ty`'s standalone table lookup -- exists because that enforcement doesn't
    /// apply to `Self`. See `LoweringCtx::as_self_ty` for where this arm gets produced.
    SelfTy(TyDef),
    Err,
}

/// What a path in *type* position named.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Type {
    /// A built-in such as `i32` or `bool`, which never gets an id of its own.
    Prim(PrimTy),
    /// A generic type parameter, addressed by the `Node::Generic` that declares it.
    Generic(HirId),
    Def(TyDef),
}

/// A nominal item. Struct, enum, and trait are combined because all three share one namespace,
/// so a consumer that needs only "a nominal item, give me its `DefId`" matches a single arm.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TyDef {
    Struct(DefId),
    Enum(DefId),
    Trait(DefId),
}

impl TyDef {
    /// The `DefId` this names, whichever kind it is.
    pub fn def_id(self) -> DefId {
        match self {
            TyDef::Struct(id) | TyDef::Enum(id) | TyDef::Trait(id) => id,
        }
    }
}

/// A binding in value position.
///
/// `SelfParam` is kept apart from `Variable` because `self` is not an ordinary local: it
/// carries a `SelfMode` rather than a declared type, and its type is the enclosing item's
/// `Self`.
///
/// There is deliberately no `Variant` arm, for the same reason `nameres::Local` has none: a
/// `.variant` names no enum of its own until typeck knows the expected type.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Local {
    Param(HirId),
    SelfParam(HirId),
    Variable(HirId),
}
