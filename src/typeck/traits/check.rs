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
