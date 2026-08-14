//! This module defines [`Terminator`], the one explicit transfer of control every basic block
//! ends in, and [`TerminatorKind`], the shapes that transfer can take.

use crate::ast::BinaryOp;
use crate::driver::source::SrcSpan;
use crate::mir::ids::BasicBlock;
use crate::mir::operand::Operand;
use crate::mir::place::Place;

#[derive(Clone, Debug)]
pub struct Terminator {
    pub kind: TerminatorKind,
    pub span: SrcSpan,
}

#[derive(Clone, Debug)]
pub enum TerminatorKind {
    Goto {
        target: BasicBlock,
    },
    Return,
    SwitchInt {
        discr: Operand,
        targets: SwitchTargets,
    },
    Call {
        func: Operand,
        args: Vec<Operand>,
        destination: Place,
        /// This is `None` for a call whose return type is `Never`. For example, the runtime
        /// panic function never returns control to its caller, so a call to it has no
        /// continuation block to target.
        target: Option<BasicBlock>,
    },
    Drop {
        place: Place,
        target: BasicBlock,
    },
    Assert {
        cond: Operand,
        expected: bool,
        msg: AssertMessage,
        target: BasicBlock,
    },
    Unreachable,
}

#[derive(Clone, Debug)]
pub struct SwitchTargets {
    pub values: Vec<(u128, BasicBlock)>,
    pub otherwise: BasicBlock,
}

/// `AssertMessage` is the panic that an `Assert` terminator's failure edge reports. Lowering
/// inserts one variant per check on its own, never in response to a user-written assertion, since
/// the language has no surface syntax for one: a `CheckedBinaryOp`'s overflow flag, the zero
/// check ahead of integer division or remainder, and the bounds check ahead of a
/// `PlaceElem::Index` projection. This type is not itself part of the original planning sketch.
/// It fills in what `Assert`'s `msg` field needs to hold to cover the overflow, division-by-zero,
/// and bounds checks that lowering inserts.
#[derive(Clone, Debug)]
pub enum AssertMessage {
    /// This variant reports that `op` overflowed on its two operands, and pairs with the
    /// `CheckedBinaryOp` computing the same operation immediately before this `Assert`.
    Overflow(BinaryOp, Operand, Operand),
    DivisionByZero(Operand),
    RemainderByZero(Operand),
    /// This variant reports that `index` was not less than `len`, ahead of a
    /// `PlaceElem::Index` projection using it.
    BoundsCheck {
        len: Operand,
        index: Operand,
    },
}
