use crate::ast::Mutability;
use crate::driver::source::SrcSpan;
use crate::hir::ids::{ExprId, TyId};
use crate::hir::path::Path;

#[derive(Debug)]
pub struct Ty {
    pub hir_id: TyId,
    pub kind: TyKind,
    pub span: SrcSpan,
}

#[derive(Debug)]
pub enum TyKind {
    Path {
        path: Path,
        args: Vec<TyId>,
    },
    SelfTy(Vec<TyId>),
    Ref {
        base: TyId,
        mutability: Mutability,
    },
    Any(TyId),
    Iso(TyId),
    Tuple(Vec<TyId>),
    Array {
        elem: TyId,
        len: Option<ExprId>,
    },
    Function {
        params: Vec<TyId>,
        ret: Option<TyId>,
    },
    Dyn {
        path: Path,
        args: Vec<TyId>,
    },
    Error,
}
