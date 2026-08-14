use crate::ast::Symbol;
use crate::ast::interner::Interner;
use crate::diag::{DiagCtx, Diagnostic};
use crate::diagnostics::typeck::display::DisplayCx;
use crate::diagnostics::typeck::traits::trait_name;
use crate::hir::Hir;
use crate::typeck::traits::index::ImplHeader;

/// Reports a conflict, pointing at the *second* of the two blocks and underlining the first
/// beneath it.
///
/// Which one is "the error" is a real choice, not an arbitrary one: neither block is wrong
/// on its own, and either could be the one to delete. The later block gets the primary span
/// because it is the one that introduced the conflict into a program that did not have it.
pub fn report_conflicting_impls(
    hir: &Hir,
    cx: DisplayCx<'_>,
    first: &ImplHeader,
    second: &ImplHeader,
) {
    let trait_ref = second
        .trait_ref
        .as_ref()
        .expect("only two trait impls are compared for a duplicate implementation");

    DiagCtx::emit(
        Diagnostic::error(
            format!(
                "conflicting implementations of trait `{}` for type `{}`",
                trait_name(hir, trait_ref.def),
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
    first: &ImplHeader,
    second: &ImplHeader,
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
