use super::pipeline::{
    mir_captures_src, mir_definite_init_src, mir_element_moves_src, mir_exclusivity_src,
    mir_never_read_src, typeck_src,
};

fn assert_no_diagnostics(reported: Vec<String>, src: &str, what: &str) {
    assert!(
        reported.is_empty(),
        "expected {src:?} to {what}: {reported:?}"
    );
}

fn assert_single_diagnostic_mentioning(reported: Vec<String>, src: &str, needle: &str) {
    assert_eq!(reported.len(), 1, "for {src:?}: {reported:?}");
    assert!(
        reported[0].contains(needle),
        "expected a diagnostic mentioning {needle:?} for {src:?}, got {reported:?}"
    );
}

pub fn typeck_accepts(src: &str) {
    assert_no_diagnostics(typeck_src(src), src, "type-check");
}

pub fn typeck_rejects(src: &str, needle: &str) {
    assert_single_diagnostic_mentioning(typeck_src(src), src, needle);
}

pub fn mir_definite_init_accepts(src: &str) {
    assert_no_diagnostics(
        mir_definite_init_src(src),
        src,
        "pass definite-initialization checking",
    );
}

pub fn mir_definite_init_rejects(src: &str, needle: &str) {
    assert_single_diagnostic_mentioning(mir_definite_init_src(src), src, needle);
}

pub fn mir_captures_accepts(src: &str) {
    assert_no_diagnostics(
        mir_captures_src(src),
        src,
        "pass the closure-capture move check",
    );
}

pub fn mir_captures_rejects(src: &str, needle: &str) {
    assert_single_diagnostic_mentioning(mir_captures_src(src), src, needle);
}

pub fn mir_element_moves_accepts(src: &str) {
    assert_no_diagnostics(
        mir_element_moves_src(src),
        src,
        "pass the array-element move check",
    );
}

pub fn mir_element_moves_rejects(src: &str, needle: &str) {
    assert_single_diagnostic_mentioning(mir_element_moves_src(src), src, needle);
}

pub fn mir_exclusivity_accepts(src: &str) {
    assert_no_diagnostics(mir_exclusivity_src(src), src, "pass exclusivity checking");
}

pub fn mir_exclusivity_rejects(src: &str, needle: &str) {
    assert_single_diagnostic_mentioning(mir_exclusivity_src(src), src, needle);
}

pub fn mir_never_read_accepts(src: &str) {
    assert_no_diagnostics(mir_never_read_src(src), src, "pass never-read checking");
}

pub fn mir_never_read_rejects(src: &str, needle: &str) {
    assert_single_diagnostic_mentioning(mir_never_read_src(src), src, needle);
}
