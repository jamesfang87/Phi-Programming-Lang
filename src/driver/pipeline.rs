use std::collections::HashMap;
use std::io;
use std::path::{Path, PathBuf};

use crate::ast::Ast;
use crate::codegen;
use crate::driver::cli::{BuildOptions, Config};
use crate::driver::emit_debug;
use crate::hir::Hir;
use crate::lexer::Lexer;
use crate::lexer::token::Token;
use crate::mir;
use crate::mir::{Body, Instance};
use crate::nameres;
use crate::options::Mode;
use crate::parser::Parser;
use crate::session::Session;
use crate::typeck;
use crate::typeck::tyctx::TyCtx;

/// Collects every `.phi` file under `src_dir`, and the core and standard libraries, into the
/// session's source map.
///
/// `core` and `std` are registered after the user's project, in that order, on purpose; see
/// [`Session::collect_core`] and [`Session::collect_std`].
fn collect_sources(session: &Session, src_dir: &Path) -> io::Result<()> {
    if !src_dir.is_dir() {
        return Err(io::Error::new(
            io::ErrorKind::NotFound,
            format!("no `src` directory at `{}`", src_dir.display()),
        ));
    }
    session.collect(src_dir)?;
    session.collect_core();
    session.collect_std();
    Ok(())
}

pub fn lex(session: &Session) -> Vec<(Vec<Token>, usize)> {
    session
        .files()
        .into_iter()
        .map(|file| {
            let tokens = Lexer::new(session, &file.content, file.global_offset).tokenize();
            (tokens, file.global_offset)
        })
        .collect()
}

pub fn parse(session: &Session, streams: Vec<(Vec<Token>, usize)>) -> Ast {
    Parser::new(session).parse_all(&streams)
}

/// Everything the front end (lex through monomorphize) produces, when it produces anything at
/// all -- `codegen`'s inputs, kept alongside each other so `build`/`run` don't need to
/// recompute what `check` already has.
///
/// Deliberately no `hir` or `types` here: from lowering onward the pipeline runs off `tcx` and the
/// MIR, so keeping the pre-MIR representations around for `codegen` would invite reaching back
/// into them. See `mir::def_infos` for the facts that crossing the boundary required snapshotting.
struct FrontendOutput {
    tcx: TyCtx,
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
fn run_frontend(
    session: &Session,
    config: &Config,
    options: &BuildOptions,
) -> io::Result<Option<FrontendOutput>> {
    collect_sources(session, &config.src_dir)?;
    let ast = parse(session, lex(session));

    if options.dumps.ast {
        emit_debug::print_ast(session, &ast);
    }

    let res = nameres::resolve(session, &ast);
    if options.dumps.nameres {
        emit_debug::print_nameres(session, &ast, &res);
    }

    let hir = Hir::from(session, &ast, &res);
    if options.dumps.hir {
        emit_debug::print_hir(session, &hir, options.exclude_core_in_emit);
    }

    let mut checked = typeck::check(session, &hir);
    if options.dumps.typeck {
        emit_debug::print_typeck(
            session,
            &hir,
            &checked.tcx,
            &checked.types,
            options.exclude_core_in_emit,
        );
    }
    // Signature rules for the entry point run before lowering: a malformed `main` should be
    // reported, not turned into MIR first. `typeck::check` does not run this itself because
    // having no `main` is only a warning, which would then appear in every type-inference test.
    typeck::entry_point::check(session, &hir);

    let program = mir::lower::lower(session, &hir, &mut checked.tcx, &checked.types, config.mode);
    mir::checks::run_checks(session, &mut checked.tcx, &program);
    let instances = mir::monomorphize::monomorphize(&mut checked.tcx, &program);
    let instances = mir::drop_elaboration::elaborate_drops(&mut checked.tcx, instances);

    if options.dumps.mir {
        emit_debug::print_mir(
            session,
            &hir,
            &checked.tcx,
            &program,
            &instances,
            options.exclude_core_in_emit,
        );
    }

    if session.report() {
        return Ok(None);
    }

    Ok(Some(FrontendOutput {
        tcx: checked.tcx,
        program,
        instances,
    }))
}

pub fn check(config: &Config, options: &BuildOptions) -> io::Result<bool> {
    let session = Session::new();
    Ok(run_frontend(&session, config, options)?.is_some())
}

pub fn build(config: &Config, options: &BuildOptions) -> io::Result<bool> {
    let session = Session::new();
    let Some(mut frontend) = run_frontend(&session, config, options)? else {
        return Ok(false);
    };

    let llvm = inkwell::context::Context::create();
    let module = match codegen::codegen(
        &session,
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

    // Codegen emits diagnostics of its own -- a missing or ill-formed `main`, for one. Without
    // this the frontend's `report` would already have run, so those would sit unrendered in the
    // collection while the build carried on to link, turning a compiler error into whatever the
    // linker made of the missing symbol.
    if session.report() {
        return Ok(false);
    }

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

    let artifact = artifact_path(config);
    std::fs::create_dir_all(
        artifact
            .parent()
            .expect("the artifact path always has a `target` parent"),
    )?;
    let emit_options = codegen::emit::EmitOptions {
        output_path: artifact,
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
    let status = std::process::Command::new(artifact_path(config)).status()?;
    Ok(status.success())
}

fn artifact_path(config: &Config) -> PathBuf {
    PathBuf::from("target").join(&config.name)
}
