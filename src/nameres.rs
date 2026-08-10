//! Name resolution.
//!
//! Runs on the `Ast`, before HIR lowering: it produces a side table keyed by `NodeId` that
//! `crate::hir::lower` consumes to build every `hir::Path` with its answer already attached
//! (see `crate::hir::path`). This used to be one of two resolvers -- an HIR-based one ran
//! alongside it purely so `typeck` had something to read -- but that second pass, and the
//! `NameResolutions` table it produced, are gone: `typeck` now reads every resolution straight
//! off the `hir::Path` that carries it.
//!
//! See `docs/superpowers/specs/2026-08-07-ast-symbol-table-design.md`.

pub mod diagnostics;
pub mod res;
mod resolver;
pub mod results;
pub mod symbol_table;
#[cfg(test)]
mod tests;

pub use res::{Local, PrimTy, Res, TyDef, Type};
pub use resolver::resolve;
pub use results::NameResolutions;
