use crate::hir::DefId;
use crate::typeck::ty::Ty;

/// Which of a value's three representations an `any`-typed position was specialized to.
/// Every `any` occurrence in one definition's signature collapses to the same mode, chosen once per call site by how that call's own result
/// is used there
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum AnyMode {
    /// Every `any` position becomes its bare `T`, taken by value.
    Owned,
    /// Every `any` position becomes `&T`.
    Ref,
    /// Every `any` position becomes `&mut T`.
    RefMut,
}

/// One monomorphized instance of a definition.
/// `any_mode` is `None` for a definition whose signature mentions no `any`
#[derive(Clone, PartialEq, Eq, Hash, Debug)]
pub struct Instance {
    pub def: DefId,
    pub any_mode: Option<AnyMode>,
    pub args: Vec<Ty>,
}
