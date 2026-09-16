mod support;

use std::path::{Path, PathBuf};
use support::Run;

fn scratch(name: &str) -> PathBuf {
    support::scratch("cli", name)
}

fn write_manifest(dir: &Path, project_name: &str) {
    support::write_manifest(dir, project_name);
}

fn write_release_manifest(dir: &Path, project_name: &str) {
    support::write_release_manifest(dir, project_name);
}

fn write_main(dir: &Path, contents: &str) {
    support::write_file(dir, "src/main.phi", contents);
}

const CLEAN_MAIN: &str = "module clean;\n\nfun main() {\n}\n";
const TYPE_ERROR_MAIN: &str = "module broken;\n\nfun broken() -> bool {\n    return 1;\n}\n";

fn run(dir: &Path, args: &[&str]) -> Run {
    support::exec(dir, args)
}

fn stderr(run: &Run) -> String {
    run.stderr.clone()
}

fn stdout(run: &Run) -> String {
    run.stdout.clone()
}

fn code(run: &Run) -> i32 {
    run.code
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

    let output = run(&dir, &["run"]);
    assert_eq!(code(&output), 0, "stderr: {}", stderr(&output));
    assert_eq!(stdout(&output), "");
}

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
