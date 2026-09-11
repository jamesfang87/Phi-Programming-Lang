pub mod lattice;

mod always_return;
pub(crate) mod borrowck;
pub(crate) mod never_read;

use crate::mir::lower::Mir;
use crate::typeck::tyctx::TyCtx;

pub fn run_checks(tcx: &mut TyCtx, mir: &Mir) {
    always_return::check(mir);
    borrowck::check(tcx, mir);
    never_read::check(mir);
}
