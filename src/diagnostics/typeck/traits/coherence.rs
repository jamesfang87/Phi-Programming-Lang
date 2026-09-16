use crate::ast::Symbol;
use crate::diagnostics::Diagnostic;
use crate::diagnostics::codes;
use crate::diagnostics::display::DisplayCtx;
use crate::diagnostics::typeck::traits::get_name_of_trait;
use crate::driver::source::SrcSpan;
use crate::hir::Hir;
use crate::typeck::traits::collect::ExtendHeader;

fn extend_span(hir: &Hir, header: &ExtendHeader) -> SrcSpan {
    hir.extend(header.def).span
}

pub fn report_conflicting_extends(
    hir: &Hir,
    cx: DisplayCtx<'_>,
    first: &ExtendHeader,
    second: &ExtendHeader,
) {
    let trait_ref = second
        .trait_
        .as_ref()
        .expect("only two extend blocks are ever compared for a duplicate implementation");

    cx.emit(
        Diagnostic::error(
            format!(
                "conflicting implementations of trait `{}` for type `{}`",
                get_name_of_trait(cx.session(), hir, trait_ref.def),
                cx.show(second.self_ty)
            ),
            extend_span(hir, second),
        )
        .with_code(codes::CONFLICTING_EXTENDS)
        .with_label("conflicting implementation")
        .with_secondary(
            extend_span(hir, first),
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
    cx: DisplayCtx<'_>,
    name: Symbol,
    first: &ExtendHeader,
    second: &ExtendHeader,
) {
    cx.emit(
        Diagnostic::error(
            format!(
                "the method `{}` is defined more than once for type `{}`",
                cx.resolve(name),
                cx.show(second.self_ty)
            ),
            extend_span(hir, second),
        )
        .with_code(codes::DUPLICATE_METHOD)
        .with_label(format!("duplicate definition of `{}`", cx.resolve(name)))
        .with_secondary(
            extend_span(hir, first),
            format!(
                "`{}` already gets a method named `{}` here",
                cx.show(first.self_ty),
                cx.resolve(name)
            ),
        )
        .with_help(
            "a call to it would have no single meaning, so one of the two has to be renamed \
                 or removed",
        ),
    );
}
