use crate::hir::Hir;
use crate::mir::lower::Mir;
use crate::mir::{Local, Place, Projection};
use crate::typeck::tyctx::TyCtx;

pub(crate) mod captures;
pub(crate) mod definite_init;
pub(crate) mod element_moves;
pub(crate) mod exclusivity;
pub(crate) mod lifetimes;

/// `Register` is a simplification of [`Place`](crate::mir::Place) for borrowcking. It
/// only contains projections (field access and constant index) for the purposes of narrowing
/// down the memory location.
#[derive(Clone, Hash, PartialEq, Eq, Debug)]
pub struct Register {
    pub owner: Local,
    pub subregister: Vec<SubRegisters>,
}

#[derive(Clone, Hash, PartialEq, Eq, Debug)]
pub enum SubRegisters {
    Deref,
    Field(u32),
    ConstantIndex(u32),
}

pub(crate) fn register_of(place: &Place) -> Register {
    let mut subregister = Vec::with_capacity(place.projections.len());
    for projection in &place.projections {
        match *projection {
            Projection::Deref => subregister.push(SubRegisters::Deref),
            Projection::Field(n) => subregister.push(SubRegisters::Field(n)),
            Projection::ConstantIndex(offset) => {
                subregister.push(SubRegisters::ConstantIndex(offset))
            }
            Projection::Downcast(_) | Projection::Index(_) => {}
        }
    }
    Register {
        owner: place.local,
        subregister,
    }
}

pub(crate) fn element_of_array(place: &Place) -> bool {
    place.projections.iter().any(|projection| {
        matches!(
            projection,
            Projection::Index(_) | Projection::ConstantIndex(_)
        )
    })
}

pub fn check(hir: &Hir, tcx: &mut TyCtx, mir: &Mir) {
    definite_init::check(mir);
    exclusivity::check(mir);
    element_moves::check(tcx, mir);
    captures::check(hir, tcx, mir);
}
