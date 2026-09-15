use crate::diagnostics::Diagnostic;
use crate::diagnostics::codes;
use crate::driver::source::SrcSpan;
use crate::session::Session;

pub fn report_not_all_paths_return(session: &Session, span: SrcSpan) {
    session.emit(
        Diagnostic::error("not all control flow paths return a value", span)
            .with_code(codes::NOT_ALL_PATHS_RETURN)
            .with_label("this function does not always return"),
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_diagnostic_names_the_paths_that_fall_through() {
        let session = crate::testing::session();
        session.clear_diagnostics();

        report_not_all_paths_return(session, SrcSpan::new(0, 1));

        assert_eq!(
            session.messages(),
            ["not all control flow paths return a value"]
        );
    }
}
