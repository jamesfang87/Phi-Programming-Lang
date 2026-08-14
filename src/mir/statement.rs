//! This module defines [`Statement`], one entry of a basic block's straight-line body, and
//! [`StatementKind`], the ways it can act.

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
    /// `StorageLive` marks `local`'s storage as live, allocating its slot. This variant is
    /// renamed from the original sketch's `LifetimeBegin`, since it has nothing to do with a
    /// projection's lifetime (the language elides those at the surface entirely) and everything
    /// to do with a stack slot's storage, exactly matching `rustc`'s own `StorageLive`.
    StorageLive(Local),
    /// `StorageDead` marks `local`'s storage as dead, the counterpart to `StorageLive` above.
    StorageDead(Local),
    Assign(Place, Rvalue),
    SetDiscriminant {
        place: Place,
        variant: VariantIdx,
    },
    /// `PlaceMention` evaluates `place` for its side effects, such as a bounds check on an index,
    /// without using the value it holds. Match lowering produces it for a scrutinee, and
    /// `with`-lend lowering produces it for a lend binding that is itself never read.
    PlaceMention(Place),
}
