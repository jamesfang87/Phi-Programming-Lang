//! The driver: everything between the command line and the compiler's passes.
//!
//! [`cli`] turns arguments and the project's `Phi.toml` into a command and a
//! [`Config`](cli::Config), then dispatches to [`project`] for scaffolding or [`pipeline`]
//! for compilation. [`source`] holds the source text every pass reads, and [`emit_debug`]
//! prints the passes' intermediate results.

pub mod cli;
pub mod emit_debug;
pub mod pipeline;
pub mod project;
pub mod source;
