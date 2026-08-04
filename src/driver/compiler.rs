//! This file defines `Compiler`, the central compilation pipeline.
//!
//! It collects source files, then drives them through the lexer, parser, and name resolver,
//! with type checking and further stages to follow. Diagnostics raised along the way go
//! straight to the `DiagCtx` singleton rather than being threaded through this pipeline.

use std::io;
use std::path::Path;

use crate::ast::ParsedSrcFile;
use crate::diag::DiagCtx;
use crate::driver::core_lib::CoreLib;
use crate::driver::emit_debug;
use crate::driver::file_collector::FileCollector;
use crate::driver::options::BuildOptions;
use crate::driver::src_map::SrcMap;
use crate::hir::lower::lower_unit;
use crate::lexer::Lexer;
use crate::lexer::token::Token;
use crate::nameres::resolve;
use crate::parser::Parser;
use crate::typeck;

pub struct Compiler;

impl Compiler {
    pub fn new() -> Self {
        Compiler {}
    }

    /// Collects every `.phi` file under `root`, and the core library, into the source map.
    ///
    /// The user's files are registered first so that they occupy the lowest global offsets. The
    /// core library is part of every build but changes on a completely different schedule to the
    /// program being compiled, and registering it last keeps a change to it -- adding a trait,
    /// say -- from shifting the span of every user file behind it.
    pub fn collect_sources(&mut self, root: &Path) -> io::Result<()> {
        FileCollector::collect(root)?;
        CoreLib::register();
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

    /// Parses every collected file's token stream into an AST.
    pub fn parse(token_streams: Vec<Vec<Token>>) -> Vec<ParsedSrcFile> {
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
    /// for it, in `SrcMap` order, which `FileCollector` sorts to be reproducible. The AST dump
    /// is what `phi build --ast` prints and what the golden tests under `tests/` snapshot; the
    /// rest are the hooks behind `--hir` and `--debug`. Unlike `--ast` and `--hir`, the `--debug`
    /// dumps resolve every `DefId` and `Symbol` to a name instead of leaving it a bare integer
    /// -- see [`crate::driver::emit_debug`].
    pub fn build(&mut self, root: &Path, options: &BuildOptions) -> io::Result<bool> {
        self.collect_sources(root)?;
        let asts = Self::parse(Self::lex());

        if options.dumps.ast {
            emit_debug::print_ast(&asts);
        }

        // Desugars every file's AST into one HIR.
        let hir = lower_unit(&asts);

        if options.dumps.hir {
            emit_debug::print_hir(&hir, options.exclude_core);
        }

        // Resolves names within the HIR, which is what lets type checking below know which
        // definition each identifier refers to.
        let nameres = resolve(&hir);

        if options.dumps.nameres {
            emit_debug::print_nameres(&hir, &nameres, options.exclude_core);
        }

        // Type checks the HIR, but only when the dump asks for it.
        //
        // This gating is a known problem and is meant to go away. A pass that runs only in
        // order to be printed has no forcing function: nothing fails when it regresses, so
        // bugs accumulate in it unnoticed and every build silently accepts programs it should
        // have rejected. `typeck::check` belongs on the unconditional path, with only
        // `print_typeck` below staying behind the flag.
        //
        // What blocks that today is that `check_expr` is still `todo!()` for most of
        // `ExprKind` -- `Binary`, `Call`, `If`, `Match`, `Variant`, and a dozen more. Running
        // it on every build doesn't turn latent type errors into reported ones; it turns every
        // build of any non-trivial program into a panic. `fun add(a: i32, b: i32) -> i32 {
        // return a + b; }` is enough to hit `todo!("check_expr: Binary")`, and the
        // `core_library` fixture under `tests/` dies on `todo!("check_expr: Variant")`.
        //
        // So the flag stays until `check_expr` handles every `ExprKind` it can be reached
        // with. Removing it is then a matter of hoisting the call out of this `if`.
        if options.dumps.typeck {
            let checked = typeck::check(&hir, &nameres);
            emit_debug::print_typeck(&hir, &checked.tcx, &checked.types, options.exclude_core);
        }

        DiagCtx::report();
        Ok(!DiagCtx::has_errors())
    }
}

impl Default for Compiler {
    fn default() -> Self {
        Compiler::new()
    }
}
