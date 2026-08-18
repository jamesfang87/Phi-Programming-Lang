use crate::ast::Mutability;
use crate::hir::{DefId, HirId};
use crate::nameres::PrimTy;

#[derive(Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Debug)]
pub struct Ty(u32);

impl Ty {
    pub(crate) fn from_usize(index: usize) -> Self {
        Ty(index as u32)
    }

    pub fn index(self) -> usize {
        self.0 as usize
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum TyVar {
    Any(u32),
    Int(u32),
    Float(u32),
}

#[derive(Clone, PartialEq, Eq, Hash, Debug)]
pub enum TyKind {
    Var(TyVar),
    Primitive(PrimTy),
    Adt {
        def: DefId,
        args: Vec<Ty>,
    },
    Generic(HirId),
    SelfTy(DefId),
    Ref {
        base: Ty,
        mutability: Mutability,
    },
    Any(Ty),
    Unit,
    Tuple(Vec<Ty>),
    Array {
        elem: Ty,
        len: Option<HirId>, // -> Node::Expr, the constant expression `N`
    },
    Fun {
        params: Vec<Ty>,
        ret: Option<Ty>,
    },
    Dyn {
        trait_: DefId,
        args: Vec<Ty>,
    },
    Never,
    Error,
}
