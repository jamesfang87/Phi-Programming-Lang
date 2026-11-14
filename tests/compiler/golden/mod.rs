use std::path::{Path, PathBuf};

use crate::support;

fn fixtures_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/compiler/golden/fixtures")
}

#[test]
fn golden_fixtures() {
    support::compare_golden_fixtures(&fixtures_dir(), "compiler");
}
