#![allow(dead_code)]

mod adt;
mod body;
pub mod checks;
mod def_names;
mod drop_elaboration;
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
mod vtables;

pub use adt::AdtDef;
pub use body::{BasicBlockData, Body, LocalDecl};
pub use ids::{BasicBlock, Local, StatementId, VariantIdx};
pub use instance::{AnyMode, Instance};
pub use lower::Mir;
pub use operand::{ConstKind, Constant, Operand};
pub use place::{Place, Projection};
pub use rvalue::{AggregateKind, CastKind, Rvalue};
pub use statement::{Statement, StatementKind};
pub use terminator::{AssertMessage, SwitchTargets, Terminator, TerminatorKind};
