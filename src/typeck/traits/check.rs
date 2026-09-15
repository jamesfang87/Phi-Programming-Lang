//! Checking that the program's traits are well formed: that implementations do not conflict,
//! that each implementation matches its trait, that every bound names a trait, and that every
//! header's argument count fits.
//!
//! These rules are program-level: they read declarations, not bodies. The per-body trait
//! questions live in [`crate::typeck::traits::solve`].

mod coherence;
mod members;
mod overlap;
mod validity;

use crate::typeck::Typeck;

impl<'hir> Typeck<'hir> {
    /// Checks every trait-related rule that has to hold across the whole program.
    pub fn check_traits(&mut self) {
        self.check_coherence();
        self.check_trait_members();
        self.check_all_bounds_are_traits();
        self.check_extend_headers_arity();
    }
}
