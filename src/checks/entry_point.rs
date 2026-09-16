use crate::diagnostics::checks::entry_point::{
    report_ambiguous_main, report_main_is_generic, report_main_returns_a_value,
    report_main_takes_parameters, report_missing_main,
};
use crate::hir::{DefId, Hir, OwnerNode};
use crate::session::Session;

pub fn check(session: &Session, hir: &Hir) -> Option<DefId> {
    let candidates = crate_root_main_candidates(session, hir);
    match candidates.as_slice() {
        [] => {
            report_missing_main(session);
            None
        }
        [one] => {
            check_signature(session, hir, *one);
            Some(*one)
        }
        _ => {
            report_ambiguous_main(session, hir, &candidates);
            None
        }
    }
}

fn check_signature(session: &Session, hir: &Hir, def_id: DefId) {
    let function = hir.function(def_id);
    if !function.params.is_empty() || function.self_param.is_some() {
        let param_count = function.params.len() + usize::from(function.self_param.is_some());
        report_main_takes_parameters(session, function.span, param_count);
    }
    if let Some(ret) = function.ret {
        report_main_returns_a_value(session, hir.ty(ret).span);
    }
    if !function.generics.is_empty() {
        report_main_is_generic(session, function.span);
    }
}

/// Every function named `main` at the crate root or in one of the root's direct child modules,
/// in declaration order. A `main` nested any deeper is not a candidate: codegen would not call
/// it, so it must not be mistaken for an entry point here.
pub(crate) fn crate_root_main_candidates(session: &Session, hir: &Hir) -> Vec<DefId> {
    let root = hir.root();
    let mut candidates: Vec<DefId> = root
        .items
        .iter()
        .copied()
        .filter(|&def| is_named_main(session, hir, def))
        .collect();

    for &item in &root.items {
        if let OwnerNode::Module(child) = hir.def(item) {
            candidates.extend(
                child
                    .items
                    .iter()
                    .copied()
                    .filter(|&def| is_named_main(session, hir, def)),
            );
        }
    }

    candidates
}

fn is_named_main(session: &Session, hir: &Hir, def: DefId) -> bool {
    matches!(
        hir.def(def),
        OwnerNode::Function(f) if session.resolve(f.name.text) == "main"
    )
}

#[cfg(test)]
mod tests {
    use super::check;
    use crate::diagnostics::Severity;
    use crate::testing;

    fn diagnose(sources: &[&str], needle: &str) -> Severity {
        let hir = testing::lower_to_hir_files(sources);
        crate::testing::clear_diagnostics();
        check(crate::testing::session(), &hir);
        let reported = crate::testing::diagnostics();
        assert_eq!(reported.len(), 1, "for {sources:?}: {reported:?}");
        assert!(
            reported[0].message.contains(needle),
            "expected a diagnostic mentioning {needle:?} for {sources:?}, got {reported:?}"
        );
        reported[0].severity
    }

    fn rejects(sources: &[&str], needle: &str) {
        assert_eq!(
            diagnose(sources, needle),
            Severity::Error,
            "expected {needle:?} to be an error for {sources:?}"
        );
    }

    /// A crate with no entry point still builds; the executable simply does nothing. Only the
    /// severity separates this from the malformed-`main` cases below.
    #[test]
    fn a_missing_main_is_only_a_warning() {
        assert_eq!(
            diagnose(&["fun f() {}\n"], "no `main` function found"),
            Severity::Warning
        );
    }

    #[test]
    fn a_main_with_parameters_is_an_error() {
        rejects(
            &["fun main(argc: i32) { let x = argc; }"],
            "the `main` function cannot take parameters",
        );
    }

    #[test]
    fn a_main_with_a_return_type_is_an_error() {
        rejects(
            &["fun main() -> i32 { return 1; }"],
            "the `main` function cannot return a value",
        );
    }

    #[test]
    fn a_generic_main_is_an_error() {
        rejects(
            &["fun main<T>() { }"],
            "the `main` function cannot be generic",
        );
    }

    #[test]
    fn two_crate_root_mains_are_an_error() {
        rejects(
            &[
                "module app;\n\nfun main() {\n}\n",
                "module other;\n\nfun main() {\n}\n",
            ],
            "the entry point is ambiguous",
        );
    }

    #[test]
    fn a_root_main_with_a_nested_module_main_is_fine() {
        let (hir, _tcx, _types, _mir, _instances) = testing::lower_mir_src_files(&[
            "module app;\n\nfun main() {\n}\n",
            "module app::inner;\n\nfun main() {\n}\n",
        ]);
        crate::testing::clear_diagnostics();
        let main = check(crate::testing::session(), &hir);
        assert!(crate::testing::messages().is_empty());
        assert!(main.is_some());
    }

    #[test]
    fn a_parameterless_main_is_fine() {
        let (hir, _tcx, _types, _mir, _instances) =
            testing::lower_to_mir("fun main() { let x = 1; }");
        crate::testing::clear_diagnostics();
        let main = check(crate::testing::session(), &hir);
        assert!(crate::testing::messages().is_empty());
        assert!(main.is_some());
    }
}
