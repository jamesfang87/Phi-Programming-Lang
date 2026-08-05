//! The compilation commands: `check`, `build`, and `run`.
//!
//! Each collects the project's source files, then drives them through the lexer, parser,
//! lowering, name resolution, and type checking. Diagnostics raised along the way go straight
//! to the `DiagCtx` singleton rather than being threaded through this pipeline, and are
//! reported once at the end -- stages are deliberately not gated on whether an earlier one
//! raised an error, because the error variants in the AST and HIR let later stages skip
//! malformed input and keep reporting on the parts of the program that are well-formed.
//!
//! `build` and `run` are `check` plus stages that do not exist yet. They say so and stop.

use std::io;
use std::path::Path;

use crate::ast::Ast;
use crate::diag::DiagCtx;
use crate::driver::cli::{BuildOptions, Config};
use crate::driver::emit_debug;
use crate::driver::source::{SrcCollector, SrcMap};
use crate::hir::lower::lower_unit;
use crate::lexer::Lexer;
use crate::lexer::token::Token;
use crate::nameres::resolve;
use crate::parser::Parser;
use crate::typeck;

/// Collects every `.phi` file under `src_dir`, and the core library, into the source map.
///
/// The core library is registered second on purpose; see [`SrcCollector::collect_core`].
fn collect_sources(src_dir: &Path) -> io::Result<()> {
    if !src_dir.is_dir() {
        return Err(io::Error::new(
            io::ErrorKind::NotFound,
            format!("no `src` directory at `{}`", src_dir.display()),
        ));
    }
    SrcCollector::collect(src_dir)?;
    SrcCollector::collect_core();
    Ok(())
}

/// Says so, once, for each requested dump whose stage does not exist yet.
///
/// These are notes rather than diagnostics: they are about the compiler's own maturity, not
/// about the user's program, so they neither go through `DiagCtx` nor affect the exit code.
fn note_unimplemented_dumps(options: &BuildOptions) {
    if options.dumps.mir {
        eprintln!("note: MIR lowering is not implemented yet; --mir has no effect");
    }
    if options.dumps.llvm {
        eprintln!("note: LLVM IR generation is not implemented yet; --llvm has no effect");
    }
}

/// Lexes every collected file, in `SrcMap` order.
pub fn lex() -> Vec<Vec<Token>> {
    let mut token_streams = Vec::with_capacity(SrcMap::files().len());
    for file in SrcMap::files() {
        let mut lexer = Lexer::new(&file.content, file.global_offset);
        token_streams.push(lexer.tokenize());
    }
    token_streams
}

/// Parses every collected file's token stream into the build's [`Ast`].
pub fn parse(token_streams: Vec<Vec<Token>>) -> Ast {
    let streams: Vec<(Vec<Token>, usize)> = token_streams
        .into_iter()
        .zip(SrcMap::files().iter())
        .map(|(stream, file)| (stream, file.global_offset))
        .collect();
    Parser::new().parse_all(&streams)
}

/// Compiles the project without producing an artifact: collects sources, lexes, parses,
/// lowers, resolves names, and type checks.
///
/// Prints any diagnostics collected and returns whether compilation succeeded.
///
/// Each stage's result is dumped to stdout if [`options.dumps`](BuildOptions::dumps) asks for
/// it, in `SrcMap` order, which [`SrcCollector`] sorts to be reproducible. The AST dump is
/// what `phi build --ast` prints and what the golden tests under `tests/` snapshot; the rest
/// are the hooks behind `--hir` and `--emit-debug`. Unlike `--ast` and `--hir`, the
/// `--emit-debug` dumps resolve every `DefId` and `Symbol` to a name instead of leaving it a
/// bare integer -- see [`crate::driver::emit_debug`].
pub fn check(config: &Config, options: &BuildOptions) -> io::Result<bool> {
    note_unimplemented_dumps(options);
    collect_sources(&config.src_dir)?;
    let ast = parse(lex());

    if options.dumps.ast {
        emit_debug::print_ast(&ast);
    }

    // Desugars the whole program's AST into one HIR.
    let hir = lower_unit(&ast);

    if options.dumps.hir {
        emit_debug::print_hir(&hir, options.exclude_core_in_emit);
    }

    // Resolves names within the HIR, which is what lets type checking below know which
    // definition each identifier refers to.
    let nameres = resolve(&hir);
    let checked = typeck::check(&hir, &nameres);

    if options.dumps.nameres {
        emit_debug::print_nameres(&hir, &nameres, options.exclude_core_in_emit);
    }

    if options.dumps.typeck {
        emit_debug::print_typeck(
            &hir,
            &checked.tcx,
            &checked.types,
            options.exclude_core_in_emit,
        );
    }

    DiagCtx::report();
    Ok(!DiagCtx::has_errors())
}

/// Compiles the project and produces an artifact.
///
/// Identical to [`check`] until there is a code generation backend to be different from it.
pub fn build(config: &Config, options: &BuildOptions) -> io::Result<bool> {
    if !check(config, options)? {
        return Ok(false);
    }
    eprintln!("note: code generation is not implemented yet; 'build' currently only checks");
    Ok(true)
}

/// Builds the project and runs the resulting artifact.
///
/// There is no artifact yet, so this reports that and fails.
pub fn run(config: &Config) -> io::Result<bool> {
    if !build(config, &BuildOptions::default())? {
        return Ok(false);
    }
    eprintln!("error: 'run' requires a code generation backend, which is not implemented yet");
    Ok(false)
}
