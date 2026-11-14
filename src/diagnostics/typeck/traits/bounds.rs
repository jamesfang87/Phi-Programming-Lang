use crate::diagnostics::Diagnostic;
use crate::diagnostics::codes;
use crate::diagnostics::display::DisplayCtx;
use crate::diagnostics::typeck::traits::show_goal;
use crate::hir::Hir;
use crate::typeck::traits::solve::Obligation;

pub fn report_unsatisfied_bound(hir: &Hir, cx: DisplayCtx<'_>, obligation: &Obligation) {
    cx.emit(
        Diagnostic::error(
            format!(
                "the trait bound {} is not satisfied",
                show_goal(hir, cx, &obligation.query)
            ),
            obligation.cause,
        )
        .with_code(codes::UNSATISFIED_BOUND)
        .with_label("this instantiation does not meet the bound its declaration writes")
        .with_secondary(obligation.declared_at, "required by this bound")
        .with_help("consider implementing the trait for this type using an `extend .. with` block"),
    );
}

pub fn report_annotations_needed(hir: &Hir, cx: DisplayCtx<'_>, obligation: &Obligation) {
    cx.emit(
        Diagnostic::error(
            format!(
                "type annotations needed: cannot tell whether {} holds",
                show_goal(hir, cx, &obligation.query)
            ),
            obligation.cause,
        )
        .with_code(codes::BOUNDS_ANNOTATIONS_NEEDED)
        .with_label("the type here is still unknown")
        .with_secondary(
            obligation.declared_at,
            "the bound that has to be decided is here",
        )
        .with_help("consider adding type annotations here"),
    );
}
