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

    /// Stores `value` into `place` and yields the value the place held.
    Replace {
        place: Place,
        value: Operand,
    },

    /// Exchanges the values two places hold and yields unit.
    Swap {
        left: Place,
        right: Place,
    },

    /// Calls one of the C library functions.
    LibcCall {
        function: LibcFunction,
        args: Vec<Operand>,
    },

    /// Yields the size, in bytes, of the named type.
    SizeOf(Ty),

    /// Yields the alignment, in bytes, of the named type.
    AlignOf(Ty),

    /// Writes the UTF-8 bytes of a character into a buffer and yields how many bytes it wrote.
    EncodeUtf8 {
        value: Operand,
        buffer: Operand,
    },

    /// Compares two `str` values by length and bytes.
    StrEq {
        left: Operand,
        right: Operand,
    },

    /// Stores `value` into uninitialized storage, dropping nothing.
    WriteUninit {
        place: Place,
        value: Operand,
    },

    /// Moves the value out of initialized storage, leaving the slot uninitialized.
    ReadUninit(Place),

    /// Borrows the value in initialized storage.
    AssumeInitRef(Place),
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
            Rvalue::EncodeUtf8 { value, buffer } => vec![value, buffer],
            Rvalue::StrEq { left, right } => vec![left, right],
            Rvalue::Replace { value, .. } => vec![value],
            Rvalue::LibcCall { args, .. } => args.iter().collect(),
            Rvalue::SizeOf(_) | Rvalue::AlignOf(_) => Vec::new(),
            Rvalue::WriteUninit { value, .. } => vec![value],
            Rvalue::Ref { .. }
            | Rvalue::Discriminant(_)
            | Rvalue::Len(_)
            | Rvalue::Swap { .. }
            | Rvalue::ReadUninit(_)
            | Rvalue::AssumeInitRef(_) => Vec::new(),
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum LibcFunction {
    Malloc,
    Free,
    Realloc,
    Memcpy,
    Memset,
    Memcmp,
    Read,
    Write,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum CastKind {
    Primitive,

    ReifyFunPointer,

    /// `&[T; N]` widened to `&[T]`, which keeps the data pointer and adds `N` as the length.
    UnsizeSlice,
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
