use crate::hir::DefId;
use crate::typeck::ty::Ty;

pub mod bounds;
pub mod coherence;
pub mod index;
pub mod members;
pub mod method;
pub mod overlap;
pub mod solve;
pub mod validity;

#[cfg(test)]
mod necessity;

/// A trait together with the generic arguments it was applied to, as declared in an extend
/// block's header: for `extend Foo with Show<i32>`, `def` is `Show` and `args` is `[i32]`.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct TraitRef {
    /// The trait this refers to.
    pub def: DefId,

    /// The trait's own generic arguments, in declared order.
    pub args: Vec<Ty>,
}
