pub mod lattice;

mod always_return;
pub(crate) mod borrowck;
pub(crate) mod constck;
mod never_read;

use crate::mir::lower::Mir;

pub fn run_checks(mir: &Mir) {
    constck::check(mir);
    always_return::check(mir);
    borrowck::check(mir);
}
