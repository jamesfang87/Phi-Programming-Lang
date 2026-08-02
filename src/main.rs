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
mod typeck;

use crate::driver::Driver;
use crate::driver::compiler::Compiler;
use std::env;
use std::path::Path;

fn main() {
    let args: Vec<String> = env::args().collect();

    if args.len() < 2 {
        Driver::print_usage();
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
            if let Err(e) = Driver::new_project(name) {
                eprintln!("error: {}", e);
                std::process::exit(1);
            }
        }

        "init" => {
            // With no extra argument, initialize the current directory rather than requiring
            // `phi init .`.
            let path = if args.len() == 2 {
                Path::new(".")
            } else if args.len() == 3 {
                Path::new(&args[2])
            } else {
                eprintln!("error: 'init' accepts zero or one argument (the target directory)");
                std::process::exit(1);
            };

            if let Err(e) = Driver::init_project(path) {
                eprintln!("error: {}", e);
                std::process::exit(1);
            }
        }

        "build" => {
            let extra = &args[2..];
            let print_ast = extra.iter().any(|a| a == "--ast");
            let print_hir = extra.iter().any(|a| a == "--hir");
            if extra.iter().any(|a| a != "--ast" && a != "--hir") {
                eprintln!(
                    "error: unknown argument after 'build' (only '--ast' and '--hir' are accepted)"
                );
                std::process::exit(1);
            }

            let mut compiler = Compiler::new();
            match compiler.build(Path::new("."), print_ast, print_hir) {
                Ok(true) => {}
                Ok(false) => std::process::exit(1),
                Err(e) => {
                    eprintln!("error: {}", e);
                    std::process::exit(1);
                }
            }
        }

        "help" | "--help" | "-h" => {
            Driver::print_usage();
        }

        _ => {
            eprintln!("error: unknown command '{}'", command);
            Driver::print_usage();
            std::process::exit(1);
        }
    }
}
