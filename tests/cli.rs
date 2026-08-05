//! End-to-end tests for the dispatch surface: `cli::main`, `exit_code`, `with_config`, and the
//! three `pipeline` entry points (`check`/`build`/`run`).
//!
//! `tests/golden.rs` only ever invokes `phi build --ast` on a fixture that already has a valid
//! `Phi.toml` and `src/` directory, so it never exercises argument parsing failures, a missing
//! manifest, a missing `src/` directory, or the mapping from a pipeline's `Ok`/`Err` result to
//! an exit code. This file invokes the real binary the same way `golden.rs` does (via
//! `env!("CARGO_BIN_EXE_phi")`), against scratch directories under `target/`, to close that gap.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

/// A fresh empty directory under `target/`, named after the calling test, so reruns are
/// deterministic and tests don't interfere with each other.
fn scratch(name: &str) -> PathBuf {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("target/test-scratch/cli")
        .join(name);
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).expect("could not create the scratch directory");
    dir
}

/// Writes a minimal manifest naming `project_name` into `dir`.
fn write_manifest(dir: &Path, project_name: &str) {
    fs::write(
        dir.join("Phi.toml"),
        format!("[project]\nname = \"{project_name}\"\nversion = \"0.1.0\"\nedition = \"2026\"\n"),
    )
    .expect("could not write Phi.toml");
}

/// Writes a manifest naming `project_name` with `mode = "release"` into `dir`.
fn write_release_manifest(dir: &Path, project_name: &str) {
    fs::write(
        dir.join("Phi.toml"),
        format!(
            "[project]\nname = \"{project_name}\"\nversion = \"0.1.0\"\nedition = \"2026\"\n\
             [profile]\nmode = \"release\"\n"
        ),
    )
    .expect("could not write Phi.toml");
}

/// Writes `contents` to `dir/src/main.phi`, creating `src/` first.
fn write_main(dir: &Path, contents: &str) {
    let src_dir = dir.join("src");
    fs::create_dir_all(&src_dir).expect("could not create src/");
    fs::write(src_dir.join("main.phi"), contents).expect("could not write main.phi");
}

/// A project whose `main.phi` type checks cleanly under today's checker.
///
/// Arithmetic (`typeck.rs:549`), `&expr` (`typeck.rs:553`), and string literals
/// (`typeck.rs:605`) all `panic!` via `todo!()` and are out of scope to fix, so this avoids all
/// three: no operators, no borrows, no string literals. An empty `main` body types as `()`
/// trivially, which is enough to prove the dispatch surface without depending on any
/// unimplemented checker path.
const CLEAN_MAIN: &str = "module clean;\n\nfun main() {\n}\n";

/// A project with a genuine type error a real compiler user would hit: a function declared to
/// return `bool` but returning an integer literal instead. This is the case that guards
/// `exit_code`'s `Ok(false) => 1` mapping -- a regression there (mapping a failed compilation to
/// exit code `0`) would make this test the only one in the suite to notice.
const TYPE_ERROR_MAIN: &str = "module broken;\n\nfun broken() -> bool {\n    return 1;\n}\n";

fn run(dir: &Path, args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_phi"))
        .args(args)
        .current_dir(dir)
        .output()
        .expect("failed to run the `phi` binary")
}

fn stderr(output: &Output) -> String {
    String::from_utf8_lossy(&output.stderr).into_owned()
}

fn stdout(output: &Output) -> String {
    String::from_utf8_lossy(&output.stdout).into_owned()
}

fn code(output: &Output) -> i32 {
    output
        .status
        .code()
        .expect("the process should exit normally")
}

#[test]
fn missing_manifest_is_reported_by_name() {
    let dir = scratch("missing_manifest");
    let output = run(&dir, &["build"]);
    assert_ne!(code(&output), 0);
    assert!(
        stderr(&output).contains("Phi.toml"),
        "the error should name the missing manifest: {}",
        stderr(&output)
    );
}

#[test]
fn missing_src_directory_is_reported_by_name() {
    let dir = scratch("missing_src");
    write_manifest(&dir, "missing_src");
    // No src/ directory created.
    let output = run(&dir, &["build"]);
    assert_ne!(code(&output), 0);
    assert!(
        stderr(&output).contains("src"),
        "the error should name the missing `src` directory: {}",
        stderr(&output)
    );
}

#[test]
fn an_unknown_flag_is_named_alongside_a_known_one() {
    let dir = scratch("unknown_flag");
    write_manifest(&dir, "unknown_flag");
    write_main(&dir, CLEAN_MAIN);
    let output = run(&dir, &["build", "--nope"]);
    assert_ne!(code(&output), 0);
    let err = stderr(&output);
    assert!(err.contains("--nope"), "{err}");
    assert!(
        err.contains("--ast"),
        "the message lists an accepted flag: {err}"
    );
}

#[test]
fn an_unknown_command_is_named_in_the_error() {
    let dir = scratch("unknown_command");
    let output = run(&dir, &["frobnicate"]);
    assert_ne!(code(&output), 0);
    assert!(
        stderr(&output).contains("frobnicate"),
        "{}",
        stderr(&output)
    );
}

#[test]
fn no_arguments_prints_usage_and_fails() {
    let dir = scratch("no_arguments");
    let output = run(&dir, &[]);
    assert_ne!(code(&output), 0);
    assert!(stderr(&output).contains("Usage:"), "{}", stderr(&output));
}

#[test]
fn help_prints_usage_and_succeeds() {
    let dir = scratch("help");
    let output = run(&dir, &["help"]);
    assert_eq!(code(&output), 0);
    assert!(
        stderr(&output).contains("Usage:") || stdout(&output).contains("Usage:"),
        "stdout: {}\nstderr: {}",
        stdout(&output),
        stderr(&output)
    );
}

#[test]
fn a_clean_project_checks_successfully() {
    let dir = scratch("clean_check");
    write_manifest(&dir, "clean_check");
    write_main(&dir, CLEAN_MAIN);
    let output = run(&dir, &["check"]);
    assert_eq!(
        code(&output),
        0,
        "stdout: {}\nstderr: {}",
        stdout(&output),
        stderr(&output)
    );
}

/// The single most valuable assertion in this file: it guards `exit_code`'s `Ok(false) => 1`
/// mapping. Flipping that arm to `=> 0` would make every failing compilation silently report
/// success, and only this test (of the whole existing suite) would notice.
#[test]
fn a_type_error_fails_the_build() {
    let dir = scratch("type_error_build");
    write_manifest(&dir, "type_error_build");
    write_main(&dir, TYPE_ERROR_MAIN);
    let output = run(&dir, &["build"]);
    assert_ne!(
        code(&output),
        0,
        "a build with a genuine type error must not exit 0\nstdout: {}\nstderr: {}",
        stdout(&output),
        stderr(&output)
    );
}

#[test]
fn release_mode_is_noted_as_having_no_effect() {
    let dir = scratch("release_mode_noted");
    write_release_manifest(&dir, "release_mode_noted");
    write_main(&dir, CLEAN_MAIN);
    let output = run(&dir, &["check"]);
    assert_eq!(code(&output), 0);
    assert!(
        stderr(&output).contains("release"),
        "release mode should be noted as having no effect: {}",
        stderr(&output)
    );
}

#[test]
fn debug_mode_prints_no_release_note() {
    let dir = scratch("debug_mode_silent");
    write_manifest(&dir, "debug_mode_silent");
    write_main(&dir, CLEAN_MAIN);
    let output = run(&dir, &["check"]);
    assert_eq!(code(&output), 0);
    assert!(
        !stderr(&output).contains("release"),
        "debug mode should not print the release note: {}",
        stderr(&output)
    );
}

#[test]
fn run_on_a_clean_project_reports_the_missing_backend() {
    let dir = scratch("run_missing_backend");
    write_manifest(&dir, "run_missing_backend");
    write_main(&dir, CLEAN_MAIN);
    let output = run(&dir, &["run"]);
    assert_ne!(code(&output), 0);
    assert!(
        stderr(&output).contains("backend"),
        "the error should mention the missing code generation backend: {}",
        stderr(&output)
    );
}
