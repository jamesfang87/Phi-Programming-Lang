#![allow(dead_code)]

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

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

pub fn scratch(category: &str, name: &str) -> PathBuf {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("target/test-scratch")
        .join(category)
        .join(name);
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).expect("could not create the scratch directory");
    dir
}

pub fn write_manifest(dir: &Path, project_name: &str) {
    fs::write(
        dir.join("Phi.toml"),
        format!("[project]\nname = \"{project_name}\"\nversion = \"0.1.0\"\nedition = \"2026\"\n"),
    )
    .expect("could not write Phi.toml");
}

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

pub fn write_file(dir: &Path, rel: &str, contents: &str) {
    let path = dir.join(rel);
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).expect("could not create the source directory");
    }
    fs::write(path, contents).expect("could not write a source file");
}

pub fn project(category: &str, name: &str, source: &str) -> PathBuf {
    let dir = scratch(category, name);
    write_manifest(&dir, name);
    write_file(&dir, "src/main.phi", source);
    dir
}

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

pub fn run_src(category: &str, name: &str, source: &str) -> Run {
    run_files(category, name, &[("src/main.phi", source)])
}

pub fn run_files(category: &str, name: &str, files: &[(&str, &str)]) -> Run {
    let dir = scratch(category, name);
    write_manifest(&dir, name);
    for (rel, contents) in files {
        write_file(&dir, rel, contents);
    }
    exec(&dir, &["run"])
}

pub fn check_src(category: &str, name: &str, source: &str) -> Run {
    check_files(category, name, &[("src/main.phi", source)])
}

pub fn check_files(category: &str, name: &str, files: &[(&str, &str)]) -> Run {
    let dir = scratch(category, name);
    write_manifest(&dir, name);
    for (rel, contents) in files {
        write_file(&dir, rel, contents);
    }
    exec(&dir, &["check"])
}

pub fn expect_ok(run: &Run, context: &str) {
    assert!(
        run.ok(),
        "{context}: expected a successful run, got exit {}\nstdout: {}\nstderr: {}",
        run.code,
        run.stdout,
        run.stderr
    );
}

pub fn expect_stdout(run: &Run, expected: &str, context: &str) {
    expect_ok(run, context);
    assert_eq!(
        run.stdout, expected,
        "{context}: stdout mismatch\nstderr: {}",
        run.stderr
    );
}

pub fn expect_no_panic(run: &Run, context: &str) {
    assert!(
        !run.stderr.contains("panicked"),
        "{context}: the compiler panicked instead of reporting a diagnostic\n{}",
        run.stderr
    );
}

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

pub fn check(idx: usize, condition: &str) -> String {
    format!("if !({condition}) {{ core::io::write_bytes(1, \";{idx};\" as &[u8]); }}")
}

const MAX_CHECKS: usize = 400;

fn evenly_sample(checks: &[String]) -> Vec<String> {
    if checks.len() <= MAX_CHECKS {
        return checks.to_vec();
    }
    (0..MAX_CHECKS)
        .map(|i| checks[i * (checks.len() - 1) / (MAX_CHECKS - 1)].clone())
        .collect()
}

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

fn build_ast_dump(fixture_dir: &Path) -> String {
    let output = Command::new(env!("CARGO_BIN_EXE_phi"))
        .arg("build")
        .arg("--ast")
        .current_dir(fixture_dir)
        .output()
        .expect("failed to run the `phi` binary");

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);

    format!(
        "=== status ===\n{}\n=== stdout ===\n{}=== stderr ===\n{}",
        output
            .status
            .code()
            .map_or("<none>".to_string(), |c| c.to_string()),
        stdout,
        stderr,
    )
}

pub fn compare_golden_fixtures(fixtures_dir: &Path, test_target: &str) {
    let bless = std::env::var_os("PHI_BLESS").is_some();
    let mut failures = Vec::new();

    let mut fixtures: Vec<PathBuf> = fs::read_dir(fixtures_dir)
        .expect("the golden fixture directory should exist")
        .filter_map(|entry| {
            let entry = entry.expect("failed to read a golden fixture entry");
            entry.path().is_dir().then(|| entry.path())
        })
        .collect();
    fixtures.sort();
    assert!(
        !fixtures.is_empty(),
        "no fixtures found under {fixtures_dir:?}"
    );

    for fixture_dir in fixtures {
        let name = fixture_dir
            .file_name()
            .unwrap()
            .to_string_lossy()
            .into_owned();
        let expected_path = fixture_dir.join("expected.txt");

        let actual = build_ast_dump(&fixture_dir);

        if bless {
            fs::write(&expected_path, &actual)
                .unwrap_or_else(|e| panic!("failed to write {}: {e}", expected_path.display()));
            println!("blessed {name}");
            continue;
        }

        let expected = fs::read_to_string(&expected_path).unwrap_or_else(|e| {
            panic!(
                "failed to read {} ({e}) — run `PHI_BLESS=1 cargo test --test {test_target} golden` to generate it",
                expected_path.display()
            )
        });

        if actual != expected {
            failures.push(format!(
                "fixture `{name}` did not match {}\n--- expected ---\n{expected}\n--- actual ---\n{actual}",
                expected_path.display()
            ));
        }
    }

    assert!(
        failures.is_empty(),
        "{} fixture(s) mismatched:\n\n{}",
        failures.len(),
        failures.join("\n\n"),
    );
}
