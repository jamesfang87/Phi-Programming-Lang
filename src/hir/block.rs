use crate::ast::Mutability;
use crate::driver::source::SrcSpan;
use crate::hir::ids::{BlockId, ExprId, PatId, StmtId, TyId};

#[derive(Debug)]
pub struct Block {
    pub hir_id: BlockId,
    pub stmts: Vec<StmtId>,
    pub expr: Option<ExprId>,
    pub span: SrcSpan,
}

#[derive(Debug)]
pub struct Stmt {
    pub hir_id: StmtId,
    pub kind: StmtKind,
    pub span: SrcSpan,
}

#[derive(Debug)]
pub enum StmtKind {
    Let {
        mutability: Mutability,
        pat: PatId,
        ty: Option<TyId>,
        init: ExprId,
        else_block: Option<BlockId>,
    },
    With {
        lends: Vec<WithLend>,
        block: BlockId,
    },
    // TODO: `Break`/`Continue` carry no label and no value (mirrors `ast::StmtKind`), so
    // lowering cannot target an outer loop or yield a value from a loop.
    Break,
    Continue,
    Return(Option<ExprId>),
    Defer(ExprId),
    Expr(ExprId),
    /// A statement that failed to parse. Lowering carries it through rather than aborting.
    Error,
}

#[derive(Clone, Debug)]
pub struct WithLend {
    pub pat: PatId,
    pub ty: Option<TyId>,
    pub init: ExprId,
    pub span: SrcSpan,
}
