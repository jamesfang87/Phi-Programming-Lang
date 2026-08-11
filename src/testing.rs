//! Test-only scaffolding for driving the pipeline over a source string.
//!
//! Every test module needs the same few lines to get from a `&str` to whatever stage it is
//! testing: clear the global diagnostic and interner state, register the source so spans can be
//! resolved, then run the passes. Those lines live here once.
//!
//! [`lex_src`] does the setup and stops, asserting nothing. Each helper after it builds on the
//! one before, so a test that means to exercise error recovery starts from [`lex_src`] and
//! drives the rest itself. [`parse_src`] and [`lower_src`] assert that no diagnostics were
//! raised; [`resolve_src`] is the exception, for the reason given on it.

use crate::ast::interner::Interner;
use crate::ast::{Ast, ParsedSrcFile};
use crate::diag::DiagCtx;
use crate::driver::source::{FileOrigin, SrcMap};
use crate::hir::lower::lower_unit;
use crate::hir::{DefId, Hir, HirId, Node, OwnerNode, StmtKind};
use crate::lexer::Lexer;
use crate::lexer::token::Token;
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
/// Runs AST-level name resolution first, as the real pipeline does, so `lower_unit` has
/// something to consume -- but only asserts on diagnostics up through parsing: many fixtures
/// name things that don't exist (a bare `fun f() { let x = y; }`, say), which is fine for
/// exercising lowering's shape but would make AST-level resolution report "not found". Those
/// diagnostics are left in `DiagCtx` rather than asserted on, same as `resolve_src` below.
pub fn lower_src(src: &str) -> Hir {
    let ast = Ast::new(vec![parse_src(src)]);
    let res = nameres::resolve(&ast);
    lower_unit(&ast, &res)
}

/// Lexes, parses, and lowers `src`, asserting no diagnostics were raised up to and including
/// lowering.
///
/// Name resolution diagnostics are left in [`DiagCtx`] rather than asserted on, because test
/// fixtures resolve without the core library and therefore always report all missing lang items.
/// A test that needs to verify a later pass clears diagnostics first, so language items are not
/// conflated with the pass's own errors.
///
/// Previously this function handed back a second `NameResolutions` value from a dedicated
/// HIR-based resolver, which ran only for type checking. That resolver no longer exists: every
/// `hir::Path` carries its resolution inline (see `crate::hir::path`), and `lower_src` produces
/// it as a side effect. This function now produces the same result as `lower_src`. The name is
/// retained because it matches the expectations of every downstream caller, and renaming would
/// require changes throughout the test infrastructure without adding precision.
pub fn resolve_src(src: &str) -> Hir {
    lower_src(src)
}

/// Runs the whole pipeline over `src`, type checking included, and hands back the messages type
/// checking reported.
///
/// Diagnostics are cleared after name resolution rather than asserted on, for the reason given on
/// [`resolve_src`]: a fixture is resolved without the core library, so name resolution reports the
/// whole set of missing lang items first and only what a fixture declares for itself resolves at
/// all. What comes back is therefore exactly what `typeck` had to say.
pub fn typeck_src(src: &str) -> Vec<String> {
    let hir = resolve_src(src);
    DiagCtx::clear();
    crate::typeck::check(&hir);

    DiagCtx::diagnostics()
        .into_iter()
        .map(|diagnostic| diagnostic.message)
        .collect()
}

/// Asserts that `src` type checks with nothing reported.
pub fn typeck_accepts(src: &str) {
    let reported = typeck_src(src);
    assert!(reported.is_empty(), "expected {src:?} to check: {reported:?}");
}

/// Asserts that `src` is rejected by exactly one diagnostic, whose message contains `needle`.
///
/// One rather than at least one, because a second diagnostic from the same fixture is usually a
/// cascade -- the thing this pass is careful to avoid -- and a test that tolerated it would stop
/// noticing.
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
    let OwnerNode::Module(module) = hir.def(hir.root_id()) else {
        unreachable!("root of a Module owner is always OwnerNode::Module");
    };
    module
        .items
        .iter()
        .copied()
        .find(|&item| pred(hir.def(item)))
        .unwrap_or_else(|| panic!("fixture declares no top-level {what}"))
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

/// The `DefId` of the first method in the first top-level `extend` block declared in `hir`.
pub fn first_extend_method(hir: &Hir) -> DefId {
    let extend = first_item(hir, "extend block", |def| {
        matches!(def, OwnerNode::Extend(_))
    });
    let OwnerNode::Extend(extend) = hir.def(extend) else {
        unreachable!("first_item only returns an extend block's DefId");
    };
    extend.methods[0]
}

/// The `return` statement in `def`'s body, and the id of the expression it returns.
pub fn find_return(hir: &Hir, def: DefId) -> (HirId, HirId) {
    let OwnerNode::Function(function) = hir.def(def) else {
        unreachable!("root of a Function owner is always OwnerNode::Function");
    };
    let block_id = function.block.expect("fixture function has a body");
    let Node::Block(block) = hir.node(block_id) else {
        unreachable!("a function's body is always a Node::Block");
    };

    for &stmt_id in &block.stmts {
        let Node::Stmt(stmt) = hir.node(stmt_id) else {
            unreachable!("Node which is not a stmt found in a block's statement list");
        };
        if let StmtKind::Return(Some(expr_id)) = stmt.kind {
            return (stmt_id, expr_id);
        }
    }
    panic!("fixture function has no `return <expr>;` statement");
}
