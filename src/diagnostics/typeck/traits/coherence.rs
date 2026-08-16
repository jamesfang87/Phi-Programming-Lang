use crate::ast::Symbol;
use crate::ast::interner::Interner;
use crate::diagnostics::typeck::display::DisplayCx;
use crate::diagnostics::typeck::traits::get_name_of_trait;
use crate::diagnostics::{DiagCtx, Diagnostic};
use crate::hir::Hir;
use crate::typeck::traits::index::ExtendHeader;

pub fn report_conflicting_extends(
    hir: &Hir,
    cx: DisplayCx<'_>,
    first: &ExtendHeader,
    second: &ExtendHeader,
) {
    let trait_ref = second
        .trait_ref
        .as_ref()
        .expect("only two trait impls are compared for a duplicate implementation");

    DiagCtx::emit(
        Diagnostic::error(
            format!(
                "conflicting implementations of trait `{}` for type `{}`",
                get_name_of_trait(hir, trait_ref.def),
                cx.show(second.self_ty)
            ),
            second.span,
        )
        .with_label("conflicting implementation")
        .with_secondary(
            first.span,
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
            second.span,
        )
        .with_label(format!(
            "duplicate definition of `{}`",
            Interner::resolve(name)
        ))
        .with_secondary(
            first.span,
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
