use crate::hir::Hir;
use crate::mir::lower::Mir;
use crate::mir::{Local, Place, Projection};
use crate::session::Session;
use crate::typeck::ty::ctx::TyCtx;

pub(crate) mod captures;
pub(crate) mod definite_init;
pub(crate) mod element_moves;
pub(crate) mod exclusivity;
pub(crate) mod lifetimes;
pub(crate) mod returned_reference;

#[derive(Clone, Hash, PartialEq, Eq, Debug)]
pub struct Register {
    pub owner: Local,
    pub subregister: Vec<SubRegister>,
}

#[derive(Clone, Hash, PartialEq, Eq, Debug)]
pub enum SubRegister {
    Deref,
    Field(u32),
    ConstantIndex(u32),
}

pub(crate) fn to_register(place: &Place) -> Register {
    let mut subregister = Vec::with_capacity(place.projections.len());
    for projection in &place.projections {
        match *projection {
            Projection::Deref => subregister.push(SubRegister::Deref),
            Projection::Field(n) => subregister.push(SubRegister::Field(n)),
            Projection::ConstantIndex(offset) => {
                subregister.push(SubRegister::ConstantIndex(offset))
            }
            Projection::Downcast(_) | Projection::Index(_) => {}
        }
    }
    Register {
        owner: place.local,
        subregister,
    }
}

pub fn check(session: &Session, hir: &Hir, tcx: &mut TyCtx, mir: &Mir) {
    definite_init::check(session, tcx, mir);
    let computed = lifetimes::compute_lifetimes_map(mir);
    exclusivity::check_with_lifetimes(session, mir, &computed);
    returned_reference::check(session, tcx, mir, &computed);
    element_moves::check(session, tcx, mir);
    captures::check(session, hir, tcx, mir);
}
