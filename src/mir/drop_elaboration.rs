//! Drop elaboration: the MIR-to-MIR pass that turns each owned local's implicit end-of-scope
//! drop into explicit `TerminatorKind::Drop`s.
//!
//! Runs after [`crate::mir::monomorphize`], on the concrete, monomorphized `Body` of every
//! `Instance` -- not on the generic bodies `mir::lower` produces -- since deciding whether a
//! local needs dropping requires a concrete `Ty` (see `codegen::drop::drop_glue`'s own match on
//! `TyKind::Iso`, which is what a `Drop` terminator ultimately compiles down to).
//!
//! For every local whose `StorageLive`/`StorageDead` pair `mir::lower` already emits, this pass
//! decides, at the point of that `StorageDead`, whether the local still owns a value that needs
//! dropping. That decision has two parts, each of which can make an unconditional
//! "one `Drop` right before the `StorageDead`" insertion wrong:
//!
//! - **Ownership can be conditional.** A local moved out (via `Operand::Move`) on one branch but
//!   not another has no single static answer at the point the branches rejoin -- MIR has no phi
//!   nodes to carry "was it moved" through a join. Where the set of predecessors feeding that join
//!   is finite and statically known (a plain `if`), this pass may resolve it by open-coding the
//!   drop separately into each predecessor instead of any runtime check at all. Where it isn't --
//!   chiefly a value conditionally moved inside a loop, whose ownership state must survive across
//!   the loop's own back edge -- this pass threads a synthesized `bool` drop flag through the
//!   local's lifetime instead: set `true` where it starts being owned, set `false` at each point
//!   it is moved out, and read at the point of doubt via a `SwitchInt` that replaces the
//!   unconditional `Drop` with one arm that drops and one that skips.
//! - **A composite type's need to drop can be conditional on its own runtime shape.** A struct
//!   that is not itself `iso`/`Drop` can still own fields that need dropping, and each such field
//!   is dropped individually (a separate `Drop` per field, `Place`-projected via
//!   `Projection::Field`) rather than the struct being handed to `drop_glue` as one unit. An enum
//!   is the same idea taken one step further: which fields need dropping depends on which variant
//!   is active at runtime, so this pass reads the value's own discriminant (`Rvalue::Discriminant`)
//!   through a `SwitchInt`, and only the active variant's arm drops its fields (`Place`-projected
//!   via `Projection::Downcast` then `Projection::Field`).
//!
//! In every case, the block containing the local's `StorageDead` is split as needed so each
//! `Drop` (or the `SwitchInt` guarding one) can end its own block as a terminator, resuming with
//! the original `StorageDead` and everything after it.
//!
//! `StorageDead` is not the only point a local's old value can need dropping, though: a genuine
//! reassignment (`p = new_value;` to an already, possibly conditionally, initialized `p` -- as
//! opposed to `p`'s own first, initializing assignment, which has no old value to drop) overwrites
//! whatever the place held with no `StorageDead` involved at all. Left alone, that old value's
//! resources become unreachable -- a leak, the mirror image of the double-free a missing
//! moved-out check at `StorageDead` would cause. So every genuine reassignment gets the same
//! treatment: a `Drop` of the place's old value spliced in immediately before it, decided by the
//! same "was it moved by now" analysis (unconditional, skipped, or flag-gated) as the
//! `StorageDead` case, just asked at the reassignment's own program point instead.

#[cfg(test)]
mod tests;

// Algo
// Register -> State {
//     Init, Uninit
// }
//
// This state is similar to how definite-init works right now
//
// Anyways, the meet function should give us whether a Register
// has potentially been moved. Based on the predecessors,
// if one predecessor has been moved, then we have been moved too.
// If so, then there should be a runtime flag.
//
// Note that reassigning may also result in a drop instr
// if we are reassigning to a value that implements Drop
