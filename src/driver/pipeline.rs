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
use crate::typeck::ty::ctx::TyCtx;

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

struct FrontendOutput {
    hir: Hir,
    tcx: TyCtx,
    program: mir::Mir,
    instances: HashMap<Instance, Body>,
}

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
    crate::checks::mutability::check(session, &hir, &checked.tcx, &checked.types);
    // Signature rules for the entry point run before lowering: a malformed `main` should be
    // reported, not turned into MIR first.
    let main = crate::checks::entry_point::check(session, &hir);

    let program = mir::lower::lower(session, &hir, &mut checked.tcx, &checked.types, config.mode);
    mir::checks::run_checks(session, &hir, &mut checked.tcx, &program);
    let instances = mir::monomorphize::monomorphize(&mut checked.tcx, &program, main);
    let instances = mir::drop_elaboration::elaborate_drops(&mut checked.tcx, instances);

    if options.dumps.mir {
        emit_debug::print_mir(
            session,
            &hir,
            &checked.tcx,
            &instances,
            options.exclude_core_in_emit,
        );
    }

    if session.report() {
        return Ok(None);
    }

    Ok(Some(FrontendOutput {
        hir,
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
        &frontend.hir,
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

    if session.report() {
        return Ok(false);
    }

    if options.dumps.llvm {
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
