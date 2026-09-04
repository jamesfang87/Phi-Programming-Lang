use std::collections::HashMap;
use std::io;
use std::path::{Path, PathBuf};

use crate::ast::Ast;
use crate::codegen;
use crate::diagnostics::DiagCtx;
use crate::driver::cli::{BuildOptions, Config, Mode};
use crate::driver::emit_debug;
use crate::driver::source::{SrcCollector, SrcMap};
use crate::hir::Hir;
use crate::hir::lower::lower_ast;
use crate::lexer::Lexer;
use crate::lexer::token::Token;
use crate::mir;
use crate::mir::{Body, Instance};
use crate::nameres;
use crate::parser::Parser;
use crate::typeck;
use crate::typeck::results::TypeResolutions;
use crate::typeck::tyctx::TyCtx;

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

/// Everything the front end (lex through monomorphize) produces, when it produces anything at
/// all -- `codegen`'s inputs, kept alongside each other so `build`/`run` don't need to
/// recompute what `check` already has.
struct FrontendOutput {
    hir: Hir,
    tcx: TyCtx,
    types: TypeResolutions,
    program: mir::Mir,
    instances: HashMap<Instance, Body>,
}

/// Runs every front-end stage -- lex, parse, name resolution, HIR lowering, type checking, MIR
/// lowering, MIR checks, and monomorphization -- reporting any requested `--ast`/`--hir`/etc.
/// dump along the way, then reports accumulated diagnostics.
///
/// Returns `Some` with the computed artifacts on success, `None` if any stage reported a
/// diagnostic error. `check`, `build`, and `run` all funnel through this so that a change to
/// the front end only has one place to land, and so `check`'s externally-observed behavior
/// (report diagnostics, then say pass/fail) stays exactly what it was before `build`/`run`
/// grew a real code generation backend to run afterward.
fn run_frontend(config: &Config, options: &BuildOptions) -> io::Result<Option<FrontendOutput>> {
    collect_sources(&config.src_dir)?;
    let ast = parse(lex());

    if options.dumps.ast {
        emit_debug::print_ast(&ast);
    }

    let res = nameres::resolve(&ast);
    if options.dumps.nameres {
        emit_debug::print_nameres(&ast, &res);
    }

    let hir = lower_ast(&ast, &res);
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

    let program = mir::lower::lower(&hir, &mut checked.tcx, &checked.types, config.mode);
    mir::checks::run_checks(&program);
    let instances = mir::monomorphize::monomorphize(&hir, &mut checked.tcx, &program);

    if options.dumps.mir {
        emit_debug::print_mir(
            &hir,
            &checked.tcx,
            &program,
            &instances,
            options.exclude_core_in_emit,
        );
    }

    DiagCtx::report();
    if DiagCtx::has_errors() {
        return Ok(None);
    }

    Ok(Some(FrontendOutput {
        hir,
        tcx: checked.tcx,
        types: checked.types,
        program,
        instances,
    }))
}

pub fn check(config: &Config, options: &BuildOptions) -> io::Result<bool> {
    Ok(run_frontend(config, options)?.is_some())
}

pub fn build(config: &Config, options: &BuildOptions) -> io::Result<bool> {
    let Some(mut frontend) = run_frontend(config, options)? else {
        return Ok(false);
    };

    let llvm = inkwell::context::Context::create();
    let module = match codegen::codegen(
        &llvm,
        &mut frontend.tcx,
        &frontend.program,
        &frontend.instances,
        &config.name,
    ) {
        Ok(m) => m,
        Err(e) => {
            eprintln!("error: codegen failed: {e:?}");
            return Ok(false);
        }
    };

    if options.dumps.llvm {
        // The spec requires the dumped IR to reflect the module as codegen left it -- after
        // verification (so a malformed module is caught first, the same way `emit` itself would
        // catch it) but before optimization (so what's printed is what codegen actually built,
        // not what a later pass rewrote it into). `emit` only verifies internally right before
        // it optimizes, with no hook to observe the module in between, so this verifies here too
        // -- a second `module.verify()` call is cheap (a single linear pass over the module) and
        // harmless to run twice; it is not a meaningfully different check than the one `emit`
        // performs immediately afterward.
        if let Err(e) = module.verify() {
            eprintln!(
                "error: codegen failed: {}\n{}",
                e,
                module.print_to_string().to_string()
            );
            return Ok(false);
        }
        println!("{}", module.print_to_string().to_string());
    }

    let target_dir = PathBuf::from("target");
    std::fs::create_dir_all(&target_dir)?;
    let emit_options = codegen::emit::EmitOptions {
        output_path: target_dir.join(&config.name),
        release: config.mode == Mode::Release,
    };
    match codegen::emit::emit(&module, &emit_options) {
        Ok(_) => Ok(true),
        Err(e) => {
            eprintln!("error: {e:?}");
            Ok(false)
        }
    }
}

pub fn run(config: &Config) -> io::Result<bool> {
    if !build(config, &BuildOptions::default())? {
        return Ok(false);
    }
    let status = std::process::Command::new(PathBuf::from("target").join(&config.name)).status()?;
    Ok(status.success())
}
