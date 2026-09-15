use std::collections::HashMap;

use crate::ast::{Ast, ParsedSrcFile};
use crate::driver::source::FileOrigin;
use crate::hir::Hir;
use crate::lexer::Lexer;
use crate::lexer::token::Token;
use crate::mir::{Body, Instance, Mir};
use crate::nameres;
use crate::options::Mode;
use crate::parser::Parser;
use crate::session::Session;
use crate::typeck::results::TypeResolutions;
use crate::typeck::tyctx::TyCtx;

use super::fixtures::OPS_PREAMBLE;
use super::session::session;

fn lex_file(src: &str, origin: FileOrigin) -> (Vec<Token>, usize) {
    let chars: Vec<char> = src.chars().collect();
    let offset = session().add_file("<test>".to_string(), chars.clone(), origin);
    (Lexer::new(session(), &chars, offset).tokenize(), offset)
}

fn parse_file(src: &str, origin: FileOrigin) -> ParsedSrcFile {
    let (tokens, offset) = lex_file(src, origin);
    Parser::new(session()).parse(&tokens, offset)
}

fn assert_clean(src: &str) {
    let diagnostics = session().diagnostics();
    assert!(
        diagnostics.is_empty(),
        "unexpected diagnostics for {src:?}: {diagnostics:?}"
    );
}

pub fn lex_src(src: &str) -> (Vec<Token>, usize) {
    session().clear_diagnostics();
    lex_file(src, FileOrigin::User)
}

pub fn parse_src(src: &str) -> ParsedSrcFile {
    let (tokens, offset) = lex_src(src);
    let unit = Parser::new(session()).parse(&tokens, offset);
    assert_clean(src);
    unit
}

fn parse_files(sources: &[&str], origin: FileOrigin) -> Ast {
    session().clear_diagnostics();
    let files: Vec<ParsedSrcFile> = sources.iter().map(|src| parse_file(src, origin)).collect();
    for src in sources {
        assert_clean(src);
    }
    Ast::from(files)
}

fn lowered_hir(ast: Ast) -> Hir {
    let res = nameres::resolve(session(), &ast);
    Hir::from(session(), &ast, &res)
}

fn typechecked(sources: &[&str], origin: FileOrigin) -> (Hir, TyCtx, TypeResolutions) {
    let hir = lowered_hir(parse_files(sources, origin));
    session().clear_diagnostics();
    let checked = crate::typeck::check(session(), &hir);
    let diagnostics = session().diagnostics();
    assert!(
        diagnostics.is_empty(),
        "unexpected diagnostics for {sources:?}: {diagnostics:?}"
    );
    let crate::typeck::TypeckOutput { tcx, types } = checked;
    (hir, tcx, types)
}

fn monomorphized(
    sources: &[&str],
    origin: FileOrigin,
) -> (Hir, TyCtx, TypeResolutions, Mir, HashMap<Instance, Body>) {
    let (hir, mut tcx, types) = typechecked(sources, origin);
    let program = crate::mir::lower::lower(session(), &hir, &mut tcx, &types, Mode::Debug);
    let instances = crate::mir::monomorphize::monomorphize(&mut tcx, &program);
    (hir, tcx, types, program, instances)
}

pub fn lower_to_hir(src: &str) -> Hir {
    lowered_hir(parse_files(&[src], FileOrigin::User))
}

/// Lowers several source files to HIR as one crate, without inferring or lowering to MIR. Useful
/// for checks that reason about signatures alone, like the entry-point rules.
pub fn lower_to_hir_files(sources: &[&str]) -> Hir {
    lowered_hir(parse_files(sources, FileOrigin::User))
}

pub fn lower_to_hir_with_ops(src: &str) -> Hir {
    lowered_hir(parse_files(&[OPS_PREAMBLE, src], FileOrigin::User))
}

pub fn typeck_src(src: &str) -> Vec<String> {
    let hir = lower_to_hir(src);
    session().clear_diagnostics();
    crate::typeck::check(session(), &hir);
    session().messages()
}

pub fn typeck_src_files(sources: &[&str]) -> Vec<String> {
    let hir = lowered_hir(parse_files(sources, FileOrigin::User));
    session().clear_diagnostics();
    crate::typeck::check(session(), &hir);
    session()
        .diagnostics()
        .into_iter()
        .map(|diagnostic| diagnostic.message)
        .collect()
}

pub fn typeck_src_as_core(src: &str) -> Vec<String> {
    let hir = lowered_hir(parse_files(&[src], FileOrigin::Core));
    session().clear_diagnostics();
    crate::typeck::check(session(), &hir);
    session().messages()
}

pub fn typecheck_only(src: &str) -> (Hir, TyCtx, TypeResolutions) {
    typechecked(&[src], FileOrigin::User)
}

pub fn lower_to_mir(
    src: &str,
) -> (Hir, TyCtx, TypeResolutions, Mir, HashMap<Instance, Body>) {
    monomorphized(&[src], FileOrigin::User)
}

pub fn lower_mir_src_files(
    sources: &[&str],
) -> (Hir, TyCtx, TypeResolutions, Mir, HashMap<Instance, Body>) {
    monomorphized(sources, FileOrigin::User)
}

pub fn lower_mir_src_as_core(
    src: &str,
) -> (Hir, TyCtx, TypeResolutions, Mir, HashMap<Instance, Body>) {
    monomorphized(&[src], FileOrigin::Core)
}

fn mir_check_src(src: &str, check: impl FnOnce(&Session, &mut TyCtx, &Mir)) -> Vec<String> {
    let (_hir, mut tcx, _types, program, _instances) =
        monomorphized(&[OPS_PREAMBLE, src], FileOrigin::User);
    check(session(), &mut tcx, &program);
    session()
        .diagnostics()
        .into_iter()
        .map(|diagnostic| diagnostic.message)
        .collect()
}

pub fn mir_definite_init_src(src: &str) -> Vec<String> {
    mir_check_src(src, |session, tcx, program| {
        crate::mir::checks::borrowck::definite_init::check(session, tcx, program)
    })
}

pub fn mir_captures_src(src: &str) -> Vec<String> {
    mir_check_src(src, |session, tcx, program| {
        crate::mir::checks::borrowck::captures::check(session, tcx, program)
    })
}

pub fn mir_element_moves_src(src: &str) -> Vec<String> {
    mir_check_src(src, |session, tcx, program| {
        crate::mir::checks::borrowck::element_moves::check(session, tcx, program)
    })
}

pub fn mir_exclusivity_src(src: &str) -> Vec<String> {
    mir_check_src(src, |session, _tcx, program| {
        crate::mir::checks::borrowck::exclusivity::check(session, program)
    })
}

pub fn mir_never_read_src(src: &str) -> Vec<String> {
    mir_check_src(src, |session, _tcx, program| {
        crate::mir::checks::never_read::check(session, program)
    })
}
