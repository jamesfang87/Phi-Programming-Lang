//! HIR representation of blocks and statements: `{ ... }`, `let`, `with`, `break`, `continue`,
//! `return`, and `defer`.
//!
//! Surface forms that desugar to a loop or a match during lowering (`while`, `for`, `while let`,
//! `if let`) don't appear here. They show up as `ExprKind::Loop` or `ExprKind::Match` instead;
//! see the module-level docs in `src/hir.rs`.

use crate::ast::Mutability;
use crate::driver::source::SrcSpan;
use crate::hir::ids::HirId;

/// A `{ ... }` block. It holds a sequence of statements, optionally followed by a trailing,
/// non-semicolon expression that becomes the block's value.
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
    /// A `let` binding, of the form `let [mut] pat[: ty] = init;`.
    Let {
        mutability: Mutability,
        pat: HirId,                // -> Node::Pat
        ty: Option<HirId>,         // -> Node::Ty
        init: HirId,               // -> Node::Expr
        else_block: Option<HirId>, // -> Node::Block
    },
    /// `with a = lend x, b = lend y { ... }`. Each [`WithLend`] borrows a value for the
    /// duration of `block` and releases it once `block` finishes.
    With {
        lends: Vec<WithLend>,
        block: HirId, // -> Node::Block
    },
    Break,
    Continue,
    Return(Option<HirId>), // -> Node::Expr
    /// `defer expr;`. Schedules `expr` to run when the enclosing scope exits, no matter how it
    /// exits.
    Defer(HirId), // -> Node::Expr
    Expr(HirId),           // -> Node::Expr
    /// A statement that failed to parse. Lowering carries it through rather than aborting.
    Error,
}

/// One binding inside a `with` statement's lend list, such as `a = lend x` in
/// `with a = lend x { ... }`.
#[derive(Clone, Debug)]
pub struct WithLend {
    pub pat: HirId,        // -> Node::Pat
    pub ty: Option<HirId>, // -> Node::Ty
    pub init: HirId,       // -> Node::Expr
    pub span: SrcSpan,
}
