use crate::ast::Ident;
use crate::driver::source::SrcSpan;
use crate::hir::ids::{DefId, HirId};
use crate::nameres::PrimTy;

/// hir::Path also contains the result that the Path resolves to, which we
/// previously determined during name resolution on the Ast.
#[derive(Clone, Debug)]
pub struct Path {
    pub segments: Vec<Ident>,
    pub span: SrcSpan,
    pub res: Res,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Res {
    Type(Type),
    Local(Local),
    Function(DefId),
    Module(DefId),
    /// We keep Self separate, unlike nameres::Res, since it must behave
    /// differently for typeck
    SelfTy(TyDef),
    Err,
}

/// What a path in *type* position named.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Type {
    Prim(PrimTy),
    Generic(HirId),
    Def(TyDef),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TyDef {
    Struct(DefId),
    Enum(DefId),
    Trait(DefId),
}

impl TyDef {
    pub fn def_id(self) -> DefId {
        match self {
            TyDef::Struct(id) | TyDef::Enum(id) | TyDef::Trait(id) => id,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Local {
    Param(HirId),
    SelfParam(HirId),
    Variable(HirId),
}
