use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

fn fixtures_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures")
}

const QUARANTINED: &[(&str, &str)] = &[(
    "core_library",
    "no compiler bug left -- it builds cleanly at exit 0. Its `expected.txt` is simply stale in \
     two deliberate ways: the AST dump gained `id: NodeId(..)` fields, and declaring no `fun \
     main` now warns on stderr. Re-bless once the AST dump format has settled",
)];

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
            (true, None) => {}
            (true, Some(reason)) => failures.push(format!(
                "fixture `{name}` is quarantined ({reason}) but now matches {} -- \
                 remove it from QUARANTINED in tests/golden.rs",
                expected_path.display()
            )),
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
