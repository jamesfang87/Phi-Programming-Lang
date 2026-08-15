use std::io;
use std::path::Path;

use crate::ast::Ast;
use crate::diag::DiagCtx;
use crate::driver::cli::{BuildOptions, Config, Mode};
use crate::driver::emit_debug;
use crate::driver::source::{SrcCollector, SrcMap};
use crate::hir::lower::lower_program;
use crate::lexer::token::Token;
use crate::lexer::Lexer;
use crate::mir;
use crate::nameres;
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

fn note_unimplemented_dumps(config: &Config, options: &BuildOptions) {
    if options.dumps.llvm {
        eprintln!("note: LLVM IR generation is not implemented yet; --llvm has no effect");
    }
    if config.mode == Mode::Release {
        eprintln!("note: release mode is not implemented yet; `mode = \"release\"` has no effect");
    }
}

pub fn lex() -> Vec<Vec<Token>> {
    SrcMap::files()
        .iter()
        .map(|file| Lexer::new(&file.content, file.global_offset).tokenize())
        .collect()
}

pub fn parse(token_streams: Vec<Vec<Token>>) -> Ast {
    let streams: Vec<(Vec<Token>, usize)> = token_streams
        .into_iter()
        .zip(SrcMap::files().iter())
        .map(|(stream, file)| (stream, file.global_offset))
        .collect();
    Parser::new().parse_all(&streams)
}

pub fn check(config: &Config, options: &BuildOptions) -> io::Result<bool> {
    note_unimplemented_dumps(config, options);
    collect_sources(&config.src_dir)?;
    let ast = parse(lex());

    if options.dumps.ast {
        emit_debug::print_ast(&ast);
    }

    let res = nameres::resolve(&ast);
    if options.dumps.nameres {
        emit_debug::print_nameres(&ast, &res);
    }

    let hir = lower_program(&ast, &res);
    if options.dumps.hir {
        emit_debug::print_hir(&hir, options.exclude_core_in_emit);
    }

    let mut checked = typeck::check(&hir);
    if options.dumps.typeck {
        emit_debug::print_typeck(
            &hir,
            &checked.tcx,
            &checked.types,
            options.exclude_core_in_emit,
        );
    }

    // TODO: Remove this assumption
    // Lowering #2 assumes a fully type-checked body with no `TyKind::Error` left in it anywhere
    // (see `mir::lower`'s own module docs: it is diagnostics-free by design, the same way
    // Lowering #1 is, because everything it needs is already validated by the stage before it).
    // A program `typeck` already rejected has nothing valid to lower, so this only runs once
    // there is nothing already reported to make that assumption false.
    if !DiagCtx::has_errors() {
        // Followed by monomorphization: `checked.types` alone is enough to build one generic
        // `Body` per function/method/closure, and substituting those into the concrete instances
        // actually used needs only `checked.tcx` in addition, mutably, to intern the substituted
        // types it produces.
        let program =
            mir::lower::lower_program(&hir, &mut checked.tcx, &checked.types, config.mode);
        let instances = mir::monomorphize::monomorphize(&hir, &mut checked.tcx, &program);

        if options.dumps.mir {
            emit_debug::print_mir(&hir, &checked.tcx, &instances, options.exclude_core_in_emit);
        }
    }

    DiagCtx::report();
    Ok(!DiagCtx::has_errors())
}

pub fn build(config: &Config, options: &BuildOptions) -> io::Result<bool> {
    if !check(config, options)? {
        return Ok(false);
    }
    eprintln!("note: code generation is not implemented yet; 'build' currently only checks");
    Ok(true)
}

pub fn run(config: &Config) -> io::Result<bool> {
    if !build(config, &BuildOptions::default())? {
        return Ok(false);
    }
    eprintln!("error: 'run' requires a code generation backend, which is not implemented yet");
    Ok(false)
}
