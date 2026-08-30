use crate::diagnostics::typeck::display::DisplayCx;
use crate::diagnostics::{DiagCtx, Diagnostic};
use crate::driver::source::SrcSpan;
use crate::typeck::ty::Ty;

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
