//! This file introduces the command-line entry point for the `phi` compiler.
//!
//! It parses the subcommand from the terminal arguments and dispatches to either `Driver`
//! or `Compiler`, depending on the task. Project-creation tasks, such as `phi new` or
//! `phi init`, are dispatched to `Driver`. Compilation tasks, such as `phi build`, are
//! dispatched to `Compiler`.

mod ast;
mod diag;
mod driver;
mod hir;
mod langitems;
mod lexer;
mod nameres;
mod parser;
#[cfg(test)]
mod testing;
mod typeck;

use crate::driver::cli::BuildOptions;
use crate::driver::compiler;
use std::env;
use std::path::Path;

fn main() {
    let args: Vec<String> = env::args().collect();

    if args.len() < 2 {
        driver::cli::print_usage();
        std::process::exit(1);
    }

    let command = &args[1];
    match command.as_str() {
        "new" => {
            if args.len() != 3 {
                eprintln!("error: 'new' requires exactly one argument (the project name)");
                std::process::exit(1);
            }

            let name = &args[2];
            if let Err(e) = driver::project::new(name) {
                eprintln!("error: {}", e);
                std::process::exit(1);
            }
        }

        "init" => {
            if args.len() != 2 {
                eprintln!("error: 'init' accepts no arguments");
                std::process::exit(1);
            }

            if let Err(e) = driver::project::init() {
                eprintln!("error: {}", e);
                std::process::exit(1);
            }
        }

        "build" => {
            let options = match BuildOptions::from_args(&args[2..]) {
                Ok(options) => options,
                Err(e) => {
                    eprintln!("error: {e}");
                    std::process::exit(1);
                }
            };

            match compiler::build(Path::new("."), &options) {
                Ok(true) => {}
                Ok(false) => std::process::exit(1),
                Err(e) => {
                    eprintln!("error: {}", e);
                    std::process::exit(1);
                }
            }
        }

        "help" | "--help" | "-h" => {
            driver::cli::print_usage();
        }

        _ => {
            eprintln!("error: unknown command '{}'", command);
            driver::cli::print_usage();
            std::process::exit(1);
        }
    }
}
