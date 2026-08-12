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

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Res {
    Type(Type),
    Local(Local),
    Function(NodeId),
    Module(NodeId),
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
