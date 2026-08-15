//! Test-only scaffolding for driving the pipeline over a source string.

use crate::ast::interner::Interner;
use crate::ast::{Ast, ParsedSrcFile};
use crate::diag::DiagCtx;
use crate::driver::source::{FileOrigin, SrcMap};
use crate::hir::lower::lower_program;
use crate::hir::{DefId, Hir, HirId, OwnerNode, StmtKind};
use crate::lexer::token::Token;
use crate::lexer::Lexer;
use crate::nameres;
use crate::parser::Parser;

// -----------------------------------------------------------------
// Driving the pipeline
// -----------------------------------------------------------------

/// Registers `src` as a file named `<test>` and lexes it, returning its tokens and the offset
/// `SrcMap` assigned it. That offset is what a sub-parser needs to build spans.
///
/// Unlike the helpers below, this asserts nothing about diagnostics: the lexer's own tests are
/// about the diagnostics it raises, and a test exercising parser recovery starts here too.
pub fn lex_src(src: &str) -> (Vec<Token>, usize) {
    DiagCtx::clear();
    Interner::clear();
    let chars: Vec<char> = src.chars().collect();
    let offset = SrcMap::add_file("<test>".to_string(), chars.clone(), FileOrigin::User);
    (Lexer::new(&chars, offset).tokenize(), offset)
}

/// Lexes and parses `src`, asserting no diagnostics were raised along the way.
pub fn parse_src(src: &str) -> ParsedSrcFile {
    let (tokens, offset) = lex_src(src);
    let unit = Parser::new().parse(&tokens, offset);
    assert_clean(src);
    unit
}

/// Lexes, parses, and lowers `src`, asserting no diagnostics were raised along the way.
///
/// Runs AST-level name resolution first, so `lower_unit` has something to consume. Only
/// diagnostics up through parsing are asserted. Many fixtures name things that don't exist
/// (a bare `fun f() { let x = y; }`, say), which is fine for exercising lowering but causes
/// AST-level resolution to report "not found". Those diagnostics are left in `DiagCtx` rather
/// than asserted, same as `resolve_src` below.
pub fn lower_src(src: &str) -> Hir {
    let ast = Ast::new(vec![parse_src(src)]);
    let res = nameres::resolve(&ast);
    lower_program(&ast, &res)
}

/// Lexes, parses, and lowers `src`, asserting no diagnostics were raised up to and including
/// lowering.
///
/// Name resolution diagnostics are left in [`DiagCtx`] rather than asserted on, because test
/// fixtures resolve without the core library and therefore always report all missing lang items.
/// A test that needs to verify a later pass clears diagnostics first, so language items are not
/// conflated with the pass's own errors.
///
/// Previously this function returned a second `NameResolutions` value from a dedicated
/// HIR-based resolver run only for type checking. That resolver no longer exists: every
/// `hir::Path` carries its resolution inline (see `crate::hir::path`), and `lower_src` produces
/// it as a side effect. This function now produces the same result as `lower_src`. The name
/// persists to match downstream caller expectations and avoid refactoring the test infrastructure.
pub fn resolve_src(src: &str) -> Hir {
    lower_src(src)
}

/// Runs the whole pipeline over `src`, type checking included, and hands back the messages type
/// checking reported.
///
/// Diagnostics are cleared after name resolution rather than checked, for the reason given on
/// [`resolve_src`]: a fixture is resolved without the core library, so name resolution reports the
/// whole set of missing lang items. Only what a fixture declares for itself resolves. The result
/// is exactly what type checking reported.
pub fn typeck_src(src: &str) -> Vec<String> {
    let hir = resolve_src(src);
    DiagCtx::clear();
    crate::typeck::check(&hir);

    DiagCtx::diagnostics()
        .into_iter()
        .map(|diagnostic| diagnostic.message)
        .collect()
}

/// [`typeck_src`] for a program spread across several files, each its own `module`.
///
/// A single fixture string is one file, and therefore one module, so nothing exercised through
/// `typeck_src` can cross a module boundary -- privacy, most notably. Mirrors
/// `nameres::tests::ast_from_files`: `DiagCtx` and the interner are cleared once up front, not
/// per file, so every file parses against the same interner and source map.
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
    let hir = lower_program(&ast, &res);

    DiagCtx::clear();
    crate::typeck::check(&hir);

    DiagCtx::diagnostics()
        .into_iter()
        .map(|diagnostic| diagnostic.message)
        .collect()
}

/// Runs the whole pipeline over `src` through Lowering #2 and monomorphization, in debug
/// profile, and hands back everything a MIR-level test needs to dig further: the `Hir` (for
/// `first_function` and friends), the `TyCtx` (to read a `Ty` a lowered `Body` carries), and the
/// finished, fully concrete `Body` per [`crate::mir::Instance`] actually used.
///
/// Panics if type checking reported anything -- the same "diagnostics-free by design" contract
/// `mir::lower`'s own module docs describe: this pass assumes its input already type-checks
/// cleanly, so a fixture meant to exercise a rejected program belongs with `typeck_rejects`
/// instead, not here.
pub fn lower_mir_src(
    src: &str,
) -> (
    Hir,
    crate::typeck::tyctx::TyCtx,
    crate::typeck::results::TypeResolutions,
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
    let program =
        crate::mir::lower::lower_program(&hir, &mut tcx, &types, crate::driver::cli::Mode::Debug);
    let instances = crate::mir::monomorphize::monomorphize(&hir, &mut tcx, &program);
    (hir, tcx, types, instances)
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
    first_item(hir, "extend block", |def| matches!(def, OwnerNode::Extend(_)))
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
