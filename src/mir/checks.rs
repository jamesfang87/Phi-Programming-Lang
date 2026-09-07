pub mod lattice;

mod always_return;
pub(crate) mod borrowck;
pub(crate) mod never_read;

use crate::mir::lower::Mir;

pub fn run_checks(mir: &Mir) {
    always_return::check(mir);
    borrowck::check(mir);
    never_read::check(mir);
}
