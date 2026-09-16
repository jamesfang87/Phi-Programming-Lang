pub mod lattice;

mod always_return;
pub(crate) mod borrowck;
pub(crate) mod never_read;

use crate::hir::Hir;
use crate::mir::lower::Mir;
use crate::session::Session;
use crate::typeck::ty::ctx::TyCtx;

pub fn run_checks(session: &Session, hir: &Hir, tcx: &mut TyCtx, mir: &Mir) {
    always_return::check(session, mir);
    borrowck::check(session, hir, tcx, mir);
    never_read::check(session, mir);
}
