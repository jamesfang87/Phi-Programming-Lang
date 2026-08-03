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
use crate::driver::src_map::SrcMap;
use crate::hir::lower::lower_unit;
use crate::lexer::token::Token;
use crate::lexer::Lexer;
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
    pub fn lex(&mut self) -> Vec<Vec<Token>> {
        let mut token_streams = Vec::with_capacity(SrcMap::files().len());
        for file in SrcMap::files() {
            let mut lexer = Lexer::new(&file.content, file.global_offset);
            token_streams.push(lexer.tokenize());
        }
        token_streams
    }

    /// Parses every collected file's token stream into an AST.
    pub fn parse(&mut self, token_streams: Vec<Vec<Token>>) -> Vec<ParsedSrcFile> {
        token_streams
            .into_iter()
            .zip(SrcMap::files().iter())
            .map(|(stream, file)| {
                let parser = Parser::new(stream, file.global_offset);
                parser.parse()
            })
            .collect()
    }

    /// Runs the full pipeline over every `.phi` file under `root`: collects sources, lexes,
    /// parses, and resolves names, with type checking and further stages to follow.
    ///
    /// Prints any diagnostics collected and reports whether compilation succeeded.
    ///
    /// If `print_ast` is set, the parsed AST for every file is pretty-printed to stdout before
    /// diagnostics are reported, in `SrcMap` order, which `FileCollector` sorts to be
    /// reproducible. This is the hook `phi build --ast` uses, and what the golden tests under
    /// `tests/` snapshot.
    ///
    /// If `print_hir` is set, the lowered HIR for the whole unit is pretty-printed to stdout
    /// once lowering finishes. This is the hook `phi build --hir` uses.
    ///
    /// If `debug` is set, every stage's results are dumped: the AST, the HIR, name resolution,
    /// and type checking (the latter two aren't otherwise wired into the pipeline yet, and are
    /// run here just to produce something to print). Unlike `--ast` and `--hir`, every `DefId`
    /// and `Symbol` in that dump is resolved to a name instead of being left as a bare integer.
    /// This is the hook `phi build --debug` uses -- see [`crate::driver::emit_debug`].
    ///
    /// `exclude_core` drops the core library -- which is linked into every build -- out of the
    /// `--hir` and `--debug` dumps. It has no effect otherwise; `--ast` always excludes the core
    /// library already.
    pub fn build(
        &mut self,
        root: &Path,
        print_ast: bool,
        print_hir: bool,
        debug: bool,
        exclude_core: bool,
    ) -> io::Result<bool> {
        self.collect_sources(root)?;
        let token_streams = self.lex();
        let asts = self.parse(token_streams);

        if print_ast || debug {
            emit_debug::print_ast(&asts);
        }

        // Desugars every file's AST into one HIR.
        let hir = lower_unit(&asts);

        if print_hir || debug {
            emit_debug::print_hir(&hir, exclude_core);
        }

        // Resolves names within the HIR. The result isn't consumed yet.
        //
        // Type checking will need it once it's wired up.
        let name_res = resolve(&hir);

        if debug {
            emit_debug::print_nameres(&hir, &name_res, exclude_core);
        }

        // TODO: typecheck `hir` using `name_res`, and continue the pipeline.
        //
        // `collect` is only run here, gated behind `--debug`, since nothing downstream consumes
        // its results yet.
        if debug {
            let (tcx, typeck_results) = typeck::collect(&hir, &name_res);
            emit_debug::print_typeck(&hir, &tcx, &typeck_results, exclude_core);
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
