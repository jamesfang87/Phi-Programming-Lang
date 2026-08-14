//! The command-line entry point for the `phi` compiler.
//!
//! Argument parsing and dispatch both live in [`driver::cli`]; this is only the shell around
//! them that turns a returned exit code into a process exit.

mod ast;
mod diag;
mod diagnostics;
mod driver;
mod hir;
mod langitems;
mod lexer;
mod nameres;
mod parser;
#[cfg(test)]
mod testing;
mod typeck;

use std::env;

fn main() {
    let args: Vec<String> = env::args().skip(1).collect();
    std::process::exit(driver::cli::main(&args));
}
