use crate::diag::{DiagCtx, Diagnostic};
use crate::diagnostics::typeck::display::DisplayCx;
use crate::diagnostics::typeck::traits::trait_name;
use crate::driver::source::SrcSpan;
use crate::hir::Hir;
use crate::typeck::traits::solve::{Obligation, RECURSION_LIMIT};
use crate::typeck::ty::Ty;

/// How a goal reads in a diagnostic: `` `Foo<i32>: Show` ``. Shared with
/// [`bounds`](crate::typeck::traits::bounds), which raises the same kind of goal from a
/// declaration's bound rather than from the solver itself.
pub fn show_goal(hir: &Hir, cx: DisplayCx<'_>, goal: &Obligation) -> String {
    format!(
        "`{}: {}`",
        cx.show(goal.self_ty),
        trait_name(hir, goal.trait_ref.def)
    )
}

pub fn report_require_extends_fails(
    cx: DisplayCx<'_>,
    self_ty: Ty,
    trait_name: &str,
    because: &str,
    span: SrcSpan,
) {
    DiagCtx::emit(
        Diagnostic::error(
            format!("`{}` does not implement `{trait_name}`", cx.show(self_ty)),
            span,
        )
        .with_label(format!(
            "{because} needs an `extend .. with {trait_name}` block providing it"
        )),
    );
}

pub fn report_cyclic_bound(hir: &Hir, cx: DisplayCx<'_>, goal: &Obligation) {
    let mut diag = Diagnostic::error(
        format!(
            "cyclic trait bound: proving {} requires proving it again",
            show_goal(hir, cx, goal)
        ),
        goal.cause,
    )
    .with_label("this bound cannot be proved")
    .with_help(
        "one of the bounds involved has to be discharged by something other than \
             itself, or the chain has no base case",
    );

    if let Some(declared_at) = goal.declared_at {
        diag = diag.with_secondary(declared_at, "the bound the cycle starts from is here");
    }

    DiagCtx::emit(diag);
}

pub fn report_recursion_limit(hir: &Hir, cx: DisplayCx<'_>, goal: &Obligation) {
    DiagCtx::emit(
        Diagnostic::error(
            format!(
                "recursion limit reached while proving {}",
                show_goal(hir, cx, goal)
            ),
            goal.cause,
        )
        .with_label("this bound needed too many steps to prove")
        .with_help(format!(
            "each step of the proof produced a larger type than the last, and the solver \
                 gave up after {RECURSION_LIMIT}"
        )),
    );
}

/// The release-mode half of the duplicate-match assertion in
/// [`Typeck::select`](crate::typeck::Typeck::select): reported rather than resolved by arbitrary
/// choice, so that a coherence bug surfaces as a diagnostic instead of as an answer that depends
/// on collection order.
pub fn report_ambiguous_impls(hir: &Hir, cx: DisplayCx<'_>, goal: &Obligation) {
    DiagCtx::emit(
        Diagnostic::error(
            format!(
                "more than one implementation applies to {}",
                show_goal(hir, cx, goal)
            ),
            goal.cause,
        )
        .with_label("cannot tell which implementation to use")
        .with_help(
            "this is a compiler bug: overlapping implementations should have been reported at \
             their declarations",
        ),
    );
}
