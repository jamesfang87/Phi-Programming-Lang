#![allow(dead_code)]

mod body;
pub mod checks;
pub mod drop_elaboration;
mod ids;
mod instance;
pub mod lower;
pub mod monomorphize;
mod operand;
mod place;
mod rvalue;
mod statement;
mod terminator;
mod vtables;

pub use body::{BasicBlockData, Body, BodyKind, LocalDecl};
pub use ids::{BasicBlock, Local, StatementId, VariantIdx};
pub use instance::{AnyMode, Instance};
pub use lower::Mir;
pub use operand::{ConstKind, Constant, FunRef, Operand};
pub use place::{Place, Projection, place_ty};
pub use rvalue::{AggregateKind, CastKind, Rvalue};
pub use statement::{Statement, StatementKind};
pub use terminator::{AssertMessage, SwitchTargets, Terminator, TerminatorKind};
