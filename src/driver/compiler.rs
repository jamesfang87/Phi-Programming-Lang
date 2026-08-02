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
use crate::driver::file_collector::FileCollector;
use crate::driver::src_file::FileOrigin;
use crate::driver::src_map::SrcMap;
use crate::hir::lower::lower_unit;
use crate::lexer::Lexer;
use crate::lexer::token::Token;
use crate::nameres::resolve;
use crate::parser::Parser;

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
    pub fn build(&mut self, root: &Path, print_ast: bool, print_hir: bool) -> io::Result<bool> {
        self.collect_sources(root)?;
        let token_streams = self.lex();
        let asts = self.parse(token_streams);

        // The core library is part of every unit, but it isn't part of the program the user
        // asked to see, so it's left out of the dump.
        if print_ast {
            for (file, ast) in SrcMap::files()
                .iter()
                .zip(asts.iter())
                .filter(|(file, _)| file.origin == FileOrigin::User)
            {
                println!("// {}", file.name);
                println!("{ast:#?}");
            }
        }

        // Desugars every file's AST into one HIR.
        let hir = lower_unit(&asts);

        if print_hir {
            println!("{hir:#?}");
        }

        // Resolves names within the HIR. The result isn't consumed yet.
        //
        // Type checking will need it once it's wired up.
        let _name_res = resolve(&hir);

        // TODO: typecheck `hir` using `_name_res`, and continue the pipeline.

        DiagCtx::report();
        Ok(!DiagCtx::has_errors())
    }
}

impl Default for Compiler {
    fn default() -> Self {
        Compiler::new()
    }
}
