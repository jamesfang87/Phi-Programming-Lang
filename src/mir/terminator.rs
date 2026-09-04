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
    /// Switch statement
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

impl Terminator {
    pub fn successors(&self) -> impl Iterator<Item = BasicBlock> + '_ {
        self.kind.successors()
    }
}

impl TerminatorKind {
    pub fn successors(&self) -> impl Iterator<Item = BasicBlock> + '_ {
        let single = match self {
            TerminatorKind::Goto { target }
            | TerminatorKind::Drop { target, .. }
            | TerminatorKind::Assert { target, .. } => Some(*target),
            TerminatorKind::Call { target, .. } => *target,
            TerminatorKind::Return
            | TerminatorKind::Unreachable
            | TerminatorKind::SwitchInt { .. } => None,
        };
        let switch = match self {
            TerminatorKind::SwitchInt { targets, .. } => Some(targets.all_targets()),
            _ => None,
        };
        single.into_iter().chain(switch.into_iter().flatten())
    }
}

#[derive(Clone, Debug)]
pub struct SwitchTargets {
    pub values: Vec<(u128, BasicBlock)>,
    pub otherwise: BasicBlock,
}

impl SwitchTargets {
    pub fn all_targets(&self) -> impl Iterator<Item = BasicBlock> + '_ {
        self.values
            .iter()
            .map(|(_, target)| *target)
            .chain(std::iter::once(self.otherwise))
    }
}

#[derive(Clone, Debug)]
pub enum AssertMessage {
    Overflow(BinaryOp, Operand, Operand),
    DivisionByZero(Operand),
    RemainderByZero(Operand),
    BoundsCheck { len: Operand, index: Operand },
}
