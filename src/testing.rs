use crate::ast::interner::Interner;
use crate::ast::{Ast, ParsedSrcFile};
use crate::diagnostics::DiagCtx;
use crate::driver::source::{FileOrigin, SrcMap};
use crate::hir::{DefId, Hir, HirId, OwnerNode, StmtKind};
use crate::lexer::Lexer;
use crate::nameres;
use crate::parser::Parser;
use crate::typeck::Typeck;

pub const OPS_PREAMBLE: &str = "module core::ops;
     public trait Add { fun add(&self, other: &Self) -> Self; }
     public trait Sub { fun sub(&self, other: &Self) -> Self; }
     public trait Mul { fun mul(&self, other: &Self) -> Self; }
     public trait Div { fun div(&self, other: &Self) -> Self; }
     public trait Rem { fun rem(&self, other: &Self) -> Self; }
     public trait Eq { fun eq(&self, other: &Self) -> bool; }
     public trait Comparable { fun less_than(&self, other: &Self) -> bool; }
     public trait Not { fun not(&self) -> Self; }
     public trait Copy { fun copy(&self) -> Self; }
     extend bool with Not { fun not(&self) -> Self { return !*self; } }
     extend bool with Copy { fun copy(&self) -> Self { return *self; } }
     extend i32 with Copy { fun copy(&self) -> Self { return *self; } }
     extend f64 with Copy { fun copy(&self) -> Self { return *self; } }
     extend i32 with Add { fun add(&self, other: &Self) -> Self { return *self + *other; } }
     extend i32 with Sub { fun sub(&self, other: &Self) -> Self { return *self - *other; } }
     extend i32 with Mul { fun mul(&self, other: &Self) -> Self { return *self * *other; } }
     extend i32 with Div { fun div(&self, other: &Self) -> Self { return *self / *other; } }
     extend i32 with Rem { fun rem(&self, other: &Self) -> Self { return *self % *other; } }
     extend i32 with Eq { fun eq(&self, other: &Self) -> bool { return *self == *other; } }
     extend i32 with Comparable { fun less_than(&self, other: &Self) -> bool { return *self < *other; } }
     extend f64 with Add { fun add(&self, other: &Self) -> Self { return *self + *other; } }
     extend f64 with Sub { fun sub(&self, other: &Self) -> Self { return *self - *other; } }
     extend f64 with Mul { fun mul(&self, other: &Self) -> Self { return *self * *other; } }
     extend f64 with Div { fun div(&self, other: &Self) -> Self { return *self / *other; } }
     extend f64 with Rem { fun rem(&self, other: &Self) -> Self { return *self % *other; } }
     extend f64 with Eq { fun eq(&self, other: &Self) -> bool { return *self == *other; } }
     extend f64 with Comparable { fun less_than(&self, other: &Self) -> bool { return *self < *other; } }
     ";

fn lex_file(src: &str, origin: FileOrigin) -> (Vec<crate::lexer::token::Token>, usize) {
    let chars: Vec<char> = src.chars().collect();
    let offset = SrcMap::add_file("<test>".to_string(), chars.clone(), origin);
    (Lexer::new(&chars, offset).tokenize(), offset)
}

fn parse_file(src: &str, origin: FileOrigin) -> ParsedSrcFile {
    let (tokens, offset) = lex_file(src, origin);
    Parser::new().parse(&tokens, offset)
}

fn assert_clean(src: &str) {
    let diagnostics = DiagCtx::diagnostics();
    assert!(
        diagnostics.is_empty(),
        "unexpected diagnostics for {src:?}: {diagnostics:?}"
    );
}

pub fn lex_src(src: &str) -> (Vec<crate::lexer::token::Token>, usize) {
    DiagCtx::clear();
    Interner::clear();
    lex_file(src, FileOrigin::User)
}

pub fn parse_src(src: &str) -> ParsedSrcFile {
    let (tokens, offset) = lex_src(src);
    let unit = Parser::new().parse(&tokens, offset);
    assert_clean(src);
    unit
}

fn parse_files(sources: &[&str], origin: FileOrigin) -> Ast {
    DiagCtx::clear();
    Interner::clear();
    let files: Vec<ParsedSrcFile> = sources.iter().map(|src| parse_file(src, origin)).collect();
    for src in sources {
        assert_clean(src);
    }
    Ast::from(files)
}

fn lowered_hir(ast: Ast) -> Hir {
    let res = nameres::resolve(&ast);
    Hir::from(&ast, &res)
}

fn typechecked(
    sources: &[&str],
    origin: FileOrigin,
) -> (
    Hir,
    crate::typeck::tyctx::TyCtx,
    crate::typeck::results::TypeResolutions,
) {
    let hir = lowered_hir(parse_files(sources, origin));
    DiagCtx::clear();
    let checked = crate::typeck::check(&hir);
    let diagnostics = DiagCtx::diagnostics();
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
) -> (
    Hir,
    crate::typeck::tyctx::TyCtx,
    crate::typeck::results::TypeResolutions,
    crate::mir::Mir,
    std::collections::HashMap<crate::mir::Instance, crate::mir::Body>,
) {
    let (hir, mut tcx, types) = typechecked(sources, origin);
    let program = crate::mir::lower::lower(&hir, &mut tcx, &types, crate::driver::cli::Mode::Debug);
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
    DiagCtx::clear();
    crate::typeck::check(&hir);
    DiagCtx::messages()
}

pub fn typeck_src_files(sources: &[&str]) -> Vec<String> {
    let hir = lowered_hir(parse_files(sources, FileOrigin::User));
    DiagCtx::clear();
    crate::typeck::check(&hir);
    DiagCtx::diagnostics()
        .into_iter()
        .map(|diagnostic| diagnostic.message)
        .collect()
}

pub fn typeck_src_as_core(src: &str) -> Vec<String> {
    let hir = lowered_hir(parse_files(&[src], FileOrigin::Core));
    DiagCtx::clear();
    crate::typeck::check(&hir);
    DiagCtx::messages()
}

pub fn typecheck_only(
    src: &str,
) -> (
    Hir,
    crate::typeck::tyctx::TyCtx,
    crate::typeck::results::TypeResolutions,
) {
    typechecked(&[src], FileOrigin::User)
}

pub fn lower_to_mir(
    src: &str,
) -> (
    Hir,
    crate::typeck::tyctx::TyCtx,
    crate::typeck::results::TypeResolutions,
    crate::mir::Mir,
    std::collections::HashMap<crate::mir::Instance, crate::mir::Body>,
) {
    monomorphized(&[src], FileOrigin::User)
}

pub fn lower_mir_src_files(
    sources: &[&str],
) -> (
    Hir,
    crate::typeck::tyctx::TyCtx,
    crate::typeck::results::TypeResolutions,
    crate::mir::Mir,
    std::collections::HashMap<crate::mir::Instance, crate::mir::Body>,
) {
    monomorphized(sources, FileOrigin::User)
}

pub fn lower_mir_src_as_core(
    src: &str,
) -> (
    Hir,
    crate::typeck::tyctx::TyCtx,
    crate::typeck::results::TypeResolutions,
    crate::mir::Mir,
    std::collections::HashMap<crate::mir::Instance, crate::mir::Body>,
) {
    monomorphized(&[src], FileOrigin::Core)
}

fn mir_check_src(
    src: &str,
    check: impl FnOnce(&mut crate::typeck::tyctx::TyCtx, &crate::mir::Mir),
) -> Vec<String> {
    let (_hir, mut tcx, _types, program, _instances) =
        monomorphized(&[OPS_PREAMBLE, src], FileOrigin::User);
    check(&mut tcx, &program);
    DiagCtx::diagnostics()
        .into_iter()
        .map(|diagnostic| diagnostic.message)
        .collect()
}

pub fn mir_definite_init_src(src: &str) -> Vec<String> {
    mir_check_src(src, |_tcx, program| {
        crate::mir::checks::borrowck::definite_init::check(program)
    })
}

pub fn mir_captures_src(src: &str) -> Vec<String> {
    mir_check_src(src, |tcx, program| {
        crate::mir::checks::borrowck::captures::check(tcx, program)
    })
}

pub fn mir_element_moves_src(src: &str) -> Vec<String> {
    mir_check_src(src, |tcx, program| {
        crate::mir::checks::borrowck::element_moves::check(tcx, program)
    })
}

pub fn mir_exclusivity_src(src: &str) -> Vec<String> {
    mir_check_src(src, |_tcx, program| {
        crate::mir::checks::borrowck::exclusivity::check(program)
    })
}

pub fn mir_never_read_src(src: &str) -> Vec<String> {
    mir_check_src(src, |_tcx, program| {
        crate::mir::checks::never_read::check(program)
    })
}

pub fn typeck_accepts(src: &str) {
    let reported = typeck_src(src);
    assert!(
        reported.is_empty(),
        "expected {src:?} to check: {reported:?}"
    );
}

pub fn typeck_rejects(src: &str, needle: &str) {
    let reported = typeck_src(src);
    assert_eq!(reported.len(), 1, "for {src:?}: {reported:?}");
    assert!(
        reported[0].contains(needle),
        "expected a diagnostic mentioning {needle:?} for {src:?}, got {reported:?}"
    );
}

pub fn mir_definite_init_accepts(src: &str) {
    let reported = mir_definite_init_src(src);
    assert!(
        reported.is_empty(),
        "expected {src:?} to pass definite-initialization checking: {reported:?}"
    );
}

pub fn mir_definite_init_rejects(src: &str, needle: &str) {
    let reported = mir_definite_init_src(src);
    assert_eq!(reported.len(), 1, "for {src:?}: {reported:?}");
    assert!(
        reported[0].contains(needle),
        "expected a diagnostic mentioning {needle:?} for {src:?}, got {reported:?}"
    );
}

pub fn mir_captures_accepts(src: &str) {
    let reported = mir_captures_src(src);
    assert!(
        reported.is_empty(),
        "expected {src:?} to pass the closure-capture move check: {reported:?}"
    );
}

pub fn mir_captures_rejects(src: &str, needle: &str) {
    let reported = mir_captures_src(src);
    assert_eq!(reported.len(), 1, "for {src:?}: {reported:?}");
    assert!(
        reported[0].contains(needle),
        "expected a diagnostic mentioning {needle:?} for {src:?}, got {reported:?}"
    );
}

pub fn mir_element_moves_accepts(src: &str) {
    let reported = mir_element_moves_src(src);
    assert!(
        reported.is_empty(),
        "expected {src:?} to pass the array-element move check: {reported:?}"
    );
}

pub fn mir_element_moves_rejects(src: &str, needle: &str) {
    let reported = mir_element_moves_src(src);
    assert_eq!(reported.len(), 1, "for {src:?}: {reported:?}");
    assert!(
        reported[0].contains(needle),
        "expected a diagnostic mentioning {needle:?} for {src:?}, got {reported:?}"
    );
}

pub fn mir_exclusivity_accepts(src: &str) {
    let reported = mir_exclusivity_src(src);
    assert!(
        reported.is_empty(),
        "expected {src:?} to pass exclusivity checking: {reported:?}"
    );
}

pub fn mir_exclusivity_rejects(src: &str, needle: &str) {
    let reported = mir_exclusivity_src(src);
    assert_eq!(reported.len(), 1, "for {src:?}: {reported:?}");
    assert!(
        reported[0].contains(needle),
        "expected a diagnostic mentioning {needle:?} for {src:?}, got {reported:?}"
    );
}

pub fn mir_never_read_accepts(src: &str) {
    let reported = mir_never_read_src(src);
    assert!(
        reported.is_empty(),
        "expected {src:?} to pass never-read checking: {reported:?}"
    );
}

pub fn mir_never_read_rejects(src: &str, needle: &str) {
    let reported = mir_never_read_src(src);
    assert_eq!(reported.len(), 1, "for {src:?}: {reported:?}");
    assert!(
        reported[0].contains(needle),
        "expected a diagnostic mentioning {needle:?} for {src:?}, got {reported:?}"
    );
}

#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Debug)]
pub enum Stage {
    Collect,
    Index,
    Coherence,
    Members,
}

pub fn checker_through(hir: &Hir, stage: Stage) -> Typeck<'_> {
    let mut checker = Typeck::new(hir);
    checker.collect_module(hir.root_id());
    if stage >= Stage::Index {
        checker.build_extend_index();
    }
    if stage >= Stage::Coherence {
        checker.check_coherence();
    }
    if stage >= Stage::Members {
        checker.check_trait_members();
    }
    checker
}

fn first_item(hir: &Hir, what: &str, pred: impl Fn(&OwnerNode) -> bool) -> DefId {
    hir.root()
        .items
        .iter()
        .copied()
        .find(|&item| pred(hir.def(item)))
        .unwrap_or_else(|| panic!("fixture declares no top-level {what}"))
}

pub fn named_def(hir: &Hir, name: &str) -> DefId {
    hir.root()
        .items
        .iter()
        .copied()
        .find(|&id| {
            let text = match hir.def(id) {
                OwnerNode::Struct(s) => s.name.text,
                OwnerNode::Enum(e) => e.name.text,
                OwnerNode::Trait(t) => t.name.text,
                OwnerNode::Function(f) => f.name.text,
                _ => return false,
            };
            Interner::resolve(text) == name
        })
        .unwrap_or_else(|| panic!("no definition named {name:?}"))
}

pub fn first_function(hir: &Hir) -> DefId {
    first_item(hir, "function", |def| matches!(def, OwnerNode::Function(_)))
}

pub fn first_struct(hir: &Hir) -> DefId {
    first_item(hir, "struct", |def| matches!(def, OwnerNode::Struct(_)))
}

pub fn first_trait(hir: &Hir) -> DefId {
    first_item(hir, "trait", |def| matches!(def, OwnerNode::Trait(_)))
}

pub fn first_extend(hir: &Hir) -> DefId {
    first_item(hir, "extend block", |def| {
        matches!(def, OwnerNode::Extend(_))
    })
}

pub fn first_extend_method(hir: &Hir) -> DefId {
    hir.extend(first_extend(hir)).methods[0]
}

pub fn find_return(hir: &Hir, def: DefId) -> (HirId, HirId) {
    let function = hir.function(def);
    let block_id = function.block.expect("fixture function has a body");
    let block = hir.block(block_id);

    for &stmt_id in &block.stmts {
        let stmt = hir.stmt(stmt_id);
        if let StmtKind::Return(Some(expr_id)) = stmt.kind {
            return (stmt_id, expr_id);
        }
    }
    panic!("fixture function has no `return <expr>;` statement");
}
