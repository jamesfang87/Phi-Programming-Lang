use crate::diagnostics::Diagnostic;
use crate::diagnostics::codes;
use crate::driver::source::SrcSpan;
use crate::session::Session;

pub fn report_extend_trait(session: &Session, span: SrcSpan) {
    session.emit(
        Diagnostic::error("a trait cannot be extended", span)
            .with_code(codes::EXTEND_TRAIT)
            .with_label("not a struct or enum")
            .with_help("the identifier after `extend` must not be a trait"),
    );
}

pub fn report_extend_generic(session: &Session, span: SrcSpan) {
    session.emit(
        Diagnostic::error("a generic type parameter cannot be extended", span)
            .with_code(codes::EXTEND_GENERIC)
            .with_label("not a struct or enum"),
    );
}

pub fn report_extend_any(session: &Session, span: SrcSpan) {
    session.emit(Diagnostic::error("`any` cannot be extended", span).with_code(codes::EXTEND_ANY));
}

pub fn report_extend_dyn(session: &Session, span: SrcSpan) {
    session.emit(
        Diagnostic::error("`dyn Trait` cannot be extended", span)
            .with_code(codes::EXTEND_DYN)
            .with_label("already satisfies its own trait")
            .with_help("a `dyn Trait` value already satisfies `Trait` by definition"),
    );
}

pub fn report_extend_bare_self(session: &Session, span: SrcSpan) {
    session.emit(
        Diagnostic::error("`Self` cannot be extended", span)
            .with_code(codes::EXTEND_BARE_SELF)
            .with_help("an `extend` block has to name a concrete type"),
    );
}

pub fn report_attempt_to_extend_with_non_trait(session: &Session, span: SrcSpan) {
    session.emit(
        Diagnostic::error("`with` must name a trait", span)
            .with_code(codes::EXTEND_WITH_NON_TRAIT)
            .with_label("not a trait")
            .with_help("types can only be extend with trait types"),
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testing::session;

    #[test]
    fn extending_bare_self_names_the_missing_concrete_type() {
        session().clear_diagnostics();

        report_extend_bare_self(session(), SrcSpan::new(0, 1));

        assert_eq!(session().messages(), ["`Self` cannot be extended"]);
    }
}
