//! What a path written in the AST resolved to.

use crate::ast::NodeId;

/// A primitive, built-in type such as `i32` or `bool`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum PrimTy {
    I8,
    I16,
    I32,
    I64,
    U8,
    U16,
    U32,
    U64,
    F32,
    F64,
    Bool,
    Char,
}

/// What one written path named.
///
/// `Err` is always recorded. Absence in `NameResolutions` must mean "never reached", not
/// "resolved, unsuccessfully". Conflating them forces consumers to disambiguate from context
/// they may not have.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Res {
    Type(Type),
    Local(Local),
    /// The `Item` wrapping a `fun`. `Function`, like `Struct`/`Enum`/`Trait`/`Extend`, carries
    /// no `NodeId` (it sits inside `Item`, which does; see `src/ast.rs:85`).
    Function(NodeId),
    /// Never constructed today: nothing in the AST writes a path in module position (an
    /// `import`'s path resolves through a dedicated walk instead, not through this `Res`). Kept
    /// for the day something does, rather than narrowing `Res` and having to widen it back.
    #[allow(dead_code)]
    Module(NodeId),
    Err,
}

/// What a path in *type* position named.
///
/// This nests inside `Res` rather than being flattened into it so that a type-position lookup
/// has exactly one return type and consumers narrow once.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Type {
    /// A built-in such as `i32` or `bool`, which never gets a `NodeId`.
    Prim(PrimTy),
    /// A generic type parameter, addressed by the `ast::Generic` that declares it.
    Generic(NodeId),
    Def(TyDef),
}

/// A nominal item. Struct, enum, and trait are combined because all three share one namespace,
/// so a consumer that needs only "a nominal item, give me its `NodeId`" matches a single arm.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TyDef {
    Struct(NodeId),
    Enum(NodeId),
    Trait(NodeId),
}

impl TyDef {
    /// The `NodeId` of the `Item` this names, whichever kind it is.
    pub fn node_id(self) -> NodeId {
        match self {
            TyDef::Struct(id) | TyDef::Enum(id) | TyDef::Trait(id) => id,
        }
    }
}

/// A binding in value position.
///
/// `SelfParam` is kept apart from `Variable` because `self` is not an ordinary local: it
/// carries a `SelfMode` rather than a declared type, and its type is the enclosing item's
/// `Self`. Every consumer handles it specially anyway; a distinct variant forces that to be
/// exhaustive.
///
/// No `Variant` arm exists. A `.variant` names no enum until the expected type is known.
/// Typeck resolves it once it has that type.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Local {
    Param(NodeId),
    SelfParam(NodeId),
    /// The binding `ast::Pat`.
    Variable(NodeId),
}
