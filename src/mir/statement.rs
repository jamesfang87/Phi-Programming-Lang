use crate::driver::source::SrcSpan;
use crate::mir::ids::{Local, StatementId, VariantIdx};
use crate::mir::place::Place;
use crate::mir::rvalue::Rvalue;

#[derive(Clone, Debug)]
pub struct Statement {
    pub id: StatementId,
    pub kind: StatementKind,
    pub span: SrcSpan,
}

#[derive(Clone, Debug)]
pub enum StatementKind {
    StorageLive(Local),
    StorageDead(Local),
    Assign(Place, Rvalue),
    SetDiscriminant {
        place: Place,
        variant: VariantIdx,
    },
    PlaceMention(Place),
    /// Marks `local` as bound by a `with` lend rather than a `let`. This carries no runtime
    /// effect; `mir::checks::borrowck::lifetimes` reads it to tell a `with`-lend borrow, whose
    /// lifetime always runs to its `StorageDead`, from an ordinary borrow, whose lifetime is NLL.
    WithLend(Local),
}
