use crate::ast::interner::Interner;
use crate::ast::{Ast, ParsedSrcFile};
use crate::diagnostics::DiagCtx;
use crate::driver::source::{FileOrigin, SrcMap};
use crate::hir::lower::lower_ast;
use crate::hir::{DefId, Hir, HirId, OwnerNode, StmtKind};
use crate::lexer::Lexer;
use crate::lexer::token::Token;
use crate::nameres;
use crate::parser::Parser;
use crate::typeck::Typeck;

// -----------------------------------------------------------------
// Driving the pipeline
// -----------------------------------------------------------------

pub fn lex_src(src: &str) -> (Vec<Token>, usize) {
    DiagCtx::clear();
    Interner::clear();
    let chars: Vec<char> = src.chars().collect();
    let offset = SrcMap::add_file("<test>".to_string(), chars.clone(), FileOrigin::User);
    (Lexer::new(&chars, offset).tokenize(), offset)
}

pub fn parse_src(src: &str) -> ParsedSrcFile {
    let (tokens, offset) = lex_src(src);
    let unit = Parser::new().parse(&tokens, offset);
    assert_clean(src);
    unit
}

pub fn lower_to_hir(src: &str) -> Hir {
    let ast = Ast::new(vec![parse_src(src)]);
    let res = nameres::resolve(&ast);
    lower_ast(&ast, &res)
}

// TODO: What the hell is the point of this?
pub fn resolve_src(src: &str) -> Hir {
    lower_to_hir(src)
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

pub const OPS_PREAMBLE: &str = "module core::ops;
     public trait Add { fun add(&self, other: &Self) -> Self; }
     public trait Sub { fun sub(&self, other: &Self) -> Self; }
     public trait Mul { fun mul(&self, other: &Self) -> Self; }
     public trait Div { fun div(&self, other: &Self) -> Self; }
     public trait Rem { fun rem(&self, other: &Self) -> Self; }
     public trait Eq { fun eq(&self, other: &Self) -> bool; }
     public trait Comparable { fun less_than(&self, other: &Self) -> bool; }
     public trait Not { fun not(&self) -> Self; }
     extend bool with Not { fun not(&self) -> Self { return !*self; } }
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

// TODO: move this into DiagCtx
pub fn messages() -> Vec<String> {
    DiagCtx::diagnostics()
        .into_iter()
        .map(|diagnostic| diagnostic.message)
        .collect()
}

pub fn typeck_src(src: &str) -> Vec<String> {
    let hir = resolve_src(src);
    DiagCtx::clear();
    crate::typeck::check(&hir);
    messages()
}

// Why is there typeck_src and typeck_src_files?
pub fn typeck_src_files(sources: &[&str]) -> Vec<String> {
    DiagCtx::clear();
    Interner::clear();
    let files: Vec<ParsedSrcFile> = sources
        .iter()
        .map(|src| {
            let chars: Vec<char> = src.chars().collect();
            let offset = SrcMap::add_file("<test>".to_string(), chars.clone(), FileOrigin::User);
            let tokens = Lexer::new(&chars, offset).tokenize();
            Parser::new().parse(&tokens, offset)
        })
        .collect();
    let ast = Ast::new(files);
    let res = nameres::resolve(&ast);
    let hir = lower_ast(&ast, &res);

    DiagCtx::clear();
    crate::typeck::check(&hir);

    DiagCtx::diagnostics()
        .into_iter()
        .map(|diagnostic| diagnostic.message)
        .collect()
}

pub fn typecheck_only(
    src: &str,
) -> (
    Hir,
    crate::typeck::tyctx::TyCtx,
    crate::typeck::results::TypeResolutions,
) {
    let hir = resolve_src(src);
    DiagCtx::clear();
    let checked = crate::typeck::check(&hir);
    let diagnostics = DiagCtx::diagnostics();
    assert!(
        diagnostics.is_empty(),
        "unexpected diagnostics for {src:?}: {diagnostics:?}"
    );
    let crate::typeck::TypeckOutput { tcx, types } = checked;
    (hir, tcx, types)
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
    let hir = resolve_src(src);
    DiagCtx::clear();
    let checked = crate::typeck::check(&hir);
    let diagnostics = DiagCtx::diagnostics();
    assert!(
        diagnostics.is_empty(),
        "unexpected diagnostics for {src:?}: {diagnostics:?}"
    );
    let crate::typeck::TypeckOutput { mut tcx, types } = checked;
    let program = crate::mir::lower::lower(&hir, &mut tcx, &types, crate::driver::cli::Mode::Debug);
    let instances = crate::mir::monomorphize::monomorphize(&hir, &mut tcx, &program);
    (hir, tcx, types, program, instances)
}

/// [`lower_mir_src`], but spread across several files, each its own `module` -- the MIR-level
/// counterpart to [`typeck_src_files`], needed for anything that must cross a module boundary
/// (e.g. a nested submodule alongside its parent) rather than living in the single implicit
/// module one fixture string gets.
///
/// Panics under the same conditions [`lower_mir_src`] does.
pub fn lower_mir_src_files(
    sources: &[&str],
) -> (
    Hir,
    crate::typeck::tyctx::TyCtx,
    crate::typeck::results::TypeResolutions,
    crate::mir::Mir,
    std::collections::HashMap<crate::mir::Instance, crate::mir::Body>,
) {
    DiagCtx::clear();
    Interner::clear();
    let files: Vec<ParsedSrcFile> = sources
        .iter()
        .map(|src| {
            let chars: Vec<char> = src.chars().collect();
            let offset = SrcMap::add_file("<test>".to_string(), chars.clone(), FileOrigin::User);
            let tokens = Lexer::new(&chars, offset).tokenize();
            Parser::new().parse(&tokens, offset)
        })
        .collect();
    let ast = Ast::new(files);
    let res = nameres::resolve(&ast);
    let hir = lower_ast(&ast, &res);

    DiagCtx::clear();
    let checked = crate::typeck::check(&hir);
    let diagnostics = DiagCtx::diagnostics();
    assert!(
        diagnostics.is_empty(),
        "unexpected diagnostics for {sources:?}: {diagnostics:?}"
    );
    let crate::typeck::TypeckOutput { mut tcx, types } = checked;
    let program = crate::mir::lower::lower(&hir, &mut tcx, &types, crate::driver::cli::Mode::Debug);
    let instances = crate::mir::monomorphize::monomorphize(&hir, &mut tcx, &program);
    (hir, tcx, types, program, instances)
}

/// Asserts that `src` type checks with nothing reported.
pub fn typeck_accepts(src: &str) {
    let reported = typeck_src(src);
    assert!(
        reported.is_empty(),
        "expected {src:?} to check: {reported:?}"
    );
}

/// Asserts that `src` is rejected by exactly one diagnostic, whose message contains `needle`.
///
/// One rather than at least one. A second diagnostic from the same fixture is usually a
/// cascade, which this pass prevents, and tolerating it would mask real failures.
pub fn typeck_rejects(src: &str, needle: &str) {
    let reported = typeck_src(src);
    assert_eq!(reported.len(), 1, "for {src:?}: {reported:?}");
    assert!(
        reported[0].contains(needle),
        "expected a diagnostic mentioning {needle:?} for {src:?}, got {reported:?}"
    );
}

/// [`typeck_src`], but registering `src` under [`FileOrigin::Core`] instead of
/// [`FileOrigin::User`]. Exists for the bodiless-intrinsic rule, the one place file provenance
/// changes what type checking accepts -- everything else in the pipeline treats the two origins
/// identically.
pub fn typeck_src_as_core(src: &str) -> Vec<String> {
    DiagCtx::clear();
    Interner::clear();
    let chars: Vec<char> = src.chars().collect();
    let offset = SrcMap::add_file("<core-test>".to_string(), chars.clone(), FileOrigin::Core);
    let tokens = Lexer::new(&chars, offset).tokenize();
    let file = Parser::new().parse(&tokens, offset);
    let ast = Ast::new(vec![file]);
    let res = nameres::resolve(&ast);
    let hir = lower_ast(&ast, &res);

    DiagCtx::clear();
    crate::typeck::check(&hir);
    messages()
}

/// [`lower_mir_src`], but registering `src` under [`FileOrigin::Core`] instead of
/// [`FileOrigin::User`] -- the one way a test can put a bodiless function other than a trait
/// method declaration through MIR lowering without tripping the bodiless-intrinsic rule (see
/// [`typeck_src_as_core`]'s own doc comment). Needed for exercising `core::io::write_bytes`,
/// since only a real `Body`-carrying call site (not a hand-rolled `Instance`/`Body` pair) proves
/// name resolution actually threads its lang-item `DefId` all the way to `mir::lower`'s output.
pub fn lower_mir_src_as_core(
    src: &str,
) -> (
    Hir,
    crate::typeck::tyctx::TyCtx,
    crate::typeck::results::TypeResolutions,
    crate::mir::Mir,
    std::collections::HashMap<crate::mir::Instance, crate::mir::Body>,
) {
    DiagCtx::clear();
    Interner::clear();
    let chars: Vec<char> = src.chars().collect();
    let offset = SrcMap::add_file("<core-test>".to_string(), chars.clone(), FileOrigin::Core);
    let tokens = Lexer::new(&chars, offset).tokenize();
    let file = Parser::new().parse(&tokens, offset);
    let ast = Ast::new(vec![file]);
    let res = nameres::resolve(&ast);
    let hir = lower_ast(&ast, &res);

    DiagCtx::clear();
    let checked = crate::typeck::check(&hir);
    let diagnostics = DiagCtx::diagnostics();
    assert!(
        diagnostics.is_empty(),
        "unexpected diagnostics for {src:?}: {diagnostics:?}"
    );
    let crate::typeck::TypeckOutput { mut tcx, types } = checked;
    let program = crate::mir::lower::lower(&hir, &mut tcx, &types, crate::driver::cli::Mode::Debug);
    let instances = crate::mir::monomorphize::monomorphize(&hir, &mut tcx, &program);
    (hir, tcx, types, program, instances)
}

/// Runs the whole pipeline over `src` through `mir::checks::constck`, and hands back the messages that
/// pass reported.
///
/// Type checking itself is asserted clean first, the same "diagnostics-free by design" contract
/// [`lower_mir_src`] documents: a fixture meant to exercise something type checking itself
/// rejects belongs with [`typeck_rejects`] instead, not here.
pub fn mir_constck_src(src: &str) -> Vec<String> {
    let hir = resolve_src(src);
    DiagCtx::clear();
    let checked = crate::typeck::check(&hir);
    let diagnostics = DiagCtx::diagnostics();
    assert!(
        diagnostics.is_empty(),
        "unexpected diagnostics for {src:?}: {diagnostics:?}"
    );
    let crate::typeck::TypeckOutput { mut tcx, types } = checked;
    let program = crate::mir::lower::lower(&hir, &mut tcx, &types, crate::driver::cli::Mode::Debug);
    crate::mir::checks::constck::check(&program);

    DiagCtx::diagnostics()
        .into_iter()
        .map(|diagnostic| diagnostic.message)
        .collect()
}

/// Asserts that `src` passes `mir::checks::constck` with nothing reported.
pub fn mir_constck_accepts(src: &str) {
    let reported = mir_constck_src(src);
    assert!(
        reported.is_empty(),
        "expected {src:?} to pass constness checking: {reported:?}"
    );
}

/// Asserts that `src` is rejected by `mir::checks::constck` with exactly one diagnostic, whose message
/// contains `needle`. One rather than at least one, for the same reason [`typeck_rejects`]
/// insists on it: a second diagnostic from the same fixture is usually a cascade.
pub fn mir_constck_rejects(src: &str, needle: &str) {
    let reported = mir_constck_src(src);
    assert_eq!(reported.len(), 1, "for {src:?}: {reported:?}");
    assert!(
        reported[0].contains(needle),
        "expected a diagnostic mentioning {needle:?} for {src:?}, got {reported:?}"
    );
}

/// Runs the whole pipeline over `src` through `mir::checks::borrowck::definite_init`, and hands
/// back the messages that pass reported.
///
/// Type checking itself is asserted clean first, the same "diagnostics-free by design" contract
/// [`lower_mir_src`] documents: a fixture meant to exercise something type checking itself
/// rejects belongs with [`typeck_rejects`] instead, not here.
pub fn mir_definite_init_src(src: &str) -> Vec<String> {
    let hir = resolve_src(src);
    DiagCtx::clear();
    let checked = crate::typeck::check(&hir);
    let diagnostics = DiagCtx::diagnostics();
    assert!(
        diagnostics.is_empty(),
        "unexpected diagnostics for {src:?}: {diagnostics:?}"
    );
    let crate::typeck::TypeckOutput { mut tcx, types } = checked;
    let program = crate::mir::lower::lower(&hir, &mut tcx, &types, crate::driver::cli::Mode::Debug);
    crate::mir::checks::borrowck::definite_init::check(&program);

    DiagCtx::diagnostics()
        .into_iter()
        .map(|diagnostic| diagnostic.message)
        .collect()
}

/// Asserts that `src` passes `mir::checks::borrowck::definite_init` with nothing reported.
pub fn mir_definite_init_accepts(src: &str) {
    let reported = mir_definite_init_src(src);
    assert!(
        reported.is_empty(),
        "expected {src:?} to pass definite-initialization checking: {reported:?}"
    );
}

/// Asserts that `src` is rejected by `mir::checks::borrowck::definite_init` with exactly one
/// diagnostic, whose message contains `needle`. One rather than at least one, for the same reason
/// [`typeck_rejects`] insists on it: a second diagnostic from the same fixture is usually a
/// cascade.
pub fn mir_definite_init_rejects(src: &str, needle: &str) {
    let reported = mir_definite_init_src(src);
    assert_eq!(reported.len(), 1, "for {src:?}: {reported:?}");
    assert!(
        reported[0].contains(needle),
        "expected a diagnostic mentioning {needle:?} for {src:?}, got {reported:?}"
    );
}

pub fn mir_exclusivity_src(src: &str) -> Vec<String> {
    let hir = resolve_src(src);
    DiagCtx::clear();
    let checked = crate::typeck::check(&hir);
    let diagnostics = DiagCtx::diagnostics();
    assert!(
        diagnostics.is_empty(),
        "unexpected diagnostics for {src:?}: {diagnostics:?}"
    );
    let crate::typeck::TypeckOutput { mut tcx, types } = checked;
    let program = crate::mir::lower::lower(&hir, &mut tcx, &types, crate::driver::cli::Mode::Debug);
    crate::mir::checks::borrowck::exclusivity::check(&program);

    DiagCtx::diagnostics()
        .into_iter()
        .map(|diagnostic| diagnostic.message)
        .collect()
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

pub fn mir_never_read_src(src: &str) -> Vec<String> {
    let hir = resolve_src(src);
    DiagCtx::clear();
    let checked = crate::typeck::check(&hir);
    let diagnostics = DiagCtx::diagnostics();
    assert!(
        diagnostics.is_empty(),
        "unexpected diagnostics for {src:?}: {diagnostics:?}"
    );
    let crate::typeck::TypeckOutput { mut tcx, types } = checked;
    let program = crate::mir::lower::lower(&hir, &mut tcx, &types, crate::driver::cli::Mode::Debug);
    crate::mir::checks::never_read::check(&program);

    DiagCtx::diagnostics()
        .into_iter()
        .map(|diagnostic| diagnostic.message)
        .collect()
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

fn assert_clean(src: &str) {
    let diagnostics = DiagCtx::diagnostics();
    assert!(
        diagnostics.is_empty(),
        "unexpected diagnostics for {src:?}: {diagnostics:?}"
    );
}

// -----------------------------------------------------------------
// Digging through the HIR
// -----------------------------------------------------------------

/// The `DefId` of the first top-level item `pred` accepts. `what` names what was being looked
/// for, for the panic message.
fn first_item(hir: &Hir, what: &str, pred: impl Fn(&OwnerNode) -> bool) -> DefId {
    hir.root()
        .items
        .iter()
        .copied()
        .find(|&item| pred(hir.def(item)))
        .unwrap_or_else(|| panic!("fixture declares no top-level {what}"))
}

/// The `DefId` of the top-level struct, enum, trait, or function declared in `hir` under `name`.
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

/// The `DefId` of the first top-level `fun` declared in `hir`.
pub fn first_function(hir: &Hir) -> DefId {
    first_item(hir, "function", |def| matches!(def, OwnerNode::Function(_)))
}

/// The `DefId` of the first top-level `struct` declared in `hir`.
pub fn first_struct(hir: &Hir) -> DefId {
    first_item(hir, "struct", |def| matches!(def, OwnerNode::Struct(_)))
}

/// The `DefId` of the first top-level `trait` declared in `hir`.
pub fn first_trait(hir: &Hir) -> DefId {
    first_item(hir, "trait", |def| matches!(def, OwnerNode::Trait(_)))
}

/// The `DefId` of the first top-level `extend` block declared in `hir`.
pub fn first_extend(hir: &Hir) -> DefId {
    first_item(hir, "extend block", |def| {
        matches!(def, OwnerNode::Extend(_))
    })
}

/// The `DefId` of the first method in the first top-level `extend` block declared in `hir`.
pub fn first_extend_method(hir: &Hir) -> DefId {
    hir.extend(first_extend(hir)).methods[0]
}

/// The `return` statement in `def`'s body, and the id of the expression it returns.
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
