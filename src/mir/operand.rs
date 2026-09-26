use crate::ast::Symbol;
use crate::hir::DefId;
use crate::mir::instance::AnyMode;
use crate::mir::place::Place;
use crate::typeck::ty::Ty;

#[derive(Clone, Debug)]
pub enum Operand {
    Copy(Place),

    Move(Place),

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

#[derive(Clone, Debug)]
pub struct FunRef {
    pub def: DefId,
    pub args: Vec<Ty>,
    pub any_mode: Option<AnyMode>,
    pub self_ty: Option<Ty>,

    pub trait_method: Option<(DefId, u32)>,
}
