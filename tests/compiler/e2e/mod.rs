#![allow(dead_code)]

use crate::support;

mod bugs;
mod diagnostics;
mod generated;
mod language;
mod modules;
mod ownership;
mod programs;
mod runtime;
mod types;

const CATEGORY: &str = "compiler/e2e";

fn run(name: &str, source: &str, expected: &str) {
    let output = support::run_src(CATEGORY, name, source);
    support::expect_stdout(&output, expected, name);
}

fn runs_ok(name: &str, source: &str) {
    let output = support::run_src(CATEGORY, name, source);
    support::expect_ok(&output, name);
}

fn rejects(name: &str, source: &str, needle: &str) {
    let output = support::check_src(CATEGORY, name, source);
    support::expect_reject(&output, needle, name);
}

fn aborts(name: &str, source: &str, needle: &str) {
    let output = support::run_src(CATEGORY, name, source);
    support::expect_abort(&output, needle, name);
}

fn run_project(name: &str, files: &[(&str, &str)], expected: &str) {
    let output = support::run_files(CATEGORY, name, files);
    support::expect_stdout(&output, expected, name);
}

fn rejects_project(name: &str, files: &[(&str, &str)], needle: &str) {
    let output = support::check_files(CATEGORY, name, files);
    support::expect_reject(&output, needle, name);
}
