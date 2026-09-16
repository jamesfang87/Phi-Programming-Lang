use crate::hir::Hir;
use crate::typeck::Typeck;

use super::session::session;

/// How far through trait solving a [`Typeck`] test wants to run before inspecting it.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Debug)]
pub enum TypeckStage {
    Collect,
    Index,
    Coherence,
    Members,
}

/// A type checker run through `stage`, in the order the pipeline runs those passes.
pub fn checker_through(hir: &Hir, stage: TypeckStage) -> Typeck<'_> {
    let mut checker = Typeck::new(session(), hir);
    checker.collect_module(hir.root_id());
    if stage >= TypeckStage::Index {
        checker.collect_traits();
    }
    if stage >= TypeckStage::Coherence {
        checker.check_coherence();
    }
    if stage >= TypeckStage::Members {
        checker.check_trait_members();
    }
    checker
}
