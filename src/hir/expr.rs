//! HIR representation of expressions.
//!
//! [`ExprKind`] stays close to the AST's `ExprKind`. See `src/hir.rs` for how the two differ:
//!
//! - Children are addressed by [`LocalId`] instead of being boxed inline.
//! - Lowering converts `while`, `for`, and `while let` loops into [`ExprKind::Loop`], and
//!   converts `if let` into [`ExprKind::Match`].

#![allow(dead_code)]

use crate::ast::{BinaryOp, Ident, Literal, Mutability, Path, UnaryOp};
use crate::hir::ids::{DefId, HirId, LocalId};
use crate::lexer::src_span::SrcSpan;

#[derive(Debug)]
pub struct Expr {
    pub hir_id: HirId,
    pub kind: ExprKind,
    pub span: SrcSpan,
}

#[derive(Debug)]
pub enum ExprKind {
    Literal(Literal),
    Path(Path),
    Unary {
        op: UnaryOp,
        operand: LocalId, // -> Node::Expr
    },
    Binary {
        op: BinaryOp,
        lhs: LocalId, // -> Node::Expr
        rhs: LocalId, // -> Node::Expr
    },
    Assign {
        lhs: LocalId, // -> Node::Expr
        rhs: LocalId, // -> Node::Expr
    },
    /// `lhs += rhs`, `lhs -= rhs`, and so on. `op` is the underlying binary operator (`+`, `-`,
    /// ...).
    AssignOp {
        op: BinaryOp,
        lhs: LocalId, // -> Node::Expr
        rhs: LocalId, // -> Node::Expr
    },
    Borrow {
        mutability: Mutability,
        operand: LocalId, // -> Node::Expr
    },
    Call {
        callee: LocalId,    // -> Node::Expr
        args: Vec<LocalId>, // -> Node::Expr
    },
    /// The `.` operator reaches a field, a method call, or an enum variant named through its
    /// type. See [`ast::ExprKind::Access`](crate::ast::ExprKind::Access). The three can't be
    /// distinguished without types, so they share this one node until typeck resolves which one
    /// it is. [`AccessArgs::Record`] is the exception. Name resolution settles that case on its
    /// own.
    Access {
        base: LocalId, // -> Node::Expr
        member: Ident,
        args: AccessArgs,
    },
    Index {
        base: LocalId,  // -> Node::Expr
        index: LocalId, // -> Node::Expr
    },
    /// A struct literal. `path` is `None` for the elided `.{ ... }` form, whose type is
    /// recovered from the expected type during typeck.
    Ctor {
        path: Option<Path>,
        payload: Vec<(Ident, LocalId)>, // -> Node::Expr
    },
    /// An enum variant being built, such as `.circle(1.24)`, `.square { l: 4.0 }`, or `.none`.
    /// Typeck recovers which enum the variant belongs to from the expected type, so this node
    /// only names the variant and leaves it unresolved.
    Variant {
        variant: Ident,
        payload: Payload, // -> Node::Expr
    },
    Tuple(Vec<LocalId>), // -> Node::Expr
    Range {
        lo: Option<LocalId>, // -> Node::Expr
        hi: Option<LocalId>, // -> Node::Expr
        inclusive: bool,
    },
    /// `expr?`. Propagates an error result out of the enclosing function.
    Try(LocalId), // -> Node::Expr
    If {
        cond: LocalId,                // -> Node::Expr
        then_branch: LocalId,         // -> Node::Block
        else_branch: Option<LocalId>, // -> Node::Expr
    },
    Match {
        scrutinee: LocalId, // -> Node::Expr
        arms: Vec<LocalId>, // -> Node::Arm
    },
    /// A loop. `source` records whether it came from `while`, `for`, or a bare `loop`, since all
    /// three converge to this one node during lowering.
    Loop {
        source: LoopSource,
        body: LocalId, // -> Node::Block
    },
    /// `spawn { ... }`. Runs `body` as a new concurrent task.
    Spawn(LocalId), // -> Node::Block
    /// `concurrent { ... }`. Runs the statements in `body` concurrently with each other.
    Concurrent(LocalId), // -> Node::Block
    Block(LocalId), // -> Node::Block
    /// A closure literal. `DefId` names the closure's own owner, which holds its params, body,
    /// and return type; see [`crate::hir::Closure`].
    Closure(DefId),
    Error,
}

/// An enum variant's payload, shared by [`ExprKind::Variant`] and
/// [`PatKind::Variant`](crate::hir::PatKind::Variant). The `LocalId`s name `Node::Expr`s in the
/// first and `Node::Pat`s in the second, exactly as [`ExprKind::Tuple`] and
/// [`PatKind::Tuple`](crate::hir::PatKind::Tuple) already do.
///
/// A payload is always one value. A tuple payload is a single tuple, not several arguments. The
/// one exception is [`Payload::Record`], which is a payload declared inline as an anonymous
/// struct and so carries the variant's own field names. Lowering desugars away the `{ l }` field
/// shorthand, so a record payload's fields always have both a name and a node here.
#[derive(Debug)]
pub enum Payload {
    None,
    Single(LocalId),
    Record(Vec<(Ident, LocalId)>),
}

/// What follows the member name in an [`ExprKind::Access`]. See
/// [`ast::AccessArgs`](crate::ast::AccessArgs).
#[derive(Debug)]
pub enum AccessArgs {
    None,
    Call(Vec<LocalId>),            // -> Node::Expr
    Record(Vec<(Ident, LocalId)>), // -> Node::Expr
}

#[derive(Debug)]
pub enum LoopSource {
    While,
    For,
    Loop,
}
