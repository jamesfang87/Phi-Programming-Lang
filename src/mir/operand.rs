//! This module defines [`Operand`], the leaf level that a binary operator, a call, or an
//! aggregate reads its arguments from, and [`Constant`], the compile-time-known value one kind of
//! operand carries directly.

use crate::ast::Symbol;
use crate::hir::DefId;
use crate::mir::instance::AnyMode;
use crate::mir::place::Place;
use crate::typeck::ty::Ty;

/// `Operand` is a value simple enough to appear directly as an argument to a binary operator, a
/// call, or an aggregate, without needing a temporary of its own.
#[derive(Clone, Debug)]
pub enum Operand {
    /// This variant reads a trivially copyable place, and may occur any number of times for the
    /// same place.
    Copy(Place),
    /// This variant reads a place by consuming it. Lowering emits it at the one syntactic point
    /// that takes a non-trivially-copyable place by value: ownership transfer into a function
    /// argument, the initializer of a new binding, a `return`, and so on.
    Move(Place),
    /// This variant is a value known outright at compile time, embedded directly into the
    /// instruction rather than read out of a local's storage.
    Constant(Constant),
}

#[derive(Clone, Debug)]
pub struct Constant {
    pub ty: Ty,
    pub kind: ConstKind,
}

#[derive(Clone, Debug)]
pub enum ConstKind {
    /// `Int` stores the constant's mathematical value, not a raw bit pattern. For example, the
    /// literal `-5` is stored as the `i128` value `-5`, rather than as `i64`'s two's-complement
    /// encoding of it reinterpreted through a narrower field. `i128` is wide enough to hold every
    /// value of every integer type Phi defines, from `i64::MIN` through `u64::MAX`, without those
    /// two ranges colliding the way they would in a same-width unsigned field. `Constant::ty`
    /// alone says which of Phi's integer types the value is.
    Int(i128),
    Float(f64),
    Bool(bool),
    Char(char),
    /// `Str` holds interned string data, for a string literal.
    Str(Symbol),
    /// `FnDef` represents a unit value, or a reference to a specific function or associated
    /// function, named by its `DefId`, the type arguments its generics were instantiated with,
    /// and, for a definition whose return type is `any T`, the projection mode that call was
    /// resolved to. A generic definition's `DefId` alone does not pick out one `Body`. For
    /// example, `largest::<Vector2D>` and `largest::<i32>` share a `DefId` and differ only in
    /// this list. An `any`-returning definition's `DefId` alone does not pick out one `Body`
    /// either, for the same reason with `AnyMode` in place of a type argument: see
    /// [`crate::mir::Instance`].
    FnDef(DefId, Vec<Ty>, Option<AnyMode>),
}
