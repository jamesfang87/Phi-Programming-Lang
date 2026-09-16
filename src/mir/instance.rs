use crate::hir::DefId;
use crate::typeck::ty::Ty;

#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum AnyMode {
    /// Every `any` position becomes its bare `T`, taken by value.
    Owned,
    /// Every `any` position becomes `&T`.
    Ref,
    /// Every `any` position becomes `&mut T`.
    RefMut,
}

#[derive(Clone, PartialEq, Eq, Hash, Debug)]
pub struct Instance {
    pub def: DefId,
    pub any_mode: Option<AnyMode>,
    pub args: Vec<Ty>,
    pub self_ty: Option<Ty>,
}
