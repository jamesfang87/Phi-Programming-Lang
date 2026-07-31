//! HIR representation of blocks and statements: `{ ... }`, `let`, `with`, `break`, `continue`,
//! `return`, and `defer`.
//!
//! Surface forms that desugar to a loop or a match during lowering (`while`, `for`, `while let`,
//! `if let`) don't appear here. They show up as `ExprKind::Loop` or `ExprKind::Match` instead;
//! see the module-level docs in `src/hir.rs`.

use crate::ast::Mutability;
use crate::hir::ids::{HirId, LocalId};
use crate::lexer::src_span::SrcSpan;

/// A `{ ... }` block. It holds a sequence of statements, optionally followed by a trailing,
/// non-semicolon expression that becomes the block's value.
#[derive(Debug)]
pub struct Block {
    pub hir_id: HirId,
    pub stmts: Vec<LocalId>,   // -> Node::Stmt
    pub expr: Option<LocalId>, // -> Node::Expr
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
    Let(LetStmt),
    /// `with a = lend x, b = lend y { ... }`. Each [`WithLend`] borrows a value for the
    /// duration of `body` and releases it once `body` finishes.
    With {
        lends: Vec<WithLend>,
        body: LocalId, // -> Node::Block
    },
    Break,
    Continue,
    Return(Option<LocalId>), // -> Node::Expr
    /// `defer expr;`. Schedules `expr` to run when the enclosing scope exits, no matter how it
    /// exits.
    Defer(LocalId), // -> Node::Expr
    Expr(LocalId),           // -> Node::Expr
    /// A statement that failed to parse. Lowering carries it through rather than aborting.
    Error,
}

/// A `let` binding, of the form `let [mut] pat[: ty] = init;`.
#[derive(Debug)]
pub struct LetStmt {
    pub mutability: Mutability,
    pub pat: LocalId,                 // -> Node::Pat
    pub ty: Option<LocalId>,          // -> Node::Ty
    pub init: LocalId,                // -> Node::Expr
    pub else_branch: Option<LocalId>, // -> Node::Block
}

/// One binding inside a `with` statement's lend list, such as `a = lend x` in
/// `with a = lend x { ... }`.
#[derive(Debug)]
pub struct WithLend {
    pub pat: LocalId,        // -> Node::Pat
    pub ty: Option<LocalId>, // -> Node::Ty
    pub init: LocalId,       // -> Node::Expr
    pub span: SrcSpan,
}
