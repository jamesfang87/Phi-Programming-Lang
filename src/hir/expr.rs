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

#[derive(Clone, Debug)]
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
    /// type.
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
    /// `expr as ty`. See [`crate::typeck::cast`] for which primitive-to-primitive conversions
    /// this is allowed to mean.
    Cast {
        expr: HirId, // -> Node::Expr
        ty: HirId,   // -> Node::Ty
    },
    Error,
}

#[derive(Clone, Debug)]
pub enum Payload {
    None,
    Single(HirId),
    Record(Vec<PayloadField>),
}

#[derive(Clone, Debug)]
pub enum AccessArgs {
    None,
    Call(Vec<HirId>), // -> Node::Expr
    Record(Vec<PayloadField>),
}

/// This is used for field initializers and definitions record payloads
#[derive(Clone, Debug)]
pub struct PayloadField {
    pub name: Ident,
    pub value: HirId,
}

#[derive(Clone, Copy, Debug)]
pub enum LoopSource {
    While,
    For,
    Loop,
}
