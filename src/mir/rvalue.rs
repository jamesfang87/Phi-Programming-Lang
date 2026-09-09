use crate::ast::{BinaryOp, Mutability, UnaryOp};
use crate::hir::DefId;
use crate::mir::ids::VariantIdx;
use crate::mir::operand::Operand;
use crate::mir::place::Place;
use crate::typeck::ty::Ty;

#[derive(Clone, Debug)]
pub enum Rvalue {
    Use(Operand),
    /// `Ref` represents `&place` or `&mut place`.
    Ref {
        mutability: Mutability,
        place: Place,
    },
    BinaryOp(BinaryOp, Operand, Operand),
    /// `CheckedBinaryOp` behaves like `BinaryOp`, but is only used for integer `+`, `-`, and `*`.
    /// Its purpose is for checking whether the operation overflowed. It produces a `(T, bool)` tuple
    /// where the first element is the result of the op and the second is whether an overflow
    /// occurred Lowering only emits this in debug builds.
    CheckedBinaryOp(BinaryOp, Operand, Operand),
    UnaryOp(UnaryOp, Operand),
    /// `kind` distinguishes a user-written `as` from a compiler-inserted coercion. See
    /// [`CastKind::ReifyFnPointer`] for the one coercion this represents.
    Cast {
        operand: Operand,
        ty: Ty,
        kind: CastKind,
    },
    Aggregate(Box<AggregateKind>, Vec<Operand>),
    /// `Discriminant` reads a place's enum discriminant as an integer
    Discriminant(Place),
    /// `Len` reads the length of an array or a slice-typed place.
    Len(Place),
    /// Allocates storage for the operand's type, moves the operand into it, and produces an
    /// `iso T` naming that storage. Allocation and initialization are one step: an `iso` local
    /// is never observably allocated-but-uninitialized.
    New(Operand),
    /// Allocates storage for `count` elements and initializes every one of them to `elem`,
    /// producing `iso [T]`. `count` is a `usize` operand, so the length is not required to be a
    /// constant.
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
            | Rvalue::New(operand) => vec![operand],
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
    /// This variant is a user-written `expr as Ty`
    Primitive,
    /// This variant is compiler-inserted. It materializes an
    /// actual function-pointer value from a zero-sized `ConstKind::FunDef` operand
    ReifyFunPointer,
}

#[derive(Clone, Debug)]
pub enum AggregateKind {
    Tuple,
    Array,
    Adt { def: DefId, variant: VariantIdx },
    Closure { def: DefId, args: Vec<Ty> },
}
