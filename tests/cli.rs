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

const CLEAN_MAIN: &str = "module clean;\n\nfun main() {\n}\n";
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

/// `core::ops::Copy` is implemented for every scalar primitive via a plain `*self` read, not
/// just derived implicitly -- calling `.copy()` explicitly exercises the real trait method
/// (`extend i32 with Copy { .. }` in `lib/core/ops.phi`), not merely `i32`'s ordinary by-value
/// semantics, and confirms it returns a value equal to the original.
#[test]
fn calling_copy_on_a_primitive_returns_an_equal_value() {
    let dir = scratch("copy_on_primitive");
    write_manifest(&dir, "copy_on_primitive");
    write_main(
        &dir,
        "module app;\n\n\
         fun main() {\n    \
             let a: i32 = 42;\n    \
             let b = a.copy();\n    \
             if b == 42 {\n        \
                 core::io::write_bytes(1, \"copy-ok\" as &[u8]);\n    \
             } else {\n        \
                 core::io::write_bytes(1, \"copy-bad\" as &[u8]);\n    \
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
    assert_eq!(stdout(&output), "copy-ok");
}

/// `str` shares `&[u8]`'s representation (a ptr+len wide pointer, not owned data -- see
/// `TyCtx::contains_ref`'s doc comment), so it gets `Copy` the same direct way a scalar
/// primitive does: duplicating the wide pointer is exactly `*self`, no special-casing needed.
#[test]
fn calling_copy_on_a_str_returns_an_equal_value() {
    let dir = scratch("copy_on_str");
    write_manifest(&dir, "copy_on_str");
    write_main(
        &dir,
        "module app;\n\n\
         fun main() {\n    \
             let a: str = \"hello\";\n    \
             let b = a.copy();\n    \
             core::io::write_bytes(1, b as &[u8]);\n\
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
    assert_eq!(stdout(&output), "hello");
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

/// A method from a *generic* `extend` block taken by `&self`. The receiver temp the call lowers
/// to used to be typed from the method's declared `&self` (`&Wrap<T>`, the block's parameter,
/// unsubstituted), which put a generic into `main`'s own locals; monomorphization seeds its roots
/// with the bodies mentioning no generic, so `main` was dropped from the program and the build
/// died at `ld` with an undefined `_main`. Exercises `&self`, `&mut self`, and a receiver reached
/// through a reference, since only the by-value `self` path was ever correct.
#[test]
fn a_method_from_a_generic_extend_block_is_reachable_by_reference() {
    let dir = scratch("generic_extend_receiver");
    write_manifest(&dir, "generic_extend_receiver");
    write_main(
        &dir,
        "module app;\n\n\
         struct Wrap<T> {\n    \
             value: T,\n\
         }\n\n\
         extend<T> Wrap<T> {\n    \
             fun ping(&self) -> i32 { return 3; }\n    \
             fun bump(&mut self) -> i32 { return 5; }\n\
         }\n\n\
         fun main() {\n    \
             let mut w: Wrap<i32> = Wrap { value: 100 };\n    \
             let r = &w;\n    \
             let total = r.ping() + w.ping() + w.bump();\n    \
             if total == 11 {\n        \
                 core::io::write_bytes(1, \"generic-ok\" as &[u8]);\n    \
             } else {\n        \
                 core::io::write_bytes(1, \"generic-bad\" as &[u8]);\n    \
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
    assert_eq!(stdout(&output), "generic-ok");
}

/// A method from a `extend Pair<i32, i32>` reached on a receiver whose generic arguments are
/// still integer literals. At the call the receiver is `Pair<{integer}, {integer}>`, which the
/// concrete header cannot be matched against, so the call is parked and answered once the
/// literals default. Runs the result to prove the parked call lowers and computes correctly,
/// not merely that it type checks.
#[test]
fn a_method_on_a_concrete_extend_is_found_through_literal_defaulting() {
    let dir = scratch("literal_defaulting_method");
    write_manifest(&dir, "literal_defaulting_method");
    write_main(
        &dir,
        "module app;\n\n\
         struct Pair<A, B> {\n    \
             first: A,\n    \
             second: B,\n\
         }\n\n\
         extend Pair<i32, i32> {\n    \
             fun sum(&self) -> i32 { return self.first + self.second; }\n\
         }\n\n\
         fun main() {\n    \
             let p = Pair { first: 4, second: 6 };\n    \
             if p.sum() == 10 {\n        \
                 core::io::write_bytes(1, \"sum-ok\" as &[u8]);\n    \
             } else {\n        \
                 core::io::write_bytes(1, \"sum-bad\" as &[u8]);\n    \
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
    assert_eq!(stdout(&output), "sum-ok");
}

/// A crate with no `main` builds and runs; it just does nothing. The missing entry point is a
/// warning, and codegen emits a `main` that returns 0, so the link still produces an executable
/// rather than failing at `ld` with an undefined `_main`.
#[test]
fn a_crate_with_no_main_builds_and_runs_doing_nothing() {
    let dir = scratch("no_main");
    write_manifest(&dir, "no_main");
    write_main(&dir, "module app;\n\nfun helper() -> i32 { return 1; }\n");

    let build = run(&dir, &["build"]);
    let err = stderr(&build);
    assert_eq!(code(&build), 0, "build should succeed: {err}");
    assert!(
        err.contains("Warning") && err.contains("no `main` function found"),
        "expected a missing-entry-point warning: {err}"
    );
    assert!(
        !err.contains("Undefined symbols") && !err.contains("symbol(s) not found"),
        "the link should still produce an executable: {err}"
    );

    // Runnable, and observably does nothing.
    let output = run(&dir, &["run"]);
    assert_eq!(code(&output), 0, "stderr: {}", stderr(&output));
    assert_eq!(stdout(&output), "");
}

/// A variant named through its enum (`Shape.rect { .. }`) rather than left to the expected type
/// (`.rect { .. }`). All three payload shapes go through codegen and are read back by matching
/// on them, so a wrong variant index or a record payload built in the written rather than the
/// declared field order would print `variant-bad`.
#[test]
fn a_variant_named_through_its_enum_is_built_and_matched() {
    let dir = scratch("qualified_variant");
    write_manifest(&dir, "qualified_variant");
    write_main(
        &dir,
        "module app;\n\n\
         enum Shape {\n    \
             empty,\n    \
             circle: i32,\n    \
             rect: { w: i32, h: i32 },\n\
         }\n\n\
         fun code(s: Shape) -> i32 {\n    \
             match s {\n        \
                 .empty => { return 1; }\n        \
                 .circle(r) => { return r; }\n        \
                 .rect { w, h } => { return w * h; }\n    \
             }\n\
         }\n\n\
         fun main() {\n    \
             let total = code(Shape.rect { w: 4, h: 5 })\n        \
                 + code(Shape.circle(7))\n        \
                 + code(Shape.empty);\n    \
             if total == 28 {\n        \
                 core::io::write_bytes(1, \"variant-ok\" as &[u8]);\n    \
             } else {\n        \
                 core::io::write_bytes(1, \"variant-bad\" as &[u8]);\n    \
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
    assert_eq!(stdout(&output), "variant-ok");
}

/// A reference to a reference (`&&i32`): `r2` borrows `r1`, which itself borrows `a`, and
/// `**r2` peels both layers back to `a`'s value. A codegen or typeck bug that mishandled the
/// second layer of indirection (e.g. treating `r2`'s pointee as `i32` instead of `&i32`) would
/// read garbage through the first `*` and print `double-deref-bad` instead.
#[test]
fn double_deref_through_a_reference_to_a_reference_reads_the_original_value() {
    let dir = scratch("double_deref");
    write_manifest(&dir, "double_deref");
    write_main(
        &dir,
        "module app;\n\n\
         fun main() {\n    \
             let a: i32 = 42;\n    \
             let r1: &i32 = &a;\n    \
             let r2: &&i32 = &r1;\n    \
             if **r2 == 42 {\n        \
                 core::io::write_bytes(1, \"double-deref-ok\" as &[u8]);\n    \
             } else {\n        \
                 core::io::write_bytes(1, \"double-deref-bad\" as &[u8]);\n    \
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
    assert_eq!(stdout(&output), "double-deref-ok");
}

#[test]
fn a_generic_extend_method_builds_and_runs() {
    let dir = scratch("generic_extend_method");
    write_manifest(&dir, "generic_extend_method");
    write_main(
        &dir,
        "module app;\n\
         \n\
         struct Wrap<T> { public value: T }\n\
         \n\
         extend<T> Wrap<T> {\n\
             public fun get(self) -> T { return self.value; }\n\
         }\n\
         \n\
         fun main() {\n\
             let w: Wrap<i32> = Wrap { value: 7 };\n\
             let n = w.get();\n\
             if n == 7 { core::io::write_bytes(1, \"ok\" as &[u8]); }\n\
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
    assert_eq!(stdout(&output), "ok");
}

#[test]
fn a_method_returning_self_builds_and_runs() {
    let dir = scratch("method_returning_self");
    write_manifest(&dir, "method_returning_self");
    write_main(
        &dir,
        "module app;\n\
         \n\
         struct Wrap<T> { public value: T }\n\
         \n\
         extend<T> Wrap<T> {\n\
             public fun same(self) -> Self { return self; }\n\
             public fun get(self) -> T { return self.value; }\n\
         }\n\
         \n\
         fun main() {\n\
             let w: Wrap<i32> = Wrap { value: 9 };\n\
             let n = w.same().get();\n\
             if n == 9 { core::io::write_bytes(1, \"ok\" as &[u8]); }\n\
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
    assert_eq!(stdout(&output), "ok");
}

#[test]
fn a_reference_match_builds_and_runs() {
    let dir = scratch("reference_match");
    write_manifest(&dir, "reference_match");
    write_main(
        &dir,
        "module app;\n\
         \n\
         enum Opt { some: i32, none }\n\
         \n\
         fun peek(o: &Opt) -> i32 {\n\
             return match o { .some(v) => *v, .none => 0, };\n\
         }\n\
         \n\
         fun main() {\n\
             let a: Opt = .some(5);\n\
             let b: Opt = .none;\n\
             if peek(&a) == 5 { core::io::write_bytes(1, \"a\" as &[u8]); }\n\
             if peek(&b) == 0 { core::io::write_bytes(1, \"b\" as &[u8]); }\n\
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
    assert_eq!(stdout(&output), "ab");
}

#[test]
fn a_reference_match_does_not_drop_what_it_borrows() {
    let dir = scratch("reference_match_drops");
    write_manifest(&dir, "reference_match_drops");
    write_main(
        &dir,
        "module app;\n\
         \n\
         enum Opt { some: i32, none }\n\
         \n\
         fun peek(o: &Opt) -> i32 {\n\
             return match o { .some(v) => *v, .none => 0, };\n\
         }\n\
         \n\
         fun main() {\n\
             let a: Opt = .some(3);\n\
             let first = peek(&a);\n\
             let second = peek(&a);\n\
             if first + second == 6 { core::io::write_bytes(1, \"ok\" as &[u8]); }\n\
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
    assert_eq!(stdout(&output), "ok");
}

#[test]
fn option_methods_build_and_run() {
    let dir = scratch("option_methods");
    write_manifest(&dir, "option_methods");
    write_main(
        &dir,
        "module app;\n\
         \n\
         fun main() {\n\
             let a: Option<i32> = .some(4);\n\
             let b: Option<i32> = .none;\n\
             if a.is_some() { core::io::write_bytes(1, \"1\" as &[u8]); }\n\
             if b.is_none() { core::io::write_bytes(1, \"2\" as &[u8]); }\n\
             let doubled: Option<i32> = .some(4);\n\
             if doubled.map(|x| x * 2).unwrap() == 8 { core::io::write_bytes(1, \"3\" as &[u8]); }\n\
             let empty: Option<i32> = .none;\n\
             if empty.unwrap_or(7) == 7 { core::io::write_bytes(1, \"4\" as &[u8]); }\n\
             let c: Option<i32> = .some(1);\n\
             let r: Result<i32, bool> = c.ok_or(false);\n\
             if r.unwrap() == 1 { core::io::write_bytes(1, \"5\" as &[u8]); }\n\
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
    assert_eq!(stdout(&output), "12345");
}

#[test]
fn result_methods_build_and_run() {
    let dir = scratch("result_methods");
    write_manifest(&dir, "result_methods");
    write_main(
        &dir,
        "module app;\n\
         \n\
         fun main() {\n\
             let a: Result<i32, bool> = .ok(4);\n\
             let b: Result<i32, bool> = .err(true);\n\
             if a.is_ok() { core::io::write_bytes(1, \"1\" as &[u8]); }\n\
             if b.is_err() { core::io::write_bytes(1, \"2\" as &[u8]); }\n\
             let c: Result<i32, bool> = .ok(4);\n\
             if c.map(|x| x * 2).unwrap() == 8 { core::io::write_bytes(1, \"3\" as &[u8]); }\n\
             let d: Result<i32, bool> = .err(true);\n\
             if d.unwrap_or(7) == 7 { core::io::write_bytes(1, \"4\" as &[u8]); }\n\
             let e: Result<i32, bool> = .ok(1);\n\
             if e.ok().unwrap() == 1 { core::io::write_bytes(1, \"5\" as &[u8]); }\n\
             let f: Result<i32, bool> = .err(true);\n\
             if f.map_err(|x| if x { 9 } else { 8 }).unwrap_or(0) == 0 { core::io::write_bytes(1, \"6\" as &[u8]); }\n\
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
    assert_eq!(stdout(&output), "123456");
}

#[test]
fn a_closure_that_ignores_its_parameter_builds_and_runs() {
    let dir = scratch("closure_ignores_parameter");
    write_manifest(&dir, "closure_ignores_parameter");
    write_main(
        &dir,
        "module app;\n\
         \n\
         fun conv<A, B>(x: A, f: fun(A) -> B) -> B {\n\
             return f(x);\n\
         }\n\
         \n\
         fun main() {\n\
             let n: i32 = conv(true, |x| 9);\n\
             if n == 9 { core::io::write_bytes(1, \"a\" as &[u8]); }\n\
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
    assert_eq!(stdout(&output), "a");
}

#[test]
fn option_map_with_an_unused_closure_parameter_builds_and_runs() {
    let dir = scratch("option_map_unused_parameter");
    write_manifest(&dir, "option_map_unused_parameter");
    write_main(
        &dir,
        "module app;\n\
         \n\
         fun main() {\n\
             let o: Option<bool> = .some(true);\n\
             if o.map(|x| 9).unwrap_or(0) == 9 { core::io::write_bytes(1, \"9\" as &[u8]); }\n\
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
    assert_eq!(stdout(&output), "9");
}

#[test]
fn a_trait_default_method_dispatches_to_the_implementing_type() {
    let dir = scratch("trait_default_dispatch");
    write_manifest(&dir, "trait_default_dispatch");
    write_main(
        &dir,
        "module app;\n\
         \n\
         trait Cloner {\n\
             fun clone_it(&self) -> Self;\n\
             fun twice(&self) -> Self { return self.clone_it(); }\n\
         }\n\
         \n\
         struct N { public value: i32 }\n\
         struct M { public value: i32 }\n\
         \n\
         extend N with Cloner {\n\
             fun clone_it(&self) -> Self { return N { value: self.value + 1 }; }\n\
         }\n\
         \n\
         extend M with Cloner {\n\
             fun clone_it(&self) -> Self { return M { value: self.value + 2 }; }\n\
         }\n\
         \n\
         fun main() {\n\
             let n: N = N { value: 1 };\n\
             let m: M = M { value: 1 };\n\
             let n2: N = n.twice();\n\
             let m2: M = m.twice();\n\
             if n2.value == 2 { core::io::write_bytes(1, \"1\" as &[u8]); }\n\
             if m2.value == 3 { core::io::write_bytes(1, \"2\" as &[u8]); }\n\
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
    assert_eq!(stdout(&output), "12");
}

#[test]
fn a_generic_trait_default_method_substitutes_the_traits_arguments() {
    let dir = scratch("generic_trait_default");
    write_manifest(&dir, "generic_trait_default");
    write_main(
        &dir,
        "module app;\n\
         \n\
         trait Sh<T> {\n\
             fun sh(&self) -> T;\n\
             fun go(&self) -> T { return self.sh(); }\n\
         }\n\
         \n\
         struct W { public value: i32 }\n\
         \n\
         extend W with Sh<i32> {\n\
             fun sh(&self) -> i32 { return self.value; }\n\
         }\n\
         \n\
         fun main() {\n\
             let w: W = W { value: 5 };\n\
             let n: i32 = w.go();\n\
             if n == 5 { core::io::write_bytes(1, \"ok\" as &[u8]); }\n\
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
    assert_eq!(stdout(&output), "ok");
}

#[test]
fn a_bound_with_trait_arguments_dispatches_end_to_end() {
    let dir = scratch("bound_with_arguments");
    write_manifest(&dir, "bound_with_arguments");
    write_main(
        &dir,
        "module app;\n\
         \n\
         trait Conv<From> {\n\
             fun conv(&self) -> From;\n\
             fun boxed(&self) -> From { return self.conv(); }\n\
         }\n\
         \n\
         struct W { public v: i32 }\n\
         \n\
         extend W with Conv<i32> {\n\
             fun conv(&self) -> i32 { return self.v; }\n\
         }\n\
         \n\
         fun call_boxed<T: Conv<i32>>(x: &T) -> i32 {\n\
             return x.boxed();\n\
         }\n\
         \n\
         fun main() {\n\
             let w: W = W { v: 5 };\n\
             let n: i32 = call_boxed(&w);\n\
             if n == 5 { core::io::write_bytes(1, \"ok\" as &[u8]); }\n\
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
    assert_eq!(stdout(&output), "ok");
}

#[test]
fn dyn_dispatch_runs_each_implementing_type() {
    let dir = scratch("dyn_dispatch");
    write_manifest(&dir, "dyn_dispatch");
    write_main(
        &dir,
        "module app;\n\
         \n\
         trait Sh {\n\
             fun sh(&self) -> i32;\n\
         }\n\
         \n\
         struct W { public v: i32 }\n\
         struct V { public v: i32 }\n\
         \n\
         extend W with Sh {\n\
             fun sh(&self) -> i32 { return self.v; }\n\
         }\n\
         \n\
         extend V with Sh {\n\
             fun sh(&self) -> i32 { return self.v * 10; }\n\
         }\n\
         \n\
         fun draw(s: &dyn Sh) -> i32 {\n\
             return s.sh();\n\
         }\n\
         \n\
         fun main() {\n\
             let w: W = W { v: 7 };\n\
             let v: V = V { v: 3 };\n\
             let a: i32 = draw(&w);\n\
             let b: i32 = draw(&v);\n\
             let d: &dyn Sh = &w;\n\
             let c: i32 = draw(d);\n\
             if a == 7 { core::io::write_bytes(1, \"1\" as &[u8]); }\n\
             if b == 30 { core::io::write_bytes(1, \"2\" as &[u8]); }\n\
             if c == 7 { core::io::write_bytes(1, \"3\" as &[u8]); }\n\
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
    assert_eq!(stdout(&output), "123");
}

#[test]
fn a_dyn_return_and_a_mutable_dyn_dispatch() {
    let dir = scratch("dyn_return_and_mut");
    write_manifest(&dir, "dyn_return_and_mut");
    write_main(
        &dir,
        "module app;\n\
         \n\
         trait Counter {\n\
             fun count(&self) -> i32;\n\
         }\n\
         \n\
         struct N { public n: i32 }\n\
         \n\
         extend N with Counter {\n\
             fun count(&self) -> i32 { return self.n; }\n\
         }\n\
         \n\
         fun pick(fresh: bool, n: &N) -> &dyn Counter {\n\
             if fresh { return n; }\n\
             return n;\n\
         }\n\
         \n\
         fun bump(c: &mut dyn Counter) -> i32 {\n\
             return c.count();\n\
         }\n\
         \n\
         fun main() {\n\
             let mut n: N = N { n: 42 };\n\
             let c: &dyn Counter = pick(true, &n);\n\
             let a: i32 = c.count();\n\
             let b: i32 = bump(&mut n);\n\
             if a == 42 { core::io::write_bytes(1, \"1\" as &[u8]); }\n\
             if b == 42 { core::io::write_bytes(1, \"2\" as &[u8]); }\n\
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
    assert_eq!(stdout(&output), "12");
}
