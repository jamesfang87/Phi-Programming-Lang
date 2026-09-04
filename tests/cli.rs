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
fn release_mode_builds_and_runs_with_no_note() {
    let dir = scratch("release_mode_runs");
    write_release_manifest(&dir, "release_mode_runs");
    write_main(&dir, CLEAN_MAIN);
    let output = run(&dir, &["run"]);
    assert_eq!(
        code(&output),
        0,
        "stdout: {}\nstderr: {}",
        stdout(&output),
        stderr(&output)
    );
    assert!(
        !stderr(&output).contains("release"),
        "release mode is implemented now (O2 via `EmitOptions::release`); no note should print: {}",
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
fn run_on_a_clean_project_actually_runs() {
    let dir = scratch("run_clean_project");
    write_manifest(&dir, "run_clean_project");
    write_main(&dir, CLEAN_MAIN);
    let output = run(&dir, &["run"]);
    assert_eq!(
        code(&output),
        0,
        "an empty `main` should build, link, and run successfully\nstdout: {}\nstderr: {}",
        stdout(&output),
        stderr(&output)
    );
}

/// The end-to-end proof that `phi run` runs a program: a real `main` that writes bytes to
/// stdout via `core::io::write_bytes`, run through the actual `phi` binary (parse through
/// codegen, object emission, linking, and process execution), with its stdout checked
/// byte-for-byte and its exit status checked to be 0.
#[test]
fn run_executes_a_program_and_propagates_exit_status() {
    let dir = scratch("run_hello_world");
    write_manifest(&dir, "run_hello_world");
    write_main(
        &dir,
        "module app;\n\nfun main() {\n    core::io::write_bytes(1, \"hello\" as &[u8]);\n}\n",
    );
    let output = run(&dir, &["run"]);
    assert_eq!(
        code(&output),
        0,
        "stdout: {}\nstderr: {}",
        stdout(&output),
        stderr(&output)
    );
    assert_eq!(stdout(&output), "hello");
}

/// `--mir` dumps a real stage (Lowering #2 plus monomorphization). This is the end-to-end
/// counterpart to `mir::lower`'s and
/// `mir::monomorphize`'s own unit tests: it exercises the whole `phi build --mir` path through
/// the real binary, the way `tests/golden.rs` does for `--ast`.
#[test]
fn mir_dump_shows_the_lowered_and_monomorphized_function() {
    let dir = scratch("mir_dump");
    write_manifest(&dir, "mir_dump");
    write_main(
        &dir,
        "fun add(x: i32, y: i32) -> i32 {\n    return x + y;\n}\n\nfun main() {\n    let z = add(1, 2);\n}\n",
    );
    let output = run(&dir, &["build", "--mir", "--emit-debug", "--no-emit-core"]);
    assert_eq!(
        code(&output),
        0,
        "stdout: {}\nstderr: {}",
        stdout(&output),
        stderr(&output)
    );
    let out = stdout(&output);
    assert!(out.contains("=== MIR ==="), "no MIR dump header: {out}");
    assert!(
        out.contains("Call") && out.contains("Return"),
        "the dump should show add's Call terminator (from main) and its own Return: {out}"
    );
}

// -----------------------------------------------------------------
// Task 13: end-to-end fixtures proving the back end works on real programs
// -----------------------------------------------------------------
//
// Each test below compiles, links, and runs a real `.phi` program through the actual `phi`
// binary and asserts on its observable behavior (stdout bytes, exit status) -- not merely that
// codegen produced valid IR (Tasks 2-12's own inline tests already cover that). Two items from
// the task brief's numbered list are not represented here:
//
// - Array indexing (item 5): there is no source-level array-literal expression in this version
//   of the language at all (`codegen::body`'s `array_aggregate_builds_via_insert_value` test
//   documents this: `AggregateKind::Array` is only ever reachable by hand-built MIR). The only
//   surface-level array constructor is `new [elem; count]`, which produces `iso [T]`, and
//   indexing (`typeck::expr::check_index`) reaches an array's element type only by peeling
//   `&`/`&mut`/`any` layers off the base (`peel_receiver`) -- it does not peel `iso`. Per the
//   runtime-semantics design doc (Sec. 3, "AST, HIR, typeck"): "There is no auto-deref in this
//   spec; a method call on an `iso` receiver and field access through one are deliberately left
//   to a later spec." So there is no program a real user could write today that produces a
//   sized, indexable array value; this is a documented, deliberate gap, not a bug to route
//   around.
// - `new`/`iso` allocation and field/element access (item 6, optional): blocked by the exact
//   same deliberate no-auto-deref gap above -- `p.x` on `p: iso Point` fails typeck with `no
//   field `x` on `iso Point`` because `peel_receiver` does not strip `Iso`, and `*p` fails with
//   `` `iso Point` cannot be dereferenced `` because `check_deref` only accepts `TyKind::Ref`.
//   Confirmed by hand against the built binary before writing this comment.

/// Item 1: an unconditional `i32` addition that overflows aborts the process. The overflow
/// happens on the addition itself, so the following `if` (added only to give `c` a read and
/// keep the test's stderr free of an unrelated "never read" warning) is never reached -- the
/// process aborts before it can print `unreachable`.
#[test]
fn integer_overflow_aborts_with_a_nonzero_exit_code() {
    let dir = scratch("overflow_aborts");
    write_manifest(&dir, "overflow_aborts");
    write_main(
        &dir,
        "module app;\n\n\
         fun main() {\n    \
             let a: i32 = 2147483647;\n    \
             let b: i32 = 1;\n    \
             let c = a + b;\n    \
             if c == 0 {\n        \
                 core::io::write_bytes(1, \"unreachable\" as &[u8]);\n    \
             }\n\
         }\n",
    );
    let output = run(&dir, &["run"]);
    assert_ne!(
        code(&output),
        0,
        "an i32 overflow must abort, not silently wrap\nstdout: {}\nstderr: {}",
        stdout(&output),
        stderr(&output)
    );
    assert_eq!(
        stdout(&output),
        "",
        "the process must abort before reaching the `if`, so nothing should print"
    );
    assert!(
        stderr(&output).contains("arithmetic overflow"),
        "the abort should be reported by name: {}",
        stderr(&output)
    );
}

/// Item 2: real arithmetic (`1 + 2`) produces the right runtime value. There is no
/// `Display`/formatting yet, so the result is made observable by branching on a comparison
/// against the expected value and writing one of two distinct literal strings -- a subtly wrong
/// codegen lowering of `+` (e.g. swapped operands, wrong width) would produce `wrong-sum`
/// instead of `computed-3`, and this test would catch it byte-for-byte.
#[test]
fn addition_computes_the_correct_value() {
    let dir = scratch("addition_correct");
    write_manifest(&dir, "addition_correct");
    write_main(
        &dir,
        "module app;\n\n\
         fun main() {\n    \
             let a: i32 = 1;\n    \
             let b: i32 = 2;\n    \
             let c = a + b;\n    \
             if c == 3 {\n        \
                 core::io::write_bytes(1, \"computed-3\" as &[u8]);\n    \
             } else {\n        \
                 core::io::write_bytes(1, \"wrong-sum\" as &[u8]);\n    \
             }\n\
         }\n",
    );
    let output = run(&dir, &["run"]);
    assert_eq!(
        code(&output),
        0,
        "stdout: {}\nstderr: {}",
        stdout(&output),
        stderr(&output)
    );
    assert_eq!(stdout(&output), "computed-3");
}

/// Item 3: `if`/`else` selects the branch a runtime comparison actually calls for, not just
/// whichever branch codegen happens to emit first. `5 > 10` is false, so the correct run takes
/// the `else` arm; a codegen bug that inverted the branch condition (or a `br` that jumped to
/// the wrong block) would make this print `then-branch` instead.
#[test]
fn if_else_selects_the_correct_branch() {
    let dir = scratch("if_else_branch");
    write_manifest(&dir, "if_else_branch");
    write_main(
        &dir,
        "module app;\n\n\
         fun main() {\n    \
             let x: i32 = 5;\n    \
             let y: i32 = 10;\n    \
             if x > y {\n        \
                 core::io::write_bytes(1, \"then-branch\" as &[u8]);\n    \
             } else {\n        \
                 core::io::write_bytes(1, \"else-branch\" as &[u8]);\n    \
             }\n\
         }\n",
    );
    let output = run(&dir, &["run"]);
    assert_eq!(
        code(&output),
        0,
        "stdout: {}\nstderr: {}",
        stdout(&output),
        stderr(&output)
    );
    assert_eq!(stdout(&output), "else-branch");
}

/// Item 4: a struct is constructed with field values and a field read back out matches what was
/// written in. `p.x` reads back through the aggregate `Point { x: 7, y: 9 }` that was just
/// built; a codegen bug that mixed up field offsets (e.g. reading `y`'s slot for `x`, or padding
/// the aggregate wrong) would make the comparison fail and print `field-bad` instead.
#[test]
fn struct_field_is_read_back_after_construction() {
    let dir = scratch("struct_field_readback");
    write_manifest(&dir, "struct_field_readback");
    write_main(
        &dir,
        "module app;\n\n\
         struct Point {\n    \
             x: i32,\n    \
             y: i32,\n\
         }\n\n\
         fun main() {\n    \
             let p = Point { x: 7, y: 9 };\n    \
             if p.x == 7 {\n        \
                 core::io::write_bytes(1, \"field-ok\" as &[u8]);\n    \
             } else {\n        \
                 core::io::write_bytes(1, \"field-bad\" as &[u8]);\n    \
             }\n\
         }\n",
    );
    let output = run(&dir, &["run"]);
    assert_eq!(
        code(&output),
        0,
        "stdout: {}\nstderr: {}",
        stdout(&output),
        stderr(&output)
    );
    assert_eq!(stdout(&output), "field-ok");
}
