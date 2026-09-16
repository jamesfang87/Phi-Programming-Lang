mod ast;
mod checks;
mod codegen;
mod diagnostics;
mod driver;
mod hir;
mod langitems;
mod lexer;
mod mir;
mod nameres;
mod options;
mod parser;
mod session;
mod typeck;

#[cfg(test)]
mod testing;

use std::env;

fn main() {
    let args: Vec<String> = env::args().skip(1).collect();
    std::process::exit(driver::cli::main(&args));
}
