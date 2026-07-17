//! Golden tests for the compiler CLI.
//!
//! Each subdirectory of `tests/fixtures/` is a tiny Phi "project": drop one or more `.phi`
//! files in it, run `phi build --ast` with it as the working directory, and compare the
//! combined exit status / stdout (the AST, when parsing produced one) / stderr (diagnostics)
//! against a committed `expected.txt`.
//!
//! To add a fixture: create `tests/fixtures/<name>/` with your `.phi` file(s), then run
//! `PHI_BLESS=1 cargo test --test golden` once to generate its `expected.txt`. Re-run the same
//! way (`PHI_BLESS=1 cargo test --test golden`) any time a fixture's expected output needs to
//! change on purpose — diff the result before committing it.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

fn fixtures_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures")
}

/// Runs `phi build --ast` with `fixture_dir` as the working directory and formats the result
/// (exit status, stdout, stderr) into one comparable string.
fn run_fixture(fixture_dir: &Path) -> String {
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
        output.status.code().map_or("<none>".to_string(), |c| c.to_string()),
        stdout,
        stderr,
    )
}

#[test]
fn golden_fixtures() {
    let bless = std::env::var_os("PHI_BLESS").is_some();
    let mut failures = Vec::new();

    let mut fixtures: Vec<PathBuf> = fs::read_dir(fixtures_dir())
        .expect("tests/fixtures should exist")
        .filter_map(|entry| {
            let entry = entry.expect("failed to read a tests/fixtures entry");
            entry.path().is_dir().then(|| entry.path())
        })
        .collect();
    fixtures.sort();
    assert!(!fixtures.is_empty(), "no fixtures found under tests/fixtures");

    for fixture_dir in fixtures {
        let name = fixture_dir
            .file_name()
            .unwrap()
            .to_string_lossy()
            .into_owned();
        let expected_path = fixture_dir.join("expected.txt");
        let actual = run_fixture(&fixture_dir);

        if bless {
            fs::write(&expected_path, &actual).unwrap_or_else(|e| {
                panic!("failed to write {}: {e}", expected_path.display())
            });
            println!("blessed {name}");
            continue;
        }

        let expected = fs::read_to_string(&expected_path).unwrap_or_else(|e| {
            panic!(
                "failed to read {} ({e}) — run `PHI_BLESS=1 cargo test --test golden` to generate it",
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
