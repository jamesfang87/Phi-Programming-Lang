//! The Mid-level Intermediate Representation (MIR) is produced by Lowering #2 from a fully
//! type-checked HIR body, where every expression's type is known, every method call is resolved
//! to a concrete definition or a trait obligation, and every pattern's binding modes are settled.
//! MIR restates that body as an explicit control-flow graph over an explicit vocabulary of memory
//! locations, rather than as a tree of expressions.
//!
//! A tree conflates control flow with data flow. A graph of basic blocks, each a straight-line
//! sequence of statements ending in one explicit transfer of control, keeps the two separate, so
//! that borrow checking, drop insertion, and the optimizer can each reason about one without
//! reconstructing the other.
//!
//! [`Body`] is the MIR of one definition. Its `local_decls`, addressed by [`Local`], name every
//! memory location the body's code reads or writes, and its `basic_blocks`, addressed by
//! [`BasicBlock`], hold the control-flow graph itself. [`Place`] and [`PlaceElem`] address a
//! location down to one field or one array element, rather than only a whole local. [`Operand`]
//! and [`Rvalue`] are what a HIR expression tree flattens into, since every sub-expression that
//! is not already a bare read of a place or a literal gets its own temporary local and its own
//! `Assign` statement, so that no statement ever nests an arbitrary expression inside another.
//! [`StatementKind`] is what a basic block's straight-line body is built from, and
//! [`TerminatorKind`] is what its one closing transfer of control is built from.
//!
//! This module holds the MIR's data types. Building a `Body` out of a HIR body is Lowering #2,
//! implemented in [`lower`]; substituting a generic `Body` into the concrete instances a program
//! actually needs is [`monomorphize`]; naming each of those instances with a unique linker-safe
//! symbol is [`mangle`].
//!
//! `--emit-debug`'s MIR dump is this module's only consumer today, and it renders every field
//! through `Debug` rather than matching on it, so a variant or a field lowering constructs (a
//! `Drop` terminator, an `Aggregate::Array`, a `StatementKind::SetDiscriminant`, and so on -- none
//! of the constructs Lowering #2 already handles are unreachable, only not yet *read back*) looks
//! unused to the compiler's dead-code analysis until a borrow checker, an optimizer, or a
//! codegen backend exists to pattern-match it. The same reasoning `ast.rs`'s own blanket allow
//! rests on.
#![allow(dead_code)]

mod body;
pub mod constck;
mod ids;
mod instance;
pub mod lower;
pub mod mangle;
pub mod monomorphize;
mod operand;
mod place;
mod rvalue;
mod statement;
mod terminator;

pub use body::{BasicBlockData, Body, LocalDecl};
pub use ids::{BasicBlock, Local, VariantIdx};
pub use instance::{AnyMode, Instance};
pub use operand::{ConstKind, Constant, Operand};
pub use place::{Place, PlaceElem};
pub use rvalue::{AggregateKind, CastKind, Rvalue};
pub use statement::{Statement, StatementKind};
pub use terminator::{AssertMessage, SwitchTargets, Terminator, TerminatorKind};
