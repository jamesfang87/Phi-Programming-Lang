pub mod res;
mod resolver;
pub mod results;
pub mod symbol_table;
#[cfg(test)]
mod tests;

pub use res::{Local, PrimTy, Res, TyDef, Type};
pub use resolver::resolve;
pub use results::NameResolutions;
