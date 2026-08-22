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
    /// `CheckedBinaryOp` behaves like `BinaryOp`, but only for integer `+`, `-`, and `*`, and it
    /// produces a `(T, bool)` tuple: the wrapped result paired with a flag reporting whether the
    /// operation overflowed. Lowering only emits this in debug builds. The `bool` feeds an
    /// `Assert` immediately afterward, and only the tuple's first field is ever read as the
    /// arithmetic result. A release build emits plain `BinaryOp` instead, which wraps with no
    /// check at all.
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
    /// `Discriminant` reads a place's enum discriminant as an integer, feeding a `SwitchInt`
    /// terminator.
    Discriminant(Place),
    /// `Len` reads the runtime length of an array or a slice-typed place.
    Len(Place),
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum CastKind {
    /// This variant is a user-written `expr as Ty`, governed by `typeck::cast::cast_allowed`.
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
    Closure { def: DefId },
}
