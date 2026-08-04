//! The central compilation pipeline.
//!
//! It collects source files, then drives them through the lexer, parser, and name resolver,
//! with type checking and further stages to follow. Diagnostics raised along the way go
//! straight to the `DiagCtx` singleton rather than being threaded through this pipeline.

use std::io;
use std::path::Path;

use crate::ast::Ast;
use crate::diag::DiagCtx;
use crate::driver::core_lib;
use crate::driver::emit_debug;
use crate::driver::file_collector;
use crate::driver::options::BuildOptions;
use crate::driver::src_map::SrcMap;
use crate::hir::lower::lower_unit;
use crate::lexer::Lexer;
use crate::lexer::token::Token;
use crate::nameres::resolve;
use crate::parser::Parser;
use crate::typeck;

/// Collects every `.phi` file under `root`, and the core library, into the source map.
fn collect_sources(root: &Path) -> io::Result<()> {
    file_collector::collect(root)?;
    core_lib::register();
    Ok(())
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

/// Runs the full pipeline over every `.phi` file under `root`: collects sources, lexes,
/// parses, and resolves names, with type checking and further stages to follow.
///
/// Prints any diagnostics collected and reports whether compilation succeeded.
///
/// Each stage's result is dumped to stdout if [`options.dumps`](BuildOptions::dumps) asks
/// for it, in `SrcMap` order, which `file_collector` sorts to be reproducible. The AST dump
/// is what `phi build --ast` prints and what the golden tests under `tests/` snapshot; the
/// rest are the hooks behind `--hir` and `--debug`. Unlike `--ast` and `--hir`, the `--debug`
/// dumps resolve every `DefId` and `Symbol` to a name instead of leaving it a bare integer
/// -- see [`crate::driver::emit_debug`].
pub fn build(root: &Path, options: &BuildOptions) -> io::Result<bool> {
    collect_sources(root)?;
    let ast = parse(lex());

    if options.dumps.ast {
        emit_debug::print_ast(&ast);
    }

    // Desugars the whole program's AST into one HIR.
    let hir = lower_unit(&ast);

    if options.dumps.hir {
        emit_debug::print_hir(&hir, options.exclude_core);
    }

    // Resolves names within the HIR, which is what lets type checking below know which
    // definition each identifier refers to.
    let nameres = resolve(&hir);

    if options.dumps.nameres {
        emit_debug::print_nameres(&hir, &nameres, options.exclude_core);
    }

    if options.dumps.typeck {
        let checked = typeck::check(&hir, &nameres);
        emit_debug::print_typeck(&hir, &checked.tcx, &checked.types, options.exclude_core);
    }

    DiagCtx::report();
    Ok(!DiagCtx::has_errors())
}
