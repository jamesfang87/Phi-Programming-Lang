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

/// Instance represents an monomorphized instance of a definition.
/// `any_mode` is `None` for a definition whose signature mentions no `any`
/// `self_ty` is `Some` only for a trait's own method, whose one body is
/// monomorphized once per implementing type
#[derive(Clone, PartialEq, Eq, Hash, Debug)]
pub struct Instance {
    pub def: DefId,
    pub any_mode: Option<AnyMode>,
    pub args: Vec<Ty>,
    pub self_ty: Option<Ty>,
}
