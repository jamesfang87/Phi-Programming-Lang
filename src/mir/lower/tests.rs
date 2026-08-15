//! Unit tests for `mir::lower`, in the digging-helper style `hir/lower/tests.rs` established.

use crate::ast::BinaryOp;
use crate::ast::interner::Interner;
use crate::driver::cli::Mode;
use crate::hir::{DefId, Hir, OwnerNode};
use crate::mir::lower::LoweredProgram;
use crate::mir::lower::ctx::{BodyLowerCtx, ExitObligation};
use crate::mir::{
    AggregateKind, AssertMessage, Body, CastKind, ConstKind, Constant, Local, Operand, PlaceElem,
    Rvalue, StatementKind, TerminatorKind,
};
use crate::nameres::PrimTy;
use crate::testing::{first_extend_method, first_function, resolve_src};
use crate::typeck::results::TypeResolutions;
use crate::typeck::ty::TyKind;
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

/// A bare `return;`, with no expression, lowers exactly like falling off the end of the function
/// does: `lower_return`'s `None` arm assigns the unit value into the return place itself, rather
/// than leaving it for the implicit `Return` `lower_body_block` appends.
#[test]
fn a_bare_return_assigns_unit_into_the_return_place() {
    let (hir, _tcx, _types, program) = lower_mir_src("fun f() { return; }");
    let body = first_function_body(&program, &hir);
    let assigns_unit_to_return_place =
        body.basic_blocks
            .iter()
            .flat_map(|b| &b.statements)
            .any(|s| {
                matches!(
                    &s.kind,
                    StatementKind::Assign(place, Rvalue::Aggregate(kind, operands))
                        if place.local == crate::mir::Local::RETURN_PLACE
                            && place.projection.is_empty()
                            && matches!(**kind, AggregateKind::Tuple)
                            && operands.is_empty()
                )
            });
    assert!(
        assigns_unit_to_return_place,
        "a bare `return;` assigns the unit value, `()`"
    );
    assert!(
        body.basic_blocks
            .iter()
            .any(|b| matches!(b.terminator.kind, TerminatorKind::Return)),
        "some block still ends in a Return terminator"
    );
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

// -----------------------------------------------------------------
// Digging helpers
// -----------------------------------------------------------------

/// The `DefId` of the top-level function named `name`. [`first_function`] only reaches whichever
/// one was declared first; several tests below need a specific one out of several.
fn find_function(hir: &Hir, name: &str) -> DefId {
    hir.root()
        .items
        .iter()
        .copied()
        .find(|&id| {
            matches!(hir.def(id), OwnerNode::Function(f) if Interner::resolve(f.name.text) == name)
        })
        .unwrap_or_else(|| panic!("no top-level function named {name:?}"))
}

/// Every `Local` a `StorageLive` statement names, in the order those statements occur across
/// `body`'s basic blocks. Meaningful as an execution order only for a body with no branching, the
/// only shape the tests below use it for -- see [`crate::mir::lower::ctx::BodyLowerCtx::new_block`]'s
/// own doc comment on why blocks are otherwise built in control-flow order rather than list order.
fn storage_live_order(body: &Body) -> Vec<Local> {
    body.basic_blocks
        .iter()
        .flat_map(|b| &b.statements)
        .filter_map(|s| match s.kind {
            StatementKind::StorageLive(local) => Some(local),
            _ => None,
        })
        .collect()
}

/// The `StorageDead` counterpart of [`storage_live_order`].
fn storage_dead_order(body: &Body) -> Vec<Local> {
    body.basic_blocks
        .iter()
        .flat_map(|b| &b.statements)
        .filter_map(|s| match s.kind {
            StatementKind::StorageDead(local) => Some(local),
            _ => None,
        })
        .collect()
}

/// Every `Assert` terminator's own message, across `body`'s whole block list.
fn assert_messages(body: &Body) -> Vec<&AssertMessage> {
    body.basic_blocks
        .iter()
        .filter_map(|b| match &b.terminator.kind {
            TerminatorKind::Assert { msg, .. } => Some(msg),
            _ => None,
        })
        .collect()
}

/// Every `CheckedBinaryOp`'s own operator, across `body`'s whole statement list.
fn checked_binary_ops(body: &Body) -> Vec<BinaryOp> {
    body.basic_blocks
        .iter()
        .flat_map(|b| &b.statements)
        .filter_map(|s| match &s.kind {
            StatementKind::Assign(_, Rvalue::CheckedBinaryOp(op, ..)) => Some(*op),
            _ => None,
        })
        .collect()
}

/// The `DefId` a `Call` terminator's callee names, for every direct call in `body` -- a call to a
/// named function, reified as `Operand::Constant(FnDef(..))`, as opposed to an indirect call
/// through a place, which names no single `DefId` at all.
fn call_callees(body: &Body) -> Vec<DefId> {
    body.basic_blocks
        .iter()
        .filter_map(|b| match &b.terminator.kind {
            TerminatorKind::Call {
                func:
                    Operand::Constant(Constant {
                        kind: ConstKind::FnDef(def, ..),
                        ..
                    }),
                ..
            } => Some(*def),
            _ => None,
        })
        .collect()
}

// -----------------------------------------------------------------
// Checked arithmetic, division, and casts
// -----------------------------------------------------------------

/// The existing `add_computes_the_sum_and_returns` test above only exercises `+`; this covers the
/// other two operators `lower_binary_op_into` checks in a debug-profile body.
#[test]
fn checked_arithmetic_covers_add_sub_and_mul() {
    let (hir, _tcx, _types, program) = lower_mir_src(
        "fun f(x: i32, y: i32) -> i32 {
             let a = x + y;
             let b = x - y;
             let c = x * y;
             return a + b + c;
         }",
    );
    let body = first_function_body(&program, &hir);
    let ops = checked_binary_ops(body);
    assert!(
        ops.contains(&BinaryOp::Add),
        "a debug body checks `+` for overflow"
    );
    assert!(
        ops.contains(&BinaryOp::Sub),
        "a debug body checks `-` for overflow"
    );
    assert!(
        ops.contains(&BinaryOp::Mul),
        "a debug body checks `*` for overflow"
    );
}

#[test]
fn comparisons_are_never_checked_or_wrapped() {
    let (hir, _tcx, _types, program) =
        lower_mir_src("fun f(x: i32, y: i32) -> bool { return x < y; }");
    let body = first_function_body(&program, &hir);
    assert!(
        checked_binary_ops(body).is_empty(),
        "only +, -, and * are ever wrapped in CheckedBinaryOp; a comparison is not arithmetic"
    );
    let has_plain_lt = body
        .basic_blocks
        .iter()
        .flat_map(|b| &b.statements)
        .any(|s| {
            matches!(
                &s.kind,
                StatementKind::Assign(_, Rvalue::BinaryOp(BinaryOp::Lt, ..))
            )
        });
    assert!(has_plain_lt, "`<` still lowers to a plain BinaryOp");
}

#[test]
fn division_by_zero_inserts_an_assert_and_is_never_checked_for_overflow() {
    let (hir, _tcx, _types, program) =
        lower_mir_src("fun f(x: i32, y: i32) -> i32 { return x / y; }");
    let body = first_function_body(&program, &hir);
    assert!(
        assert_messages(body)
            .iter()
            .any(|m| matches!(m, AssertMessage::DivisionByZero(_))),
        "an integer division inserts a zero-check assert ahead of the division itself"
    );
    assert!(
        checked_binary_ops(body).is_empty(),
        "a division has no overflow to check, so it is never wrapped in CheckedBinaryOp"
    );
    let has_plain_div = body
        .basic_blocks
        .iter()
        .flat_map(|b| &b.statements)
        .any(|s| {
            matches!(
                &s.kind,
                StatementKind::Assign(_, Rvalue::BinaryOp(BinaryOp::Div, ..))
            )
        });
    assert!(
        has_plain_div,
        "the division itself is a plain BinaryOp, past the assert"
    );
}

#[test]
fn remainder_by_zero_inserts_an_assert() {
    let (hir, _tcx, _types, program) =
        lower_mir_src("fun f(x: i32, y: i32) -> i32 { return x % y; }");
    let body = first_function_body(&program, &hir);
    assert!(
        assert_messages(body)
            .iter()
            .any(|m| matches!(m, AssertMessage::RemainderByZero(_))),
        "an integer remainder inserts its own zero-check assert, distinct from division's"
    );
}

/// Division-by-zero is a memory-safety check, not an overflow check, so unlike `CheckedBinaryOp`
/// it is not gated on `self.mode == Mode::Debug` in `lower_binary_op_into` -- it must survive a
/// release build.
#[test]
fn division_by_zero_assert_survives_release_mode() {
    let (hir, _tcx, _types, program) = lower_mir_src_with_mode(
        "fun f(x: i32, y: i32) -> i32 { return x / y; }",
        Mode::Release,
    );
    let body = first_function_body(&program, &hir);
    assert!(
        assert_messages(body)
            .iter()
            .any(|m| matches!(m, AssertMessage::DivisionByZero(_))),
        "release profile still inserts the division-by-zero assert"
    );
    assert!(
        checked_binary_ops(body).is_empty(),
        "release profile still never checks for overflow"
    );
}

/// `lower_binary_op_into`'s `is_int`/`is_flt` split means neither the zero-check assert nor
/// `CheckedBinaryOp` ever applies to a float operand -- IEEE 754 already defines `x / 0.0`.
#[test]
fn float_division_has_no_assert_and_is_never_checked() {
    let (hir, _tcx, _types, program) =
        lower_mir_src("fun f(x: f64, y: f64) -> f64 { return x / y; }");
    let body = first_function_body(&program, &hir);
    assert!(
        assert_messages(body).is_empty(),
        "a float division inserts no zero-check assert at all"
    );
    assert!(checked_binary_ops(body).is_empty());
    let has_plain_div = body
        .basic_blocks
        .iter()
        .flat_map(|b| &b.statements)
        .any(|s| {
            matches!(
                &s.kind,
                StatementKind::Assign(_, Rvalue::BinaryOp(BinaryOp::Div, ..))
            )
        });
    assert!(
        has_plain_div,
        "the division itself still lowers to a plain BinaryOp"
    );
}

// -----------------------------------------------------------------
// Places: field access, indexing, casts
// -----------------------------------------------------------------

#[test]
fn field_access_through_a_reference_inserts_a_deref_projection() {
    let (hir, _tcx, _types, program) = lower_mir_src(
        "struct Point { public x: i32, public y: i32 }
         fun f(p: &Point) -> i32 { return p.x; }",
    );
    let body = first_function_body(&program, &hir);
    let found = body
        .basic_blocks
        .iter()
        .flat_map(|b| &b.statements)
        .any(|s| match &s.kind {
            StatementKind::Assign(_, Rvalue::Use(Operand::Copy(place) | Operand::Move(place))) => {
                place.projection == [PlaceElem::Deref, PlaceElem::Field(0)]
            }
            _ => false,
        });
    assert!(
        found,
        "`p.x` through a `&Point` derefs `p` before projecting to field 0"
    );
}

#[test]
fn array_indexing_inserts_a_bounds_check_and_reads_the_length() {
    let (hir, _tcx, _types, program) =
        lower_mir_src("fun f(a: [i32; 4], i: i32) -> i32 { return a[i]; }");
    let body = first_function_body(&program, &hir);
    assert!(
        assert_messages(body)
            .iter()
            .any(|m| matches!(m, AssertMessage::BoundsCheck { .. })),
        "indexing a fixed-size array by a runtime value inserts a bounds-check assert"
    );
    let reads_len = body
        .basic_blocks
        .iter()
        .flat_map(|b| &b.statements)
        .any(|s| matches!(&s.kind, StatementKind::Assign(_, Rvalue::Len(_))));
    assert!(
        reads_len,
        "the bounds check reads the array's own length via Rvalue::Len"
    );
    let indexes = body
        .basic_blocks
        .iter()
        .flat_map(|b| &b.statements)
        .any(|s| match &s.kind {
            StatementKind::Assign(_, Rvalue::Use(Operand::Copy(place) | Operand::Move(place))) => {
                matches!(place.projection.last(), Some(PlaceElem::Index(_)))
            }
            _ => false,
        });
    assert!(
        indexes,
        "the checked read itself projects through PlaceElem::Index"
    );
}

#[test]
fn a_primitive_cast_produces_a_cast_rvalue_with_the_target_type() {
    let (hir, tcx, _types, program) = lower_mir_src("fun f(x: i32) -> i64 { return x as i64; }");
    let body = first_function_body(&program, &hir);
    let found = body
        .basic_blocks
        .iter()
        .flat_map(|b| &b.statements)
        .find_map(|s| match &s.kind {
            StatementKind::Assign(
                _,
                Rvalue::Cast {
                    kind: CastKind::Primitive,
                    ty,
                    ..
                },
            ) => Some(*ty),
            _ => None,
        });
    let ty = found.expect("`x as i64` lowers to a Cast rvalue");
    assert!(
        matches!(tcx.kind(ty), TyKind::Primitive(PrimTy::I64)),
        "the cast's own recorded type is the target, i64, not the operand's, i32"
    );
}

#[test]
fn a_named_function_used_as_a_value_is_reified() {
    let (hir, _tcx, _types, program) = lower_mir_src(
        "fun f() -> fun(i32, i32) -> i32 { return add; }
         fun add(x: i32, y: i32) -> i32 { return x + y; }",
    );
    let body = first_function_body(&program, &hir);
    let found = body
        .basic_blocks
        .iter()
        .flat_map(|b| &b.statements)
        .any(|s| {
            matches!(
                &s.kind,
                StatementKind::Assign(
                    _,
                    Rvalue::Cast {
                        kind: CastKind::ReifyFnPointer,
                        ..
                    }
                )
            )
        });
    assert!(
        found,
        "naming `add` without calling it reifies it as a function-pointer value"
    );
}

#[test]
fn an_indirect_call_through_a_function_typed_place_moves_the_callee() {
    let (hir, _tcx, _types, program) = lower_mir_src(
        "fun apply(f: fun(i32, i32) -> i32, x: i32, y: i32) -> i32 { return f(x, y); }
         fun add(x: i32, y: i32) -> i32 { return x + y; }",
    );
    let body = first_function_body(&program, &hir);
    let indirect = body.basic_blocks.iter().any(|b| {
        matches!(
            &b.terminator.kind,
            TerminatorKind::Call {
                func: Operand::Move(_),
                ..
            }
        )
    });
    assert!(
        indirect,
        "calling through a `fun`-typed parameter moves its place; it names no single DefId, \
         unlike a direct call to a named function"
    );
}

// -----------------------------------------------------------------
// Logical short-circuiting
// -----------------------------------------------------------------

#[test]
fn logical_and_short_circuits_without_evaluating_the_rhs() {
    let (hir, _tcx, _types, program) =
        lower_mir_src("fun f(x: bool, y: bool) -> bool { return x && y; }");
    let body = first_function_body(&program, &hir);
    let switches = body
        .basic_blocks
        .iter()
        .filter(|b| matches!(b.terminator.kind, TerminatorKind::SwitchInt { .. }))
        .count();
    assert_eq!(
        switches, 1,
        "`&&` branches on its left operand exactly once"
    );
    let short_circuits_to_false = body
        .basic_blocks
        .iter()
        .flat_map(|b| &b.statements)
        .any(|s| {
            matches!(
                &s.kind,
                StatementKind::Assign(
                    _,
                    Rvalue::Use(Operand::Constant(Constant {
                        kind: ConstKind::Bool(false),
                        ..
                    }))
                )
            )
        });
    assert!(
        short_circuits_to_false,
        "a false left operand short-circuits `&&` straight to false"
    );
}

#[test]
fn logical_or_short_circuits_without_evaluating_the_rhs() {
    let (hir, _tcx, _types, program) =
        lower_mir_src("fun f(x: bool, y: bool) -> bool { return x || y; }");
    let body = first_function_body(&program, &hir);
    let switches = body
        .basic_blocks
        .iter()
        .filter(|b| matches!(b.terminator.kind, TerminatorKind::SwitchInt { .. }))
        .count();
    assert_eq!(
        switches, 1,
        "`||` branches on its left operand exactly once"
    );
    let short_circuits_to_true = body
        .basic_blocks
        .iter()
        .flat_map(|b| &b.statements)
        .any(|s| {
            matches!(
                &s.kind,
                StatementKind::Assign(
                    _,
                    Rvalue::Use(Operand::Constant(Constant {
                        kind: ConstKind::Bool(true),
                        ..
                    }))
                )
            )
        });
    assert!(
        short_circuits_to_true,
        "a true left operand short-circuits `||` straight to true"
    );
}

// -----------------------------------------------------------------
// Dead code
// -----------------------------------------------------------------

/// A `match` with no arms is `Never`-typed (see `typeck::expr::check_match`), so this is a
/// surface-syntax way to construct a `Never`-typed *statement*, exactly the case
/// `BodyLowerCtx::lower_block`'s own doc comment calls out: "an expression statement whose own
/// type is `Never`" makes every statement after it dead code that lowering never even visits.
#[test]
fn dead_code_after_a_never_typed_statement_is_never_lowered() {
    let (hir, _tcx, _types, program) = lower_mir_src(
        "fun f(x: i32) {
             match x {};
             marker();
         }
         fun marker() {}",
    );
    let body = first_function_body(&program, &hir);
    let marker_def = find_function(&hir, "marker");
    assert!(
        !call_callees(body).contains(&marker_def),
        "`marker()` is lexically after a Never-typed statement, so it is never lowered at all"
    );
    assert!(
        body.basic_blocks
            .iter()
            .any(|b| matches!(b.terminator.kind, TerminatorKind::Unreachable)),
        "a match with no arms compiles straight to an Unreachable terminator"
    );
}

// -----------------------------------------------------------------
// `defer` and `with`: exit obligations
// -----------------------------------------------------------------

#[test]
fn defers_run_in_reverse_declaration_order() {
    let (hir, _tcx, _types, program) = lower_mir_src(
        "fun f() {
             defer a();
             defer b();
         }
         fun a() {}
         fun b() {}",
    );
    let body = first_function_body(&program, &hir);
    let a_def = find_function(&hir, "a");
    let b_def = find_function(&hir, "b");
    let order = call_callees(body);
    let a_pos = order
        .iter()
        .position(|&d| d == a_def)
        .expect("`a` is called somewhere");
    let b_pos = order
        .iter()
        .position(|&d| d == b_def)
        .expect("`b` is called somewhere");
    assert!(
        b_pos < a_pos,
        "the later-registered defer, `b`, must run before the earlier one, `a`: {order:?}"
    );
}

#[test]
fn with_lend_storage_is_freed_in_reverse_of_acquisition_order() {
    let (hir, _tcx, _types, program) = lower_mir_src(
        "fun f() {
             let a = 1;
             let mut b = 2;
             with x = &a, y = &mut b {
                 noop();
             }
         }
         fun noop() {}",
    );
    let body = first_function_body(&program, &hir);
    let live = storage_live_order(body);
    let dead = storage_dead_order(body);
    let expected: Vec<Local> = live.iter().rev().copied().collect();
    assert_eq!(
        live.len(),
        4,
        "`a`, `b`, and the two with-lends, `x` and `y`, are each made live exactly once"
    );
    assert_eq!(
        dead, expected,
        "every StorageDead runs in the exact reverse of StorageLive's own order, the same \
         last-in-first-out discipline an ordinary stack has"
    );
}

/// A bare, irrefutable `let x = 1;` -- no destructuring, no `else` -- allocates exactly one local
/// for `x`, with `1` lowered directly into it. `lower_let` special-cases this shape specifically
/// to skip the general scrutinee-then-`bind_pat` walk: there is no structure to test and nothing
/// else a scrutinee's own place would need to be projected out of. Every other pattern shape (a
/// `Tuple`, or anything paired with an `else`) still goes through that general path, and still
/// needs the scrutinee -- see `with_lend_storage_is_freed_in_reverse_of_acquisition_order`, just
/// above, which sidesteps a `let` binding for exactly that reason: it means to count only the
/// two with-lends' own locals.
#[test]
fn a_plain_let_binding_allocates_exactly_one_local() {
    let (hir, _tcx, _types, program) = lower_mir_src("fun f() { let x = 1; }");
    let body = first_function_body(&program, &hir);
    assert_eq!(body.arg_count, 0);
    assert_eq!(
        body.local_decls.len(),
        2,
        "just the return place and `x` itself, no separate scrutinee temp"
    );
    assert_eq!(
        storage_live_order(body).len(),
        1,
        "one StorageLive, for `x` itself"
    );
}

/// `BodyLowerCtx::continue_target`'s own doc comment states the contract this exercises: a
/// `continue` "leaves every block the loop's own body opened, and no block outside the loop".
///
/// This drives `BodyLowerCtx`'s own scope-stack bookkeeping directly, rather than through surface
/// syntax and `call_callees` counting, the way most of the tests around it do. A syntactic
/// `while`/`defer` fixture cannot isolate this cleanly: `lower_block`'s own natural-exit replay
/// runs unconditionally, even after a `continue` has already diverged that same block (see its own
/// doc comment), so the loop body's own obligations end up replayed twice over regardless -- once
/// live, by `continue` itself, and once more into dead code nothing ever reaches. That duplication
/// is real (and, on its own, harmless, since the second copy is unreachable), but it drowns out the
/// one thing this test means to isolate: that `continue_target`'s own obligation list stops at the
/// loop's own scope depth and does not reach past it to an obligation registered outside the loop.
#[test]
fn continue_target_only_returns_obligations_registered_since_the_loop_was_entered() {
    let (hir, mut tcx, types, _program) = lower_mir_src("fun f() {}");
    let def_id = first_function(&hir);
    let unit_ty = tcx.unit();
    let span = hir.def(def_id).span();

    let mut ctx = BodyLowerCtx::new(&hir, &mut tcx, &types, Mode::Debug, def_id, None);
    ctx.push_block_scope();
    let outer_local = ctx.new_temp(unit_ty, span);
    ctx.register_exit_obligation(ExitObligation::StorageDead(outer_local));

    let break_block = ctx.new_block();
    let continue_block = ctx.new_block();
    ctx.push_loop(break_block, continue_block);
    ctx.push_block_scope();
    let inner_local = ctx.new_temp(unit_ty, span);
    ctx.register_exit_obligation(ExitObligation::StorageDead(inner_local));

    let (target, obligations) = ctx.continue_target().expect("a loop is on the stack");
    assert_eq!(target, continue_block);
    assert_eq!(
        obligations.len(),
        1,
        "only the loop body's own obligation should replay, not the outer one too: {obligations:?}"
    );
    assert!(
        matches!(obligations[0], ExitObligation::StorageDead(local) if local == inner_local),
        "the one obligation that does replay is the loop body's own, not the outer scope's"
    );
}

// -----------------------------------------------------------------
// Pattern matching
// -----------------------------------------------------------------

/// `BodyLowerCtx::peek_block_scope`'s own doc comment describes exactly this: a guard's failure
/// path must clean up the arm's own bindings before falling through to the next candidate, but
/// without popping the scope, since the same arm's success path (lowered right after, in the same
/// sequential pass) still needs it open. So `n`'s `StorageDead` should appear twice: once on the
/// guard-failure path, once more on the success path.
#[test]
fn a_guard_failure_cleans_up_the_arms_bindings_before_falling_through() {
    let (hir, _tcx, _types, program) = lower_mir_src(
        "fun f(x: i32) -> i32 {
             return match x {
                 n if n > 0 => n,
                 _ => 0,
             };
         }",
    );
    let body = first_function_body(&program, &hir);
    let live = storage_live_order(body);
    assert_eq!(
        live.len(),
        1,
        "the only binding introduced anywhere in this match is `n`"
    );
    let n = live[0];
    let dead_for_n = storage_dead_order(body).iter().filter(|&&l| l == n).count();
    assert_eq!(
        dead_for_n, 2,
        "`n` is cleaned up once on the guard's failure path and once more on its own success path"
    );
}

#[test]
fn a_tuple_patterns_elements_are_tested_independently() {
    let (hir, _tcx, _types, program) = lower_mir_src(
        "fun f(p: (i32, i32)) -> i32 {
             return match p {
                 (0, 0) => 1,
                 _ => 0,
             };
         }",
    );
    let body = first_function_body(&program, &hir);
    let switches = body
        .basic_blocks
        .iter()
        .filter(|b| matches!(b.terminator.kind, TerminatorKind::SwitchInt { .. }))
        .count();
    assert_eq!(
        switches, 2,
        "each tuple element's own literal test branches independently"
    );
    let field_indices: Vec<u32> = body
        .basic_blocks
        .iter()
        .flat_map(|b| &b.statements)
        .filter_map(|s| match &s.kind {
            StatementKind::Assign(_, Rvalue::BinaryOp(BinaryOp::Eq, Operand::Copy(place), _)) => {
                match place.projection.last() {
                    Some(PlaceElem::Field(i)) => Some(*i),
                    _ => None,
                }
            }
            _ => None,
        })
        .collect();
    assert_eq!(
        field_indices,
        vec![0, 1],
        "the two literal tests read field 0, then field 1, matching the tuple's own element order"
    );
}

// -----------------------------------------------------------------
// Aggregates: declared field order, not source order
// -----------------------------------------------------------------

#[test]
fn a_struct_literals_fields_are_ordered_by_declaration_not_by_source() {
    let (hir, _tcx, _types, program) = lower_mir_src(
        "struct Point { public x: i32, public y: i32 }
         fun f() -> Point { return Point { y: 2, x: 1 }; }",
    );
    let body = first_function_body(&program, &hir);
    let operands = body
        .basic_blocks
        .iter()
        .flat_map(|b| &b.statements)
        .find_map(|s| match &s.kind {
            StatementKind::Assign(_, Rvalue::Aggregate(kind, operands))
                if matches!(**kind, AggregateKind::Adt { .. }) =>
            {
                Some(operands.clone())
            }
            _ => None,
        })
        .expect("the struct literal lowers to an Adt aggregate");
    let values: Vec<i128> = operands
        .iter()
        .map(|op| match op {
            Operand::Constant(Constant {
                kind: ConstKind::Int(v),
                ..
            }) => *v,
            other => panic!("expected an int constant operand, got {other:?}"),
        })
        .collect();
    assert_eq!(
        values,
        vec![1, 2],
        "`x`'s value (1) comes first, even though `y` was written first in the literal"
    );
}

#[test]
fn a_record_variants_fields_are_ordered_by_declaration_not_by_source() {
    let (hir, _tcx, _types, program) = lower_mir_src(
        "enum Shape { rect: { w: f64, h: f64 } }
         fun f() -> Shape { return .rect { h: 2.0, w: 1.0 }; }",
    );
    let body = first_function_body(&program, &hir);
    let operands = body
        .basic_blocks
        .iter()
        .flat_map(|b| &b.statements)
        .find_map(|s| match &s.kind {
            StatementKind::Assign(_, Rvalue::Aggregate(kind, operands))
                if matches!(**kind, AggregateKind::Adt { .. }) =>
            {
                Some(operands.clone())
            }
            _ => None,
        })
        .expect("the variant literal lowers to an Adt aggregate");
    let values: Vec<f64> = operands
        .iter()
        .map(|op| match op {
            Operand::Constant(Constant {
                kind: ConstKind::Float(v),
                ..
            }) => *v,
            other => panic!("expected a float constant operand, got {other:?}"),
        })
        .collect();
    assert_eq!(
        values,
        vec![1.0, 2.0],
        "`w`'s value (1.0) comes first, even though `h` was written first in the literal"
    );
}

// -----------------------------------------------------------------
// Closures
// -----------------------------------------------------------------

#[test]
fn a_variable_read_twice_in_a_closure_is_captured_only_once() {
    let (hir, _tcx, _types, program) = lower_mir_src(
        "fun f() -> i32 {
             let x = 5;
             let g = || x + x;
             return g();
         }",
    );
    let body = first_function_body(&program, &hir);
    let operands = body
        .basic_blocks
        .iter()
        .flat_map(|b| &b.statements)
        .find_map(|s| match &s.kind {
            StatementKind::Assign(_, Rvalue::Aggregate(kind, operands))
                if matches!(**kind, AggregateKind::Closure { .. }) =>
            {
                Some(operands.clone())
            }
            _ => None,
        })
        .expect("the closure literal lowers to a Closure aggregate");
    assert_eq!(
        operands.len(),
        1,
        "`x` is captured once, however many times the closure reads it"
    );
}

/// A closure's environment local is unconditional, per `lower_closure_body`'s own doc comment:
/// "whether or not this particular closure captures anything -- a uniform calling convention is
/// simpler than a conditional one".
#[test]
fn a_closure_with_no_captures_still_gets_an_environment_local() {
    let (hir, tcx, _types, program) = lower_mir_src(
        "fun f() -> i32 {
             let g = || 1;
             return g();
         }",
    );
    let closure_body = program
        .bodies
        .iter()
        .find(|((def, _), _)| *def != first_function(&hir))
        .map(|(_, body)| body)
        .expect("the closure gets its own Body");
    assert_eq!(
        closure_body.arg_count, 1,
        "the environment local is the closure's own implicit first argument, even with no \
         captures at all"
    );
    let env_ty = closure_body.local_decls[1].ty;
    assert!(
        matches!(tcx.kind(env_ty), TyKind::Tuple(elems) if elems.is_empty()),
        "an empty capture list still gets a zero-element-tuple-typed environment local"
    );
}

// -----------------------------------------------------------------
// Compound assignment
// -----------------------------------------------------------------

/// `ExprKind::AssignOp`'s own lowering calls `lower_place(lhs)` exactly once and reuses the
/// resulting `Place` for both the read and the write, rather than re-lowering `lhs` a second time
/// for the write -- otherwise a side-effecting index expression like `idx()` here would run twice.
#[test]
fn a_compound_assignments_index_target_is_evaluated_only_once() {
    let (hir, _tcx, _types, program) = lower_mir_src(
        "fun f(a: [i32; 4]) -> i32 {
             let mut arr = a;
             arr[idx()] += 1;
             return arr[0];
         }
         fun idx() -> i32 { return 0; }",
    );
    let body = first_function_body(&program, &hir);
    let idx_def = find_function(&hir, "idx");
    let calls = call_callees(body).iter().filter(|&&d| d == idx_def).count();
    assert_eq!(
        calls, 1,
        "`arr[idx()] += 1` calls `idx` exactly once, not once to read and once to write"
    );
}

// -----------------------------------------------------------------
// `any`-mode specialization
// -----------------------------------------------------------------

/// The README's own `min` example: a definition returning `any i32` is lowered once per mode a
/// call site actually demands (see `mir::lower`'s module docs), and a parameter declared `any i32`
/// resolves concretely under that mode -- `&i32` under `AnyMode::Ref`, matching `&min(a, b)`'s own
/// `&`.
#[test]
fn an_any_returning_calls_argument_is_borrowed_to_match_the_call_sites_mode() {
    let (hir, tcx, _types, program) = lower_mir_src(
        "fun min(x: any i32, y: any i32) -> any i32 {
             return if x < y { x } else { y };
         }
         fun f(a: i32, b: i32) -> i32 {
             let r = &min(a, b);
             return a;
         }",
    );
    let min_def = find_function(&hir, "min");
    let ref_body = program
        .bodies
        .get(&(min_def, Some(crate::mir::AnyMode::Ref)))
        .expect("calling `min` under `&` discovers its Ref-specialized body");
    let param_ty = ref_body.local_decls[1].ty;
    assert!(
        matches!(tcx.kind(param_ty), TyKind::Ref { .. }),
        "under AnyMode::Ref, `x`'s own `any i32` parameter resolves to `&i32`"
    );
}

// -----------------------------------------------------------------
// Debug names
// -----------------------------------------------------------------

/// `LocalDecl::name`'s own doc comment says it "is the source name of a user-written local, for
/// `--emit-debug` dumps and diagnostics": `bind_pat`'s `PatKind::Binding` arm threads the
/// pattern's own name through to `new_local`, rather than passing `None`, so a `let`-bound local
/// is no longer indistinguishable from a compiler-introduced temporary in `--emit-debug`'s own
/// MIR dump (`driver::emit_debug::print_mir` prints `_` only for a `None` name).
#[test]
fn a_let_bound_local_carries_its_declared_name() {
    let (hir, _tcx, _types, program) = lower_mir_src("fun f() { let x = 1; }");
    let body = first_function_body(&program, &hir);
    assert_eq!(body.arg_count, 0);
    // Slot 0 is the return place; slot 1 is `x` itself -- the fast, scrutinee-free path a bare
    // `Binding` pattern takes (see `a_plain_let_binding_allocates_exactly_one_local`).
    let x_decl = &body.local_decls[1];
    let name = x_decl
        .name
        .expect("a let-bound local carries its declared name");
    assert_eq!(Interner::resolve(name.text), "x");
}

/// `lower_with_lend` threads a lend's own pattern name through the same way.
#[test]
fn a_with_lends_local_carries_its_declared_name() {
    let (hir, _tcx, _types, program) =
        lower_mir_src("fun f(a: i32) { with x = &a { noop(); } } fun noop() {}");
    let body = first_function_body(&program, &hir);
    // Slot 0 is the return place; slot 1 is `a`, the parameter; slot 2 is `x`, the lend.
    let x_decl = &body.local_decls[2];
    let name = x_decl
        .name
        .expect("a with-bound local carries its declared name");
    assert_eq!(Interner::resolve(name.text), "x");
}
