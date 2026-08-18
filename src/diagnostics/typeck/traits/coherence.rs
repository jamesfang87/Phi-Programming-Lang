use crate::ast::Symbol;
use crate::ast::interner::Interner;
use crate::diagnostics::typeck::display::DisplayCx;
use crate::diagnostics::typeck::traits::get_name_of_trait;
use crate::diagnostics::{DiagCtx, Diagnostic};
use crate::driver::source::SrcSpan;
use crate::hir::Hir;
use crate::typeck::traits::overlap::ExtendHeader;

/// Where the block a header describes was written. A header names the block rather than carrying
/// its span, since the two say the same thing and only the diagnostics here ask for the span.
fn block_span(hir: &Hir, header: &ExtendHeader) -> SrcSpan {
    hir.extend(header.def).span
}

pub fn report_conflicting_extends(
    hir: &Hir,
    cx: DisplayCx<'_>,
    first: &ExtendHeader,
    second: &ExtendHeader,
) {
    let trait_ref = second
        .trait_
        .as_ref()
        .expect("only two extend blocks are ever compared for a duplicate implementation");

    DiagCtx::emit(
        Diagnostic::error(
            format!(
                "conflicting implementations of trait `{}` for type `{}`",
                get_name_of_trait(hir, trait_ref.def),
                cx.show(second.self_ty)
            ),
            block_span(hir, second),
        )
        .with_label("conflicting implementation")
        .with_secondary(
            block_span(hir, first),
            format!("`{}` is already implemented here", cx.show(first.self_ty)),
        )
        .with_help(
            "two implementations may not both apply to one type; note that bounds on an \
                 implementation's own generics are not considered when deciding whether two of \
                 them overlap",
        ),
    );
}

pub fn report_duplicate_method(
    hir: &Hir,
    cx: DisplayCx<'_>,
    name: Symbol,
    first: &ExtendHeader,
    second: &ExtendHeader,
) {
    DiagCtx::emit(
        Diagnostic::error(
            format!(
                "the method `{}` is defined more than once for type `{}`",
                Interner::resolve(name),
                cx.show(second.self_ty)
            ),
            block_span(hir, second),
        )
        .with_label(format!(
            "duplicate definition of `{}`",
            Interner::resolve(name)
        ))
        .with_secondary(
            block_span(hir, first),
            format!(
                "`{}` already gets a method named `{}` here",
                cx.show(first.self_ty),
                Interner::resolve(name)
            ),
        )
        .with_help(
            "a call to it would have no single meaning, so one of the two has to be renamed \
                 or removed",
        ),
    );
}
