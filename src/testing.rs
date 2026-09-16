//! Shared support for the compiler's tests, split by concern: the shared thread-local
//! [`session`], the stage-by-stage [`pipeline`] drivers, fixture [`fixtures`], diagnostic
//! [`assert`]ions, type-check [`checks`], and HIR [`lookup`] helpers.

mod assert;
mod checks;
mod fixtures;
mod lookup;
mod pipeline;
mod session;

pub use assert::*;
pub use checks::*;
pub use fixtures::*;
pub use lookup::*;
pub use pipeline::*;
pub use session::*;
