use crate::mir::Local;
use crate::mir::lower::Mir;

pub(crate) mod definite_init;
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
    Field(u32),
    ConstantIndex { offset: u32, from_end: bool },
}

pub fn check(mir: &Mir) {
    definite_init::check(mir);
    exclusivity::check(mir);
}
