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
    Usize,
    Str,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Res {
    Type(Type),
    Local(Local),
    Function(NodeId),
    Module(NodeId),
    /// A path written as `Self`, carrying the type it stands for here.
    ///
    /// Kept apart from [`Res::Type`] holding that same type because typeck has to read a
    /// signature's `Self` as the receiver type at each call site, not as the one type the
    /// defining `extend` block is on. Only [`Resolver::resolve_type_path`] produces this;
    /// [`SymbolTable::lookup_type_path`] answers with the bare [`Type`], since a caller such as
    /// `dyn` resolution only wants what the name denotes.
    ///
    /// [`Resolver::resolve_type_path`]: crate::nameres::resolver::Resolver::resolve_type_path
    /// [`SymbolTable::lookup_type_path`]: crate::nameres::symbol_table::SymbolTable::lookup_type_path
    SelfTy(Type),
    Err,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Type {
    Prim(PrimTy),
    Generic(NodeId),
    Def(TyDef),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TyDef {
    Struct(NodeId),
    Enum(NodeId),
    Trait(NodeId),
}

impl TyDef {
    pub fn node_id(self) -> NodeId {
        match self {
            TyDef::Struct(id) | TyDef::Enum(id) | TyDef::Trait(id) => id,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Local {
    Param(NodeId),
    SelfParam(NodeId),
    Variable(NodeId),
}
