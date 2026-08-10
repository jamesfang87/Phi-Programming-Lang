//! HIR representation of expressions.
//!
//! [`ExprKind`] stays close to the AST's `ExprKind`. See `src/hir.rs` for how the two differ:
//!
//! - Children are addressed by [`LocalId`] instead of being boxed inline.
//! - Lowering converts `while`, `for`, and `while let` loops into [`ExprKind::Loop`], and
//!   converts `if let` into [`ExprKind::Match`].

#![allow(dead_code)]

use crate::ast::{BinaryOp, Ident, Literal, Mutability, UnaryOp};
use crate::driver::source::SrcSpan;
use crate::hir::ids::{DefId, HirId};
use crate::hir::path::Path;

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
        operand: HirId, // -> Node::Expr
    },
    Binary {
        op: BinaryOp,
        lhs: HirId, // -> Node::Expr
        rhs: HirId, // -> Node::Expr
    },
    Assign {
        lhs: HirId, // -> Node::Expr
        rhs: HirId, // -> Node::Expr
    },
    /// `lhs += rhs`, `lhs -= rhs`, and so on. `op` is the underlying binary operator (`+`, `-`,
    /// ...).
    AssignOp {
        op: BinaryOp,
        lhs: HirId, // -> Node::Expr
        rhs: HirId, // -> Node::Expr
    },
    Borrow {
        mutability: Mutability,
        operand: HirId, // -> Node::Expr
    },
    Call {
        callee: HirId,    // -> Node::Expr
        args: Vec<HirId>, // -> Node::Expr
    },
    /// The `.` operator reaches a field, a method call, or an enum variant named through its
    /// type. See [`ast::ExprKind::Access`](crate::ast::ExprKind::Access). The three can't be
    /// distinguished without types, so they share this one node until typeck resolves which one
    /// it is. [`AccessArgs::Record`] is the exception. Name resolution settles that case on its
    /// own.
    Access {
        base: HirId, // -> Node::Expr
        member: Ident,
        args: AccessArgs,
    },
    Index {
        base: HirId,  // -> Node::Expr
        index: HirId, // -> Node::Expr
    },
    /// A struct literal. `path` is `None` for the elided `.{ ... }` form, whose type is
    /// recovered from the expected type during typeck.
    Ctor {
        path: Option<Path>,
        payload: Vec<PayloadField>,
    },
    /// An enum variant being built, such as `.circle(1.24)`, `.square { l: 4.0 }`, or `.none`.
    /// Typeck recovers which enum the variant belongs to from the expected type, so this node
    /// only names the variant and leaves it unresolved.
    Variant {
        variant: Ident,
        payload: Payload, // -> Node::Expr
    },
    Tuple(Vec<HirId>), // -> Node::Expr
    Range {
        lo: Option<HirId>, // -> Node::Expr
        hi: Option<HirId>, // -> Node::Expr
        inclusive: bool,
    },
    /// `expr?`. Propagates an error result out of the enclosing function.
    Try(HirId), // -> Node::Expr
    /// Both branches are blocks. An `else if` chain lowers to `else { if .. }`, so a chain of
    /// any length is uniform rather than alternating between an `If` and a `Block`.
    If {
        cond: HirId,               // -> Node::Expr
        then_block: HirId,         // -> Node::Block
        else_block: Option<HirId>, // -> Node::Block
    },
    Match {
        scrutinee: HirId, // -> Node::Expr
        arms: Vec<HirId>, // -> Node::Arm
    },
    /// A loop. `source` records whether it came from `while`, `for`, or a bare `loop`, since all
    /// three converge to this one node during lowering.
    Loop {
        source: LoopSource,
        block: HirId, // -> Node::Block
    },
    /// `spawn { ... }`. Runs the block as a new concurrent task.
    Spawn(HirId), // -> Node::Block
    /// `concurrent { ... }`. Runs the statements in the block concurrently with each other.
    Concurrent(HirId), // -> Node::Block
    Block(HirId), // -> Node::Block
    /// A closure literal. `DefId` names the closure's own owner, which holds its params, block,
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
    Single(HirId),
    Record(Vec<PayloadField>),
}

/// What follows the member name in an [`ExprKind::Access`]. See
/// [`ast::AccessArgs`](crate::ast::AccessArgs).
#[derive(Debug)]
pub enum AccessArgs {
    None,
    Call(Vec<HirId>), // -> Node::Expr
    Record(Vec<PayloadField>),
}

/// One named field and the node bound to it: a field initializer in an [`ExprKind::Ctor`]
/// struct literal, or one field of a record payload in [`Payload::Record`] or
/// [`AccessArgs::Record`].
///
/// Like [`Payload`] itself, this is shared between building and matching, so `value` names a
/// `Node::Expr` in an expression and a `Node::Pat` in a pattern.
///
/// Lowering desugars the `{ l }` field shorthand into `{ l: l }`, so `value` is always a real
/// node here -- unlike the AST's [`ast::PayloadField`](crate::ast::PayloadField), whose value is
/// optional.
#[derive(Debug)]
pub struct PayloadField {
    pub name: Ident,
    pub value: HirId,
}

#[derive(Debug)]
pub enum LoopSource {
    While,
    For,
    Loop,
}
