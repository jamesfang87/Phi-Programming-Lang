use crate::ast::Mutability;
use crate::driver::source::SrcSpan;
use crate::hir::ids::HirId;

#[derive(Debug)]
pub struct Block {
    pub hir_id: HirId,
    pub stmts: Vec<HirId>,   // -> Node::Stmt
    pub expr: Option<HirId>, // -> Node::Expr
    pub span: SrcSpan,
}

#[derive(Debug)]
pub struct Stmt {
    pub hir_id: HirId,
    pub kind: StmtKind,
    pub span: SrcSpan,
}

#[derive(Debug)]
pub enum StmtKind {
    Let {
        mutability: Mutability,
        pat: HirId,                // -> Node::Pat
        ty: Option<HirId>,         // -> Node::Ty
        init: HirId,               // -> Node::Expr
        else_block: Option<HirId>, // -> Node::Block
    },
    With {
        lends: Vec<WithLend>,
        block: HirId, // -> Node::Block
    },
    Break,
    Continue,
    Return(Option<HirId>), // -> Node::Expr
    Defer(HirId),          // -> Node::Expr
    Expr(HirId),           // -> Node::Expr
    /// A statement that failed to parse. Lowering carries it through rather than aborting.
    Error,
}

#[derive(Clone, Debug)]
pub struct WithLend {
    pub pat: HirId,        // -> Node::Pat
    pub ty: Option<HirId>, // -> Node::Ty
    pub init: HirId,       // -> Node::Expr
    pub span: SrcSpan,
}
