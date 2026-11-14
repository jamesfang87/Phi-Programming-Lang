use crate::diagnostics::Diagnostic;
use crate::diagnostics::codes;
use crate::driver::source::SrcSpan;
use crate::hir::{DefId, Hir, OwnerNode, Path};
use crate::session::Session;

pub fn report_bound_is_not_a_trait(session: &Session, path: &Path) {
    let name = path
        .segments
        .last()
        .map_or("this path", |segment| session.resolve(segment.text));

    session.emit(
        Diagnostic::error(format!("`{name}` is not a trait"), path.span())
            .with_code(codes::BOUND_IS_NOT_A_TRAIT)
            .with_label("not a trait"),
    );
}

pub fn report_arg_count_mismatch(
    session: &Session,
    hir: &Hir,
    def: DefId,
    declared: usize,
    found: usize,
    span: SrcSpan,
) {
    let plural = if declared == 1 { "" } else { "s" };
    session.emit(
        Diagnostic::error(
            format!(
                "`{}` takes {declared} generic argument{plural} but {found} {} supplied",
                defined_name(session, hir, def),
                if found == 1 { "was" } else { "were" }
            ),
            span,
        )
        .with_code(codes::ARG_COUNT_MISMATCH)
        .with_label(format!("expected {declared} argument{plural}"))
        .with_secondary(
            defined_span(hir, def),
            format!(
                "`{}` declares {declared} type parameter{plural} here",
                defined_name(session, hir, def)
            ),
        )
        .with_help(
            "every parameter has to be given an argument, since the declaration is written in \
                 terms of all of them",
        ),
    );
}

fn defined_name(session: &Session, hir: &Hir, def: DefId) -> &'static str {
    let name = match hir.def(def) {
        OwnerNode::Function(f) => f.name.text,
        OwnerNode::Struct(s) => s.name.text,
        OwnerNode::Enum(e) => e.name.text,
        OwnerNode::Trait(t) => t.name.text,
        OwnerNode::Extend(_) | OwnerNode::Module(_) | OwnerNode::Closure(_) => {
            unreachable!("only a named definition is ever applied to generic arguments")
        }
    };
    session.resolve(name)
}

fn defined_span(hir: &Hir, def: DefId) -> SrcSpan {
    match hir.def(def) {
        OwnerNode::Function(f) => f.name.span,
        OwnerNode::Struct(s) => s.name.span,
        OwnerNode::Enum(e) => e.name.span,
        OwnerNode::Trait(t) => t.name.span,
        OwnerNode::Extend(_) | OwnerNode::Module(_) | OwnerNode::Closure(_) => {
            unreachable!("only a named definition is ever applied to generic arguments")
        }
    }
}

pub fn report_missing_associated_type(
    session: &Session,
    hir: &Hir,
    trait_def: DefId,
    name: &str,
    span: SrcSpan,
) {
    session.emit(
        Diagnostic::error(
            format!(
                "missing associated type `{name}` in the implementation of trait `{}`",
                defined_name(session, hir, trait_def)
            ),
            span,
        )
        .with_code(codes::MISSING_ASSOCIATED_TYPE)
        .with_label("this implementation does not define it")
        .with_secondary(
            defined_span(hir, trait_def),
            format!(
                "`{}` is declared here",
                defined_name(session, hir, trait_def)
            ),
        ),
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testing::{lower_to_hir, named_def, session};

    const NAMED_DEFINITIONS: &str = "
        fun function<T>() {}
        struct Structure<T> { value: T }
        enum Enumeration<T> { some: T }
        trait TraitDeclaration<T> { }
    ";

    #[test]
    fn every_named_definition_reports_its_name_and_span() {
        let hir = lower_to_hir(NAMED_DEFINITIONS);

        for name in ["function", "Structure", "Enumeration", "TraitDeclaration"] {
            session().clear_diagnostics();
            let def = named_def(&hir, name);

            report_arg_count_mismatch(session(), &hir, def, 1, 0, SrcSpan::new(0, 1));

            let messages = session().messages();
            assert_eq!(messages.len(), 1, "for {name}: {messages:?}");
            assert!(
                messages[0].contains(name),
                "the diagnostic should name {name}: {messages:?}"
            );
        }
    }

    #[test]
    fn a_missing_associated_type_names_the_trait_and_the_type() {
        let hir = lower_to_hir("trait Container { type Item; }");
        let trait_def = named_def(&hir, "Container");
        session().clear_diagnostics();

        report_missing_associated_type(session(), &hir, trait_def, "Item", SrcSpan::new(0, 1));

        assert_eq!(
            session().messages(),
            ["missing associated type `Item` in the implementation of trait `Container`"]
        );
    }
}
