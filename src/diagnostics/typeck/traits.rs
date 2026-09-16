pub mod bounds;
pub mod coherence;
pub mod index;
pub mod members;
pub mod method;
pub mod solve;
pub mod validity;

use crate::diagnostics::display::DisplayCtx;
use crate::hir::{DefId, Hir};
use crate::session::Session;
use crate::typeck::traits::solve::Goal;

pub fn get_name_of_trait(session: &Session, hir: &Hir, def: DefId) -> &'static str {
    session.resolve(hir.trait_(def).name.text)
}

pub fn show_goal(hir: &Hir, cx: DisplayCtx<'_>, goal: &Goal) -> String {
    let args = match &goal.trait_.args[..] {
        [] => String::new(),
        args => format!(
            "<{}>",
            args.iter()
                .map(|arg| cx.show(*arg).to_string())
                .collect::<Vec<_>>()
                .join(", ")
        ),
    };
    format!(
        "`{}: {}{args}`",
        cx.show(goal.self_ty),
        get_name_of_trait(cx.session(), hir, goal.trait_.def)
    )
}
