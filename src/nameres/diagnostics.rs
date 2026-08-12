//! The diagnostics AST-level name resolution emits. All seven go through `DiagCtx::emit`.

use crate::ast::interner::Interner;
use crate::ast::Ident;
use crate::diag::{DiagCtx, Diagnostic};
use crate::driver::source::SrcSpan;

pub fn report_not_found(name: Ident) {
    DiagCtx::emit(
        Diagnostic::error(
            format!(
                "cannot find `{}` in this scope",
                Interner::resolve(name.text)
            ),
            name.span,
        )
        .with_label("not found in this scope"),
    );
}

pub fn report_conflict(name: Ident) {
    DiagCtx::emit(
        Diagnostic::error(
            format!(
                "the name `{}` is defined multiple times",
                Interner::resolve(name.text)
            ),
            name.span,
        )
        .with_label("redefined here")
        .with_help("a name with the same spelling is already in scope"),
    );
}

pub fn report_duplicate_bound(name: Ident) {
    DiagCtx::emit(
        Diagnostic::error(
            format!("duplicate bound `{}`", Interner::resolve(name.text)),
            name.span,
        )
        .with_label("already listed for this type parameter")
        .with_help("a bound only needs to be written once, even if satisfied redundantly"),
    );
}

pub fn report_self_extend(name: Ident) {
    DiagCtx::emit(
        Diagnostic::error(
            format!(
                "`extend` target and trait are the same type: `{}`",
                Interner::resolve(name.text)
            ),
            name.span,
        )
        .with_label("names the same type as the `extend` target")
        .with_help("a type cannot implement itself as a trait"),
    );
}

pub fn report_ambiguous_import(name: Ident) {
    DiagCtx::emit(
        Diagnostic::error(
            format!(
                "ambiguous import: `{}` refers to more than one item",
                Interner::resolve(name.text)
            ),
            name.span,
        )
        .with_label("ambiguous import")
        .with_help(
            "this path matches more than one declaration; use a more specific path to disambiguate",
        ),
    );
}

pub fn report_dyn_not_trait(span: SrcSpan) {
    DiagCtx::emit(
        Diagnostic::error("`dyn` requires a trait".to_string(), span)
            .with_label("not a trait")
            .with_help(
                "`dyn` dispatches dynamically over a trait; a struct or enum is used directly",
            ),
    );
}

pub fn report_self_unavailable(span: SrcSpan) {
    DiagCtx::emit(
        Diagnostic::error("`Self` is not available here".to_string(), span)
            .with_label("no enclosing struct, enum, trait, or `extend` block")
            .with_help("`Self` names the definition it is written inside"),
    );
}
