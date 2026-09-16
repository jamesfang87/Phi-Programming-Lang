//! Shared helpers for the integration tests, driven by the real `phi` binary.
//!
//! Every helper runs `phi` in a throwaway project under `target/test-scratch/`, so tests
//! exercise the whole pipeline (lex, parse, name resolution, type check, MIR, codegen, link,
//! execute) rather than a single stage. Included by each `tests/*.rs` binary via `mod support;`.

#![allow(dead_code)]

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

/// The observable result of one `phi` invocation.
pub struct Run {
    pub code: i32,
    pub stdout: String,
    pub stderr: String,
}

impl Run {
    pub fn ok(&self) -> bool {
        self.code == 0
    }
}

/// Creates a fresh, empty directory under `target/test-scratch/<category>/<name>`.
///
/// The directory is removed first so a rerun never sees a previous run's artifacts, and
/// the `category` prefix keeps separate test binaries from colliding.
pub fn scratch(category: &str, name: &str) -> PathBuf {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("target/test-scratch")
        .join(category)
        .join(name);
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).expect("could not create the scratch directory");
    dir
}

/// Writes a minimal `Phi.toml` naming `project_name` into `dir`.
pub fn write_manifest(dir: &Path, project_name: &str) {
    fs::write(
        dir.join("Phi.toml"),
        format!("[project]\nname = \"{project_name}\"\nversion = \"0.1.0\"\nedition = \"2026\"\n"),
    )
    .expect("could not write Phi.toml");
}

/// Writes a minimal `Phi.toml` naming `project_name` with `mode = "release"` into `dir`.
pub fn write_release_manifest(dir: &Path, project_name: &str) {
    fs::write(
        dir.join("Phi.toml"),
        format!(
            "[project]\nname = \"{project_name}\"\nversion = \"0.1.0\"\nedition = \"2026\"\n\
             [profile]\nmode = \"release\"\n"
        ),
    )
    .expect("could not write Phi.toml");
}

/// Writes `contents` to `dir/rel`, creating any parent directories.
pub fn write_file(dir: &Path, rel: &str, contents: &str) {
    let path = dir.join(rel);
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).expect("could not create the source directory");
    }
    fs::write(path, contents).expect("could not write a source file");
}

/// Sets up a scratch project whose `src/main.phi` is `source` and returns its directory.
pub fn project(category: &str, name: &str, source: &str) -> PathBuf {
    let dir = scratch(category, name);
    write_manifest(&dir, name);
    write_file(&dir, "src/main.phi", source);
    dir
}

/// Runs `phi <args>` with `dir` as the working directory.
pub fn exec(dir: &Path, args: &[&str]) -> Run {
    let output = Command::new(env!("CARGO_BIN_EXE_phi"))
        .args(args)
        .current_dir(dir)
        .output()
        .expect("failed to run the `phi` binary");
    Run {
        code: output.status.code().unwrap_or(-1),
        stdout: String::from_utf8_lossy(&output.stdout).into_owned(),
        stderr: String::from_utf8_lossy(&output.stderr).into_owned(),
    }
}

/// Builds and runs a one-file project whose `src/main.phi` is `source`.
pub fn run_src(category: &str, name: &str, source: &str) -> Run {
    run_files(category, name, &[("src/main.phi", source)])
}

/// Builds and runs a multi-file project. `files` are paths relative to the project root,
/// so a caller writes `("src/main.phi", ...)` and `("src/math.phi", ...)`.
pub fn run_files(category: &str, name: &str, files: &[(&str, &str)]) -> Run {
    let dir = scratch(category, name);
    write_manifest(&dir, name);
    for (rel, contents) in files {
        write_file(&dir, rel, contents);
    }
    exec(&dir, &["run"])
}

/// Type-checks (but does not build) a one-file project.
pub fn check_src(category: &str, name: &str, source: &str) -> Run {
    check_files(category, name, &[("src/main.phi", source)])
}

/// Type-checks a multi-file project.
pub fn check_files(category: &str, name: &str, files: &[(&str, &str)]) -> Run {
    let dir = scratch(category, name);
    write_manifest(&dir, name);
    for (rel, contents) in files {
        write_file(&dir, rel, contents);
    }
    exec(&dir, &["check"])
}

/// Asserts a run succeeded, reporting both streams on failure.
pub fn expect_ok(run: &Run, context: &str) {
    assert!(
        run.ok(),
        "{context}: expected a successful run, got exit {}\nstdout: {}\nstderr: {}",
        run.code,
        run.stdout,
        run.stderr
    );
}

/// Asserts a run succeeded and wrote exactly `expected` to stdout.
pub fn expect_stdout(run: &Run, expected: &str, context: &str) {
    expect_ok(run, context);
    assert_eq!(
        run.stdout, expected,
        "{context}: stdout mismatch\nstderr: {}",
        run.stderr
    );
}

/// Asserts the compiler reported a diagnostic rather than crashing, without requiring a
/// particular message or exit code.
pub fn expect_no_panic(run: &Run, context: &str) {
    assert!(
        !run.stderr.contains("panicked"),
        "{context}: the compiler panicked instead of reporting a diagnostic\n{}",
        run.stderr
    );
}

/// Asserts the build/check failed cleanly (nonzero exit) with a diagnostic mentioning
/// `needle`, and did not crash the compiler.
pub fn expect_reject(run: &Run, needle: &str, context: &str) {
    expect_no_panic(run, context);
    assert_ne!(
        run.code, 0,
        "{context}: expected an error, but the compiler exited 0\nstdout: {}",
        run.stdout
    );
    assert!(
        run.stderr.contains(needle),
        "{context}: expected a diagnostic mentioning {needle:?}\nstderr: {}",
        run.stderr
    );
}

/// Asserts the program compiled but aborted at runtime, naming `needle` on stderr.
pub fn expect_abort(run: &Run, needle: &str, context: &str) {
    assert_ne!(
        run.code, 0,
        "{context}: expected a runtime abort, but the process exited 0\nstdout: {}",
        run.stdout
    );
    assert!(
        run.stderr.contains(needle),
        "{context}: expected the abort to mention {needle:?}\nstderr: {}",
        run.stderr
    );
}

/// Wraps a condition into a check statement that writes `;<idx>;` to stdout when the
/// condition is *false*. A generated program of these should produce empty stdout.
pub fn check(idx: usize, condition: &str) -> String {
    format!("if !({condition}) {{ core::io::write_bytes(1, \";{idx};\" as &[u8]); }}")
}

/// The most checks a single generated program may contain. Larger batteries are sampled evenly;
/// every case is still computed, just not every one is compiled into one enormous function.
const MAX_CHECKS: usize = 400;

fn evenly_sample(checks: &[String]) -> Vec<String> {
    if checks.len() <= MAX_CHECKS {
        return checks.to_vec();
    }
    (0..MAX_CHECKS)
        .map(|i| checks[i * (checks.len() - 1) / (MAX_CHECKS - 1)].clone())
        .collect()
}

/// Builds a program from top-level `items` plus a `main` whose body is `checks`, runs it,
/// and asserts every check passed (empty stdout). On failure the concatenated `;N;` markers
/// name the exact checks that failed.
pub fn run_checks(category: &str, name: &str, items: &str, checks: &[String]) -> Run {
    assert!(!checks.is_empty(), "{name}: no checks generated");
    let checks = evenly_sample(checks);
    let mut source = String::from("module app;\n\n");
    if !items.is_empty() {
        source.push_str(items);
        source.push('\n');
    }
    source.push_str("fun main() {\n");
    for statement in &checks {
        source.push_str("    ");
        source.push_str(statement);
        source.push('\n');
    }
    source.push_str("}\n");

    let run = run_src(category, name, &source);
    assert!(
        run.ok(),
        "{name}: generated program failed to build\nexit: {}\nstdout: {}\nstderr: {}\nsource:\n{source}",
        run.code,
        run.stdout,
        run.stderr
    );
    assert!(
        run.stdout.is_empty(),
        "{name}: {} check(s) failed; failing markers: {:?}\nstderr: {}\nsource:\n{source}",
        run.stdout.matches(';').count() / 2,
        run.stdout,
        run.stderr
    );
    run
}
