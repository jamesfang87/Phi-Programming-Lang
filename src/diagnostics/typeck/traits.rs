//! Diagnostics for `typeck::traits`, one file per module there.

pub mod bounds;
pub mod coherence;
pub mod index;
pub mod members;
pub mod method;
pub mod solve;

use crate::ast::interner::Interner;
use crate::hir::{DefId, Hir, OwnerNode};

/// The name a trait was declared with, for a diagnostic that has to name one. Shared by every
/// submodule here rather than reimplemented per-file -- they all mean the same
/// `OwnerNode::Trait(t) => Interner::resolve(t.name.text)` lookup.
pub fn trait_name(hir: &Hir, def: DefId) -> &'static str {
    let OwnerNode::Trait(trait_) = hir.def(def) else {
        unreachable!("a TraitRef's def always names a trait; the index is what enforces it");
    };
    Interner::resolve(trait_.name.text)
}
