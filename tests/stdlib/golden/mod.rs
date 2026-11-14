use std::path::{Path, PathBuf};

use crate::support;

fn fixtures_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/stdlib/golden/fixtures")
}

#[test]
fn golden_fixtures() {
    support::compare_golden_fixtures(&fixtures_dir(), "stdlib");
}
