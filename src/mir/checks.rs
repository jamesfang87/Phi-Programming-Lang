pub mod lattice;

mod always_return;
pub(crate) mod borrowck;
pub(crate) mod never_read;

use crate::mir::lower::Mir;
use crate::session::Session;
use crate::typeck::tyctx::TyCtx;

pub fn run_checks(session: &Session, tcx: &mut TyCtx, mir: &Mir) {
    always_return::check(session, mir);
    borrowck::check(session, tcx, mir);
    never_read::check(session, mir);
}
