use crate::driver::source::SrcSpan;
use crate::mir::ids::{Local, VariantIdx};
use crate::mir::place::Place;
use crate::mir::rvalue::Rvalue;

#[derive(Clone, Debug)]
pub struct Statement {
    pub kind: StatementKind,
    pub span: SrcSpan,
}

#[derive(Clone, Debug)]
pub enum StatementKind {
    StorageLive(Local),
    StorageDead(Local),
    Assign(Place, Rvalue),
    SetDiscriminant { place: Place, variant: VariantIdx },
    PlaceMention(Place),
    CheckMutable(Place),
}
