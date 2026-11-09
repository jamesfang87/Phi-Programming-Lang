use crate::hir::DefId;
use crate::typeck::ty::Ty;

pub mod check;
pub mod collect;
pub mod solve;

#[cfg(test)]
mod tests;

/// Represents a trait with type parameters: Trait<T, U, ...>
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct TraitRef {
    /// The trait this refers to.
    pub def: DefId,

    /// The trait's type arguments
    pub args: Vec<Ty>,
}
