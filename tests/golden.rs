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
//!
//! A fixture known to fail because of a real, out-of-scope compiler bug belongs in
//! [`QUARANTINED`] instead of being re-blessed: blessing a `todo!()` panic's backtrace as
//! "expected" would hide the bug rather than track it. A quarantined fixture's mismatch does not
//! fail this test, but if it ever starts matching again the test fails and says to remove it
//! from the list -- otherwise the quarantine would silently become permanent once someone fixes
//! the underlying bug.
//!
//! `PHI_BLESS=1` skips quarantined fixtures rather than rewriting them, so re-blessing to update
//! one fixture cannot quietly capture another's crash as its expected output.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

fn fixtures_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures")
}

/// Fixtures whose `expected.txt` is known not to match the compiler's current output, and why.
///
/// A quarantined fixture's mismatch is tolerated rather than blessed, because blessing a
/// `todo!()` panic's backtrace as "expected" would hide the bug instead of documenting it. See
/// [`golden_fixtures`] for what happens once a quarantined fixture starts matching again.
const QUARANTINED: &[(&str, &str)] = &[(
    "core_library",
    "its `map.phi` contains `&self.value`, which hits `todo!(\"check_expr: Borrow\")` at \
     src/typeck.rs:553",
)];

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
        output
            .status
            .code()
            .map_or("<none>".to_string(), |c| c.to_string()),
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
    assert!(
        !fixtures.is_empty(),
        "no fixtures found under tests/fixtures"
    );

    for fixture_dir in fixtures {
        let name = fixture_dir
            .file_name()
            .unwrap()
            .to_string_lossy()
            .into_owned();
        let expected_path = fixture_dir.join("expected.txt");

        let quarantine_reason = QUARANTINED
            .iter()
            .find(|(fixture, _)| *fixture == name)
            .map(|(_, reason)| *reason);

        // Blessing is checked against the quarantine *before* anything is written. A quarantined
        // fixture's current output is the very thing the quarantine exists to keep out of
        // `expected.txt` -- so a routine re-bless, run to update some unrelated fixture, must not
        // silently capture this one's `todo!()` backtrace and call it expected.
        if bless {
            if let Some(reason) = quarantine_reason {
                println!("skipped quarantined fixture {name} ({reason})");
                continue;
            }
            let actual = run_fixture(&fixture_dir);
            fs::write(&expected_path, &actual)
                .unwrap_or_else(|e| panic!("failed to write {}: {e}", expected_path.display()));
            println!("blessed {name}");
            continue;
        }

        let actual = run_fixture(&fixture_dir);
        let expected = fs::read_to_string(&expected_path).unwrap_or_else(|e| {
            panic!(
                "failed to read {} ({e}) — run `PHI_BLESS=1 cargo test --test golden` to generate it",
                expected_path.display()
            )
        });

        match (actual == expected, quarantine_reason) {
            // Strictly enforced fixture, and it matched: nothing to do.
            (true, None) => {}
            (true, Some(reason)) => failures.push(format!(
                "fixture `{name}` is quarantined ({reason}) but now matches {} -- \
                 remove it from QUARANTINED in tests/golden.rs",
                expected_path.display()
            )),
            // Quarantined fixture, still mismatching as expected: tolerated.
            (false, Some(_)) => {}
            (false, None) => failures.push(format!(
                "fixture `{name}` did not match {}\n--- expected ---\n{expected}\n--- actual ---\n{actual}",
                expected_path.display()
            )),
        }
    }

    assert!(
        failures.is_empty(),
        "{} fixture(s) mismatched:\n\n{}",
        failures.len(),
        failures.join("\n\n"),
    );
}
