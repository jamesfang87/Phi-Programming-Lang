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
use crate::hir::{DefId, Hir, HirId, NameResolutions, Node, OwnerNode, StmtKind};
use crate::lexer::Lexer;
use crate::lexer::token::Token;
use crate::nameres::resolve;
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
pub fn lower_src(src: &str) -> Hir {
    lower_unit(&Ast::new(vec![parse_src(src)]))
}

/// Lexes, parses, lowers, and name-resolves `src`, asserting no diagnostics were raised up to
/// and including lowering.
///
/// Name resolution's own diagnostics are left in [`DiagCtx`] rather than asserted on: a fixture
/// is resolved without the core library, so every one of them reports the whole set of missing
/// lang items. A caller that goes on to assert about a later pass clears them first.
pub fn resolve_src(src: &str) -> (Hir, NameResolutions) {
    let hir = lower_src(src);
    let nameres = resolve(&hir);
    (hir, nameres)
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
