use crate::ast::Mutability;
use crate::driver::source::SrcSpan;
use crate::hir::ids::HirId;
use crate::hir::path::Path;

#[derive(Debug)]
pub struct Ty {
    pub hir_id: HirId,
    pub kind: TyKind,
    pub span: SrcSpan,
}

#[derive(Debug)]
pub enum TyKind {
    Path {
        path: Path,
        args: Vec<HirId>, // -> Node::Ty
    },
    SelfTy(Vec<HirId>), // -> Node::Ty
    Ref {
        base: HirId, // -> Node::Ty
        mutability: Mutability,
    },
    Any(HirId),        // -> Node::Ty
    Iso(HirId),        // -> Node::Ty
    Tuple(Vec<HirId>), // -> Node::Ty
    Array {
        elem: HirId,        // -> Node::Ty
        len: Option<HirId>, // -> Node::Expr, the constant expression `N`
    },
    Function {
        params: Vec<HirId>, // -> Node::Ty
        ret: Option<HirId>, // -> Node::Ty
    },
    Dyn {
        path: Path,
        args: Vec<HirId>, // -> Node::Ty
    },
    Error,
}
