use crate::ast::Symbol;
use crate::hir::DefId;
use crate::mir::instance::AnyMode;
use crate::mir::place::Place;
use crate::typeck::ty::Ty;

/// `Operand` is an argument to a binary operator, a
/// call, or an aggregate, without needing a temporary of its own.
#[derive(Clone, Debug)]
pub enum Operand {
    /// This variant reads a trivially copyable place, and may occur any number of times for the
    /// same place.
    Copy(Place),
    /// This variant reads a place by consuming it.
    Move(Place),
    /// This variant is a value known at compile time, which is embedded directly into the
    /// instruction
    Constant(Constant),
}

#[derive(Clone, Debug)]
pub struct Constant {
    pub ty: Ty,
    pub kind: ConstKind,
}

#[derive(Clone, Debug)]
pub enum ConstKind {
    Int(i128),
    Float(f64),
    Bool(bool),
    Char(char),
    Str(Symbol),
    FunDef(FunRef),
}

/// A reference to a definition used as a value or a call target.
#[derive(Clone, Debug)]
pub struct FunRef {
    pub def: DefId,
    pub args: Vec<Ty>,
    pub any_mode: Option<AnyMode>,
    pub self_ty: Option<Ty>,
    /// The trait and vtable slot `def` belongs to, when it is a method a trait declares.
    pub trait_method: Option<(DefId, u32)>,
}
