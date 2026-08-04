//! Tests that the resolver's traversal reaches every part of the tree, and that what it finds
//! there resolves to the right declaration.
//!
//! The bug these were written for is the one this walk is structurally prone to: every child
//! reference in the HIR is a bare [`HirId`], so handing an `if`'s `else_block` -- a
//! [`Node::Block`](crate::hir::Node::Block) -- to `resolve_expr` instead of `resolve_block`
//! compiles cleanly and only fails at run time. A traversal that never reaches a branch, or
//! reaches it through the wrong door, is invisible unless a test names something inside that
//! branch and checks the answer, which is what each test below does.

use crate::ast::interner::Interner;
use crate::hir::{DefId, ExprKind, Hir, HirId, Node, OwnerNode};
use crate::nameres::results::{NameResolutions, ValueRes};
use crate::testing::resolve_src;

/// Every expression in `def`'s arena that is a bare, single-segment path naming `name`.
///
/// Walking the arena directly rather than the tree is deliberate: a test asserting that the
/// traversal reaches a branch cannot use that same traversal to find what to assert on. The arena
/// holds every node the owner has, reachable or not.
fn refs_to(hir: &Hir, def: DefId, name: &str) -> Vec<HirId> {
    let symbol = Interner::intern(name);
    hir.arena(def)
        .nodes
        .iter()
        .filter_map(|node| match node {
            Node::Expr(expr) => match &expr.kind {
                ExprKind::Path(path) => match path.segments.as_slice() {
                    [segment] if segment.text == symbol => Some(expr.hir_id),
                    _ => None,
                },
                _ => None,
            },
            _ => None,
        })
        .collect()
}

/// The `HirId` of the parameter `name` in `def`'s parameter list.
fn param_named(hir: &Hir, def: DefId, name: &str) -> HirId {
    let OwnerNode::Function(function) = hir.def(def) else {
        panic!("fixture item is not a function");
    };
    let symbol = Interner::intern(name);
    *function
        .params
        .iter()
        .find(|&&id| hir.param(id).name.text == symbol)
        .unwrap_or_else(|| panic!("fixture function declares no parameter `{name}`"))
}

/// Asserts that the sole reference to `name` in the fixture's only function resolved to that
/// function's parameter of the same name.
///
/// "Sole" is asserted too: if the resolver had skipped the branch the reference sits in, the
/// lookup would simply be absent, and an assertion that quietly passes over an empty set would
/// be exactly the wrong shape for a test about a traversal that panics or misses nodes.
fn assert_names_param(hir: &Hir, nameres: &NameResolutions, def: DefId, name: &str) {
    let refs = refs_to(hir, def, name);
    assert_eq!(refs.len(), 1, "expected exactly one reference to `{name}`");
    let param = param_named(hir, def, name);

    match nameres.value(refs[0]) {
        Some(ValueRes::Local(id)) if id == param => {}
        other => panic!("`{name}` resolved to {other:?}, expected ValueRes::Local({param:?})"),
    }
}

/// The only function the fixture declares.
fn only_function(hir: &Hir) -> DefId {
    let items = &hir.root().items;
    assert_eq!(items.len(), 1, "fixture declares more than one item");
    items[0]
}

/// The crash this file exists for. Lowering wraps whatever follows `else` in a block, so the
/// `else_block` field names a `Node::Block`; resolving it as an expression panicked before it
/// ever got as far as `b`.
#[test]
fn if_else_resolves_names_in_both_branches() {
    let (hir, nameres) = resolve_src(
        "fun f(a: i32, b: i32) -> i32 {
             if true { a } else { b }
         }",
    );
    let def = only_function(&hir);

    assert_names_param(&hir, &nameres, def, "a");
    assert_names_param(&hir, &nameres, def, "b");
}

/// `else if` reaches the same field by a different route: the `else` block lowering synthesizes
/// holds a nested `If` as its tail expression, so the resolver has to descend a block, an
/// expression, and a block again to reach the last branch.
#[test]
fn else_if_chain_resolves_names_in_every_branch() {
    let (hir, nameres) = resolve_src(
        "fun f(a: i32, b: i32, c: i32) -> i32 {
             if true { a } else if false { b } else { c }
         }",
    );
    let def = only_function(&hir);

    assert_names_param(&hir, &nameres, def, "a");
    assert_names_param(&hir, &nameres, def, "b");
    assert_names_param(&hir, &nameres, def, "c");
}

/// `if let ... else` never reaches `ExprKind::If` at all -- lowering turns it into a `Match` whose
/// wildcard arm holds the `else` block -- but it routes through the same `lower_expr_as_block`, so
/// it is worth pinning that the arm's block is walked as a block here too.
#[test]
fn if_let_with_else_resolves_names_in_both_branches() {
    let (hir, nameres) = resolve_src(
        "fun f(a: i32, b: i32) -> i32 {
             if let .some(x) = a { x } else { b }
         }",
    );
    let def = only_function(&hir);

    // `a` is the scrutinee rather than a branch, but it shares the walk, so it is checked too.
    assert_names_param(&hir, &nameres, def, "a");
    assert_names_param(&hir, &nameres, def, "b");
}

/// A binding the matched arm's pattern introduces has to be visible in that arm's block. This is
/// the counterpart of the test above: it checks the *other* arm of the same desugaring.
#[test]
fn if_let_binding_is_visible_in_the_matched_branch() {
    let (hir, nameres) = resolve_src(
        "fun f(a: i32, b: i32) -> i32 {
             if let .some(x) = a { x } else { b }
         }",
    );
    let def = only_function(&hir);

    let refs = refs_to(&hir, def, "x");
    assert_eq!(refs.len(), 1, "expected exactly one reference to `x`");
    assert!(
        matches!(nameres.value(refs[0]), Some(ValueRes::Local(_))),
        "`x` did not resolve to the binding its pattern introduces"
    );
}
