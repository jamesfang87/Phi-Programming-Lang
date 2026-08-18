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
    /// `CheckMutable` asserts that `place` may be written to directly at this point in the
    /// program: that walking its projection down to a bare local, stopping the moment a `Deref`
    /// is crossed (what a reference points at is reachable regardless of the reference-holding
    /// local's own mutability), reaches a local other than an unadorned `let`'s. Lowering emits
    /// one immediately alongside each surface form that writes through a place directly -- a
    /// plain assignment, a compound assignment, and an explicit `&mut` borrow -- rather than
    /// through an intervening reference, which the check does not restrict. It asserts nothing by
    /// itself; [`crate::mir::constck`] is the pass that walks every `CheckMutable` a `Body`
    /// contains and reports the ones that fail.
    CheckMutable(Place),
}
