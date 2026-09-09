pub mod lattice;

mod always_return;
mod entry_point;
pub(crate) mod borrowck;
pub(crate) mod never_read;

use crate::hir::Hir;
use crate::mir::lower::Mir;
use crate::typeck::tyctx::TyCtx;

pub fn run_checks(hir: &Hir, tcx: &mut TyCtx, mir: &Mir) {
    always_return::check(mir);
    borrowck::check(hir, tcx, mir);
    never_read::check(mir);
    entry_point::check(hir);
}
