//! Diagnostic construction for every compiler stage.
//!
//! Each submodule mirrors the stage it reports for (`langitems`, `parser`, `nameres`, `typeck`)
//! and holds only `report_*` functions: given the raw facts a caller already worked out (a span,
//! a name, a type), each one decides how to phrase the [`Diagnostic`](crate::diag::Diagnostic)
//! and calls [`DiagCtx::emit`](crate::diag::DiagCtx::emit). The logic that decides *whether*
//! something is wrong stays in the stage's own module; this is only ever the *how to say it*
//! half.

pub mod langitems;
pub mod nameres;
pub mod parser;
pub mod typeck;
