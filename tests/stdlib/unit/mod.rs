#![allow(dead_code)]

use crate::support;

const CATEGORY: &str = "stdlib/unit";

fn run(name: &str, source: &str, expected: &str) {
    let output = support::run_src(CATEGORY, name, source);
    support::expect_stdout(&output, expected, name);
}

fn runs_ok(name: &str, source: &str) {
    let output = support::run_src(CATEGORY, name, source);
    support::expect_ok(&output, name);
}

mod fmt;
mod io;
mod iter;
mod libc;
mod ops;
mod option;
mod range;
mod result;
mod slice;
mod string;
mod text;
mod vector;
