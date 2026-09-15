use crate::ast::Ident;
use crate::diagnostics::Diagnostic;
use crate::diagnostics::codes;
use crate::driver::source::SrcSpan;
use crate::session::Session;

pub fn report_not_found(session: &Session, name: Ident) {
    session.emit(
        Diagnostic::error(
            format!("cannot find `{}` in this scope", session.resolve(name.text)),
            name.span,
        )
        .with_code(codes::UNRESOLVED_NAME)
        .with_label("not found in this scope"),
    );
}

pub fn report_conflict(session: &Session, name: Ident) {
    session.emit(
        Diagnostic::error(
            format!(
                "the name `{}` is defined multiple times",
                session.resolve(name.text)
            ),
            name.span,
        )
        .with_code(codes::DUPLICATE_NAME)
        .with_label("redefined here")
        .with_help("a name with the same spelling is already in scope"),
    );
}

pub fn report_duplicate_bound(session: &Session, name: Ident) {
    session.emit(
        Diagnostic::error(
            format!("duplicate bound `{}`", session.resolve(name.text)),
            name.span,
        )
        .with_code(codes::DUPLICATE_BOUND)
        .with_label("already listed for this type parameter")
        .with_help("a bound only needs to be written once, even if satisfied redundantly"),
    );
}

pub fn report_self_extend(session: &Session, name: Ident) {
    session.emit(
        Diagnostic::error(
            format!(
                "`extend` target and trait are the same type: `{}`",
                session.resolve(name.text)
            ),
            name.span,
        )
        .with_code(codes::SELF_EXTEND)
        .with_label("names the same type as the `extend` target")
        .with_help("a type cannot implement itself as a trait"),
    );
}

pub fn report_ambiguous_import(session: &Session, name: Ident) {
    session.emit(
        Diagnostic::error(
            format!(
                "ambiguous import: `{}` refers to more than one item",
                session.resolve(name.text)
            ),
            name.span,
        )
        .with_code(codes::AMBIGUOUS_IMPORT)
        .with_label("ambiguous import")
        .with_help(
            "this path matches more than one declaration; use a more specific path to disambiguate",
        ),
    );
}

pub fn report_private_item(session: &Session, name: Ident) {
    session.emit(
        Diagnostic::error(
            format!("`{}` is private", session.resolve(name.text)),
            name.span,
        )
        .with_code(codes::PRIVATE_ITEM)
        .with_label("not visible from here")
        .with_help("mark the declaration `public` to use it outside its own module"),
    );
}

pub fn report_dyn_not_trait(session: &Session, span: SrcSpan) {
    session.emit(
        Diagnostic::error("`dyn` requires a trait".to_string(), span)
            .with_code(codes::DYN_NOT_TRAIT)
            .with_label("not a trait")
            .with_help(
                "`dyn` dispatches dynamically over a trait; a struct or enum is used directly",
            ),
    );
}

pub fn report_self_unavailable(session: &Session, span: SrcSpan) {
    session.emit(
        Diagnostic::error("`Self` is not available here".to_string(), span)
            .with_code(codes::SELF_UNAVAILABLE)
            .with_label("no enclosing struct, enum, trait, or `extend` block")
            .with_help("`Self` names the definition it is written inside"),
    );
}
