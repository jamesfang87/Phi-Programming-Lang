use crate::diagnostics::typeck::display::DisplayCx;
use crate::diagnostics::{DiagCtx, Diagnostic};
use crate::driver::source::SrcSpan;
use crate::typeck::ty::Ty;

/// An operator applied to a type that does not implement the operator's trait.
///
/// The only place a failed query is reported without an obligation behind it: an operator raises
/// its goal and reads the answer on the spot, because the result type feeds the expression around
/// it and there is no later moment to retry (see
/// [`implements_operator`](crate::typeck::Typeck::implements_operator)).
pub fn report_operator_trait_missing(
    cx: DisplayCx<'_>,
    self_ty: Ty,
    trait_name: &str,
    span: SrcSpan,
) {
    DiagCtx::emit(
        Diagnostic::error(
            format!("`{}` does not implement `{trait_name}`", cx.show(self_ty)),
            span,
        )
        .with_label(format!(
            "this operator needs an `extend .. with {trait_name}` block providing it"
        )),
    );
}
