use crate::hir::DefId;
use crate::typeck::ty::Ty;

#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum AnyMode {
    Owned,
    Ref,
    RefMut,
}

// TODO: this is unclear as to what instance refers to
// is this a struct or a function or both
#[derive(Clone, PartialEq, Eq, Hash, Debug)]
pub struct Instance {
    pub def: DefId,
    pub any_mode: Option<AnyMode>,
    pub args: Vec<Ty>,
    pub self_ty: Option<Ty>,
}
