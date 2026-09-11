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

impl PrimTy {
    /// Whether this is one of the signed or unsigned integer types (`usize` included).
    pub(crate) fn is_integer(self) -> bool {
        matches!(
            self,
            PrimTy::I8
                | PrimTy::I16
                | PrimTy::I32
                | PrimTy::I64
                | PrimTy::U8
                | PrimTy::U16
                | PrimTy::U32
                | PrimTy::U64
                | PrimTy::Usize
        )
    }

    /// Whether this is one of the IEEE-754 floating-point types.
    pub(crate) fn is_float(self) -> bool {
        matches!(self, PrimTy::F32 | PrimTy::F64)
    }

    /// Whether this is a signed integer type.
    pub(crate) fn is_signed(self) -> bool {
        matches!(self, PrimTy::I8 | PrimTy::I16 | PrimTy::I32 | PrimTy::I64)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Res {
    Type(Type),
    Local(Local),
    Function(NodeId),
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
