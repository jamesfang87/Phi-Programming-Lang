use crate::diagnostics::typeck::display::DisplayCx;
use crate::diagnostics::typeck::traits::show_goal;
use crate::diagnostics::{DiagCtx, Diagnostic};
use crate::hir::Hir;
use crate::typeck::traits::bounds::Obligation;

pub fn report_unsatisfied_bound(hir: &Hir, cx: DisplayCx<'_>, obligation: &Obligation) {
    DiagCtx::emit(
        Diagnostic::error(
            format!(
                "the trait bound {} is not satisfied",
                show_goal(hir, cx, &obligation.query)
            ),
            obligation.cause,
        )
        .with_label("this instantiation does not meet the bound its declaration writes")
        .with_secondary(obligation.declared_at, "required by this bound")
        .with_help(
            "either write an `extend .. with` block implementing the trait for this type, or \
             pass a type that already has one",
        ),
    );
}

pub fn report_annotations_needed(hir: &Hir, cx: DisplayCx<'_>, obligation: &Obligation) {
    DiagCtx::emit(
        Diagnostic::error(
            format!(
                "type annotations needed: cannot tell whether {} holds",
                show_goal(hir, cx, &obligation.query)
            ),
            obligation.cause,
        )
        .with_label("the type here is still unknown")
        .with_secondary(
            obligation.declared_at,
            "the bound that has to be decided is here",
        )
        .with_help(
            "nothing in this body pins the type down, so whether it satisfies the bound \
             cannot be decided; write the type out",
        ),
    );
}
