use crate::diagnostics::typeck::lower_ty::{
    report_reference_generic_arg, report_unexpected_generic_args, report_unsized_dyn,
};
use crate::diagnostics::typeck::report_any_outside_signature;
use crate::driver::source::SrcSpan;
use crate::hir::TyId;
use crate::session::Session;
use crate::typeck::Typeck;
use crate::typeck::ty::Ty;

impl<'hir> Typeck<'hir> {
    /// Returns whether none of `args` mentions a reference, reporting the first that does.
    pub(crate) fn check_no_reference_args(&mut self, hir_args: &[TyId], args: &[Ty]) -> bool {
        for (&hir_id, &arg) in hir_args.iter().zip(args) {
            if self.tcx.contains_ref(arg) {
                let span = self.hir.ty(hir_id).span;
                report_reference_generic_arg(self.display_cx(), arg, span);
                return false;
            }
        }
        true
    }

    /// Returns whether none of `args` mentions `any`, reporting the first that does.
    pub(crate) fn check_no_any_args(&mut self, hir_args: &[TyId], args: &[Ty]) -> bool {
        for (&hir_id, &arg) in hir_args.iter().zip(args) {
            if self.tcx.contains_any(arg) {
                let span = self.hir.ty(hir_id).span;
                report_any_outside_signature(self.display_cx(), arg, span);
                return false;
            }
        }
        true
    }

    /// Returns whether none of `args` mentions a bare `dyn`, reporting the first that does.
    pub(crate) fn check_no_dyn_args(&mut self, hir_args: &[TyId], args: &[Ty]) -> bool {
        for (&hir_id, &arg) in hir_args.iter().zip(args) {
            if self.tcx.contains_bare_dyn(arg) {
                let span = self.hir.ty(hir_id).span;
                report_unsized_dyn(self.display_cx(), arg, span);
                return false;
            }
        }
        true
    }

    pub(crate) fn check_no_dyn(&mut self, ty: Ty, span: SrcSpan) {
        if self.tcx.contains_bare_dyn(ty) {
            report_unsized_dyn(self.display_cx(), ty, span);
        }
    }

    pub(crate) fn check_no_args(session: &Session, args: &[TyId], span: SrcSpan, kind: &str) {
        if !args.is_empty() {
            report_unexpected_generic_args(session, kind, span);
        }
    }
}
