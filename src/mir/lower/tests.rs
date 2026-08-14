//! Unit tests for `mir::lower`, in the digging-helper style `hir/lower/tests.rs` established.

use crate::ast::BinaryOp;
use crate::driver::cli::Mode;
use crate::hir::Hir;
use crate::mir::lower::LoweredProgram;
use crate::mir::{AggregateKind, Body, Rvalue, StatementKind, TerminatorKind};
use crate::testing::{first_extend_method, first_function, resolve_src};
use crate::typeck::results::TypeResolutions;
use crate::typeck::tyctx::TyCtx;

/// Lexes, parses, resolves, lowers, type-checks, and MIR-lowers `src`, in debug profile.
/// Panics if any diagnostic was reported by type checking.
fn lower_mir_src(src: &str) -> (Hir, TyCtx, TypeResolutions, LoweredProgram) {
    lower_mir_src_with_mode(src, Mode::Debug)
}

fn lower_mir_src_with_mode(src: &str, mode: Mode) -> (Hir, TyCtx, TypeResolutions, LoweredProgram) {
    let hir = resolve_src(src);
    crate::diag::DiagCtx::clear();
    let checked = crate::typeck::check(&hir);
    let diagnostics = crate::diag::DiagCtx::diagnostics();
    assert!(
        diagnostics.is_empty(),
        "unexpected diagnostics for {src:?}: {diagnostics:?}"
    );
    let crate::typeck::TypeckOutput { mut tcx, types } = checked;
    let program = super::lower_program(&hir, &mut tcx, &types, mode);
    (hir, tcx, types, program)
}

/// The `Body` lowered for the first top-level function `resolve_src`d and MIR-lowered.
fn first_function_body<'a>(program: &'a LoweredProgram, hir: &Hir) -> &'a Body {
    let def_id = first_function(hir);
    program
        .bodies
        .get(&(def_id, None))
        .unwrap_or_else(|| panic!("no lowered body for the first function"))
}

#[test]
fn an_empty_function_returns_unit() {
    let (hir, _tcx, _types, program) = lower_mir_src("fun f() {}");
    let body = first_function_body(&program, &hir);
    assert_eq!(body.arg_count, 0);
    assert_eq!(body.local_decls.len(), 1, "just the return place");
    assert_eq!(body.basic_blocks.len(), 1);
    assert!(matches!(
        body.basic_blocks[0].terminator.kind,
        TerminatorKind::Return
    ));
}

#[test]
fn add_computes_the_sum_and_returns() {
    let (hir, _tcx, _types, program) =
        lower_mir_src("fun add(x: i32, y: i32) -> i32 { return x + y; }");
    let body = first_function_body(&program, &hir);
    assert_eq!(body.arg_count, 2);
    // Slots 0..=2 are the return place, `x`, and `y`; debug profile adds further temporaries
    // for the checked-arithmetic overflow test (see `checked_add_asserts_on_overflow` below).
    assert!(body.local_decls.len() >= 3);
    assert!(
        body.basic_blocks
            .iter()
            .any(|b| matches!(b.terminator.kind, TerminatorKind::Return)),
        "some block returns"
    );
    assert!(
        body.basic_blocks
            .iter()
            .flat_map(|b| &b.statements)
            .any(|s| matches!(
                &s.kind,
                StatementKind::Assign(_, Rvalue::CheckedBinaryOp(BinaryOp::Add, _, _))
            )),
        "a debug-profile body checks the addition for overflow"
    );
}

#[test]
fn an_if_expression_joins_both_branches() {
    let (hir, _tcx, _types, program) =
        lower_mir_src("fun f(x: i32) -> i32 { return if x < 0 { 0 } else { x }; }");
    let body = first_function_body(&program, &hir);
    // then-block, else-block, join-block, plus the entry block that switches on the condition.
    assert!(body.basic_blocks.len() >= 4);
    let switches = body
        .basic_blocks
        .iter()
        .filter(|b| matches!(b.terminator.kind, TerminatorKind::SwitchInt { .. }))
        .count();
    assert_eq!(switches, 1);
}

#[test]
fn a_release_profile_body_wraps_instead_of_checking() {
    let (hir, _tcx, _types, program) = lower_mir_src_with_mode(
        "fun add(x: i32, y: i32) -> i32 { return x + y; }",
        Mode::Release,
    );
    let body = first_function_body(&program, &hir);
    assert!(
        body.basic_blocks
            .iter()
            .flat_map(|b| &b.statements)
            .any(|s| matches!(
                &s.kind,
                StatementKind::Assign(_, Rvalue::BinaryOp(BinaryOp::Add, _, _))
            )),
        "a release-profile body emits a plain, unchecked BinaryOp"
    );
    assert!(
        !body
            .basic_blocks
            .iter()
            .flat_map(|b| &b.statements)
            .any(|s| matches!(
                &s.kind,
                StatementKind::Assign(_, Rvalue::CheckedBinaryOp(..))
            )),
        "a release-profile body never emits CheckedBinaryOp"
    );
}

#[test]
fn a_method_call_lowers_to_a_call_terminator() {
    let (hir, _tcx, _types, program) = lower_mir_src(
        "struct Rectangle { public l: f64, public w: f64 }
         extend Rectangle {
             public fun area(&self) -> f64 { return self.l * self.w; }
         }
         fun f(r: &Rectangle) -> f64 { return r.area(); }",
    );
    let body = first_function_body(&program, &hir);
    let method_def = first_extend_method(&hir);
    let calls = body
        .basic_blocks
        .iter()
        .filter_map(|b| match &b.terminator.kind {
            TerminatorKind::Call { func, .. } => Some(func),
            _ => None,
        })
        .count();
    assert_eq!(calls, 1, "exactly one Call terminator for r.area()");
    let _ = method_def;
}

#[test]
fn a_struct_literal_lowers_to_an_adt_aggregate() {
    let (hir, _tcx, _types, program) = lower_mir_src(
        "struct Point { public x: i32, public y: i32 }
         fun f() -> Point { return Point { x: 1, y: 2 }; }",
    );
    let body = first_function_body(&program, &hir);
    assert!(
        body.basic_blocks.iter().flat_map(|b| &b.statements).any(|s| matches!(
            &s.kind,
            StatementKind::Assign(_, Rvalue::Aggregate(kind, _)) if matches!(**kind, AggregateKind::Adt { .. })
        )),
        "the struct literal lowers to an Adt aggregate"
    );
}

#[test]
fn a_variant_match_switches_on_the_discriminant() {
    let (hir, _tcx, _types, program) = lower_mir_src(
        "struct Rectangle { public l: f64, public w: f64 }
         enum Shape { rectangle: Rectangle, circle: f64 }
         fun area(s: Shape) -> f64 {
             return match s {
                 .rectangle(r) => r.l * r.w,
                 .circle(radius) => radius,
             };
         }",
    );
    let body = first_function_body(&program, &hir);
    let switches = body
        .basic_blocks
        .iter()
        .filter(|b| matches!(b.terminator.kind, TerminatorKind::SwitchInt { .. }))
        .count();
    assert!(
        switches >= 1,
        "the match compiles to at least one SwitchInt"
    );
}

#[test]
fn a_capturing_closure_lowers_its_own_body() {
    let (hir, _tcx, _types, program) = lower_mir_src(
        "fun f() -> i32 {
             let x = 5;
             let add_x = |y: i32| -> i32 { return x + y; };
             return add_x(1);
         }",
    );
    // The closure gets its own Body, in addition to `f`'s.
    let closure_bodies = program
        .bodies
        .iter()
        .filter(|((def, _), _)| *def != first_function(&hir))
        .count();
    assert_eq!(closure_bodies, 1, "exactly one closure body was lowered");
}
