#![allow(dead_code)]

use crate::ast::{BinaryOp, Ident, Literal, Mutability, UnaryOp};
use crate::driver::source::SrcSpan;
use crate::hir::ids::{ArmId, BlockId, DefId, ExprId, HirId, TyId};
use crate::hir::path::Path;

#[derive(Debug)]
pub struct Expr {
    pub hir_id: ExprId,
    pub kind: ExprKind,
    pub span: SrcSpan,
}

#[derive(Clone, Debug)]
pub enum ExprKind {
    Literal(Literal),
    Path(Path),
    Unary {
        op: UnaryOp,
        operand: ExprId,
    },
    Binary {
        op: BinaryOp,
        lhs: ExprId,
        rhs: ExprId,
    },
    Assign {
        lhs: ExprId,
        rhs: ExprId,
    },
    /// `lhs += rhs`, `lhs -= rhs`, and so on. `op` is the underlying binary operator (`+`, `-`,
    /// ...).
    AssignOp {
        op: BinaryOp,
        lhs: ExprId,
        rhs: ExprId,
    },
    Borrow {
        mutability: Mutability,
        operand: ExprId,
    },
    Call {
        callee: ExprId,
        args: Vec<ExprId>,
    },
    /// The `.` operator reaches a field, a method call, or an enum variant named through its
    /// enum. `base` naming a type rather than a value is what marks the last of the three; see
    /// [`crate::hir::Hir::names_a_type`].
    Access {
        base: ExprId,
        member: Ident,
        args: AccessArgs,
    },
    Index {
        base: ExprId,
        index: ExprId,
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
        payload: Payload,
    },
    Tuple(Vec<ExprId>),
    /// `expr?`. Propagates an error result out of the enclosing function.
    Try(ExprId),
    /// Both branches are blocks. An `else if` chain lowers to `else { if .. }`, so a chain of
    /// any length is uniform rather than alternating between an `If` and a `Block`.
    If {
        cond: ExprId,
        then_block: BlockId,
        else_block: Option<BlockId>,
    },
    Match {
        scrutinee: ExprId,
        arms: Vec<ArmId>,
    },
    /// A loop. `source` records whether it came from `while`, `for`, or a bare `loop`, since all
    /// three converge to this one node during lowering.
    Loop {
        source: LoopSource,
        block: BlockId,
    },
    /// `spawn { ... }`. Runs the block as a new concurrent task.
    Spawn(BlockId),
    /// `concurrent { ... }`. Runs the statements in the block concurrently with each other.
    Concurrent(BlockId),
    Block(BlockId),
    /// A closure literal. `DefId` names the closure's own owner, which holds its params, block,
    /// and return type; see [`crate::hir::Closure`].
    Closure(DefId),
    /// `expr as ty`. See [`crate::typeck::cast`] for which primitive-to-primitive conversions
    /// this is allowed to mean.
    Cast {
        expr: ExprId,
        ty: TyId,
    },
    /// `new <expr>`. See [`crate::ast::ExprKind::New`].
    New(ExprId),
    /// `new [<elem>; <count>]`. See [`crate::ast::ExprKind::NewArray`].
    NewArray {
        elem: ExprId,
        count: ExprId,
    },
    Assert {
        cond: ExprId,
        msg: Option<ExprId>,
    },
    Panic {
        msg: Option<ExprId>,
    },
    Unreachable {
        msg: Option<ExprId>,
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
    Call(Vec<ExprId>),
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
}
