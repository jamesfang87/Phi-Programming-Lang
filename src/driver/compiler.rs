use std::io;
use std::path::Path;

use crate::ast::SrcUnit;
use crate::diag::DiagCtx;
use crate::driver::file_collector::FileCollector;
use crate::driver::src_map::SrcMap;
use crate::lexer::Lexer;
use crate::lexer::token::Token;
use crate::parser::Parser;

/// The central pipeline: collects source files, then drives them through the lexer, parser,
/// and (eventually) later compilation stages. Diagnostics raised along the way go straight to
/// the [`DiagCtx`] singleton rather than being threaded through here.
pub struct Compiler;

impl Compiler {
    pub fn new() -> Self {
        Compiler {}
    }

    /// Collect every `.phi` file under `root` into the source map.
    pub fn collect_sources(&mut self, root: &Path) -> io::Result<()> {
        FileCollector::collect(root)
    }

    /// Lex every collected file, in `SrcMap` order.
    pub fn lex(&mut self) -> Vec<Vec<Token>> {
        let mut token_streams = Vec::with_capacity(SrcMap::files().len());
        for file in SrcMap::files() {
            let mut lexer = Lexer::new(&file.content, file.global_offset);
            token_streams.push(lexer.tokenize());
        }
        token_streams
    }

    /// Parse every collected file's token stream into an AST.
    pub fn parse(&mut self, token_streams: Vec<Vec<Token>>) -> Vec<SrcUnit> {
        token_streams
            .into_iter()
            .zip(SrcMap::files().iter())
            .map(|(stream, file)| {
                let parser = Parser::new(stream, file.global_offset);
                parser.parse()
            })
            .collect()
    }

    /// Runs the full pipeline over every `.phi` file under `root`: collect sources, lex, parse,
    /// and (eventually) typecheck, etc. Prints any diagnostics collected and reports whether
    /// compilation succeeded.
    ///
    /// If `print_ast` is set, the parsed AST for every file is pretty-printed to stdout (in
    /// `SrcMap` order, which `FileCollector` sorts to be reproducible) before diagnostics are
    /// reported — this is the hook `phi build --ast` uses, and what the golden tests under
    /// `tests/` snapshot.
    pub fn build(&mut self, root: &Path, print_ast: bool) -> io::Result<bool> {
        self.collect_sources(root)?;
        let token_streams = self.lex();
        let asts = self.parse(token_streams);

        if print_ast {
            for (file, ast) in SrcMap::files().iter().zip(asts.iter()) {
                println!("// {}", file.name);
                println!("{ast:#?}");
            }
        }

        // TODO: typecheck `asts` and continue the pipeline.

        DiagCtx::report();
        Ok(!DiagCtx::has_errors())
    }
}

impl Default for Compiler {
    fn default() -> Self {
        Compiler::new()
    }
}
