pub mod bounds;
pub mod coherence;
pub mod index;
pub mod members;
pub mod method;
pub mod solve;

use crate::ast::interner::Interner;
use crate::hir::{DefId, Hir};

pub fn get_name_of_trait(hir: &Hir, def: DefId) -> &'static str {
    Interner::resolve(hir.trait_(def).name.text)
}
