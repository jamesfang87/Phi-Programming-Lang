pub mod bounds;
pub mod coherence;
pub mod index;
pub mod members;
pub mod method;
pub mod solve;
pub mod validity;

use crate::ast::interner::Interner;
use crate::diagnostics::typeck::display::DisplayCx;
use crate::hir::{DefId, Hir};
use crate::typeck::traits::solve::Query;

pub fn get_name_of_trait(hir: &Hir, def: DefId) -> &'static str {
    Interner::resolve(hir.trait_(def).name.text)
}

/// A goal as the user would write it: `` `Foo: Show` ``.
pub fn show_goal(hir: &Hir, cx: DisplayCx<'_>, goal: &Query) -> String {
    format!(
        "`{}: {}`",
        cx.show(goal.self_ty),
        get_name_of_trait(hir, goal.trait_.def)
    )
}
