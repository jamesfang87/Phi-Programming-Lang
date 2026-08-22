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
    /// This variant reads a place by consuming it.
    Move(Place),
    /// This variant is a value known at compile time, which is embedded directly into the
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
    Int(i128),
    Float(f64),
    Bool(bool),
    Char(char),
    Str(Symbol),
    FunDef(DefId, Vec<Ty>, Option<AnyMode>),
}
