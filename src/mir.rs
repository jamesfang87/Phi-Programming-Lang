#![allow(dead_code)]

mod body;
pub mod checks;
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

pub use body::{BasicBlockData, Body, LocalDecl, Predecessors};
pub use ids::{BasicBlock, Local, VariantIdx};
pub use instance::{AnyMode, Instance};
pub use operand::{ConstKind, Constant, Operand};
pub use place::{Place, Projection};
pub use rvalue::{AggregateKind, CastKind, Rvalue};
pub use statement::{Statement, StatementKind};
pub use terminator::{AssertMessage, SwitchTargets, Terminator, TerminatorKind};
