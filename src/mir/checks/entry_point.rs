use crate::diagnostics::mir::entry_point::{
    report_ambiguous_main, report_main_is_generic, report_main_returns_a_value,
    report_main_takes_parameters, report_missing_main,
};
use crate::hir::{DefId, Hir};
use crate::mir::lower::crate_root_main_candidates;

/// A crate has at most one crate-root `main`, and if it has one it must be callable with no
/// arguments and return nothing: not generic, no parameters, no declared return type. Those are
/// errors, since otherwise they surface far later as an LLVM verification failure (a
/// zero-argument call into a parameterized `main`).
///
/// Having *no* `main` is only a warning. Such a crate still compiles and links; codegen emits a
/// `main` that returns without doing anything, so what comes out is a runnable executable with
/// no entry point of its own rather than a build failure.
pub fn check(hir: &Hir) {
    let candidates = crate_root_main_candidates(hir);
    match candidates.as_slice() {
        [] => report_missing_main(),
        [one] => check_signature(hir, *one),
        _ => report_ambiguous_main(hir, &candidates),
    }
}

fn check_signature(hir: &Hir, def_id: DefId) {
    let function = hir.function(def_id);
    if !function.params.is_empty() || function.self_param.is_some() {
        let param_count = function.params.len() + usize::from(function.self_param.is_some());
        report_main_takes_parameters(function.span, param_count);
    }
    if let Some(ret) = function.ret {
        report_main_returns_a_value(hir.ty(ret).span);
    }
    if !function.generics.is_empty() {
        report_main_is_generic(function.span);
    }
}

#[cfg(test)]
mod tests {
    use super::check;
    use crate::diagnostics::{DiagCtx, Severity};
    use crate::testing;

    fn diagnose(sources: &[&str], needle: &str) -> Severity {
        let (hir, _tcx, _types, _mir, _instances) = testing::lower_mir_src_files(sources);
        DiagCtx::clear();
        check(&hir);
        let reported = DiagCtx::diagnostics();
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
        let (hir, _tcx, _types, mir, _instances) = testing::lower_mir_src_files(&[
            "module app;\n\nfun main() {\n}\n",
            "module app::inner;\n\nfun main() {\n}\n",
        ]);
        DiagCtx::clear();
        check(&hir);
        assert!(DiagCtx::messages().is_empty());
        assert!(mir.main.is_some());
    }

    #[test]
    fn a_parameterless_main_is_fine() {
        let (hir, _tcx, _types, mir, _instances) =
            testing::lower_to_mir("fun main() { let x = 1; }");
        DiagCtx::clear();
        check(&hir);
        assert!(DiagCtx::messages().is_empty());
        assert!(mir.main.is_some());
    }
}
