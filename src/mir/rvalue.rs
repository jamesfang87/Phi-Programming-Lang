use crate::ast::{BinaryOp, Mutability, UnaryOp};
use crate::hir::DefId;
use crate::mir::ids::VariantIdx;
use crate::mir::operand::Operand;
use crate::mir::place::Place;
use crate::typeck::ty::Ty;

#[derive(Clone, Debug)]
pub enum Rvalue {
    Use(Operand),

    Ref {
        mutability: Mutability,
        place: Place,
    },
    BinaryOp(BinaryOp, Operand, Operand),
    CheckedBinaryOp(BinaryOp, Operand, Operand),
    UnaryOp(UnaryOp, Operand),

    Cast {
        operand: Operand,
        ty: Ty,
        kind: CastKind,
    },
    Aggregate(Box<AggregateKind>, Vec<Operand>),

    Unsize {
        operand: Operand,
        trait_: DefId,
    },

    Discriminant(Place),

    Len(Place),

    New(Operand),

    NewArray {
        elem: Operand,
        count: Operand,
    },
}

impl Rvalue {
    pub fn operands(&self) -> Vec<&Operand> {
        match self {
            Rvalue::Use(operand)
            | Rvalue::UnaryOp(_, operand)
            | Rvalue::Cast { operand, .. }
            | Rvalue::New(operand)
            | Rvalue::Unsize { operand, .. } => vec![operand],
            Rvalue::BinaryOp(_, lhs, rhs)
            | Rvalue::CheckedBinaryOp(_, lhs, rhs)
            | Rvalue::NewArray {
                elem: lhs,
                count: rhs,
            } => vec![lhs, rhs],
            Rvalue::Aggregate(_, operands) => operands.iter().collect(),
            Rvalue::Ref { .. } | Rvalue::Discriminant(_) | Rvalue::Len(_) => Vec::new(),
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum CastKind {

    Primitive,

    ReifyFunPointer,
}

#[derive(Clone, Debug)]
pub enum AggregateKind {
    Tuple,
    Array,
    Adt {
        def: DefId,
        variant: VariantIdx,
    },
    Closure {
        def: DefId,
        args: Vec<Ty>,
        self_ty: Option<Ty>,
    },
}
