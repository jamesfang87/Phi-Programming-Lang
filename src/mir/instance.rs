//! `Instance` names one concrete, fully-lowered [`Body`](crate::mir::Body): a definition, the
//! `any`-mode it was specialized to (if any), and the generic arguments it was monomorphized
//! with. A bare `DefId` picks out at most a family of `Body`s for a generic or `any`-returning
//! definition, per the spec's "Generic monomorphization" and "`any` parameters and
//! projection-mode specialization" sections; `Instance` is the triple that picks out exactly one.

use crate::hir::DefId;
use crate::typeck::ty::Ty;

/// Which of a value's three representations an `any`-typed position was specialized to.
///
/// Every `any` occurrence in one definition's signature -- its parameters, `self`, and its
/// return -- collapses to the same mode, chosen once per call site by how that call's own result
/// is used there, never independently per parameter (`fun f(x: any T, y: any T) -> any T`
/// collapses to `fun f(x: &T, y: &T) -> &T` as a whole, for instance, not field by field). See
/// `mir::lower`'s `any`-mode documentation and section 7 of the README.
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
///
/// `any_mode` is `None` for a definition whose signature mentions no `any`, and `Some` for one
/// that does, independent of `args`: the two are separate specialization axes, decided at
/// different times (`any_mode` while lowering, since it changes a parameter's `Place`-building
/// structurally; `args` afterward, by the monomorphize pass, since it is pure `Ty` substitution).
/// `args` is empty for a non-generic definition.
#[derive(Clone, PartialEq, Eq, Hash, Debug)]
pub struct Instance {
    pub def: DefId,
    pub any_mode: Option<AnyMode>,
    pub args: Vec<Ty>,
}
