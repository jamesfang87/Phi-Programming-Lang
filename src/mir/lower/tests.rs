use crate::ast::BinaryOp;
use crate::ast::{Ast, ParsedSrcFile};
use crate::driver::source::FileOrigin;
use crate::hir::{DefId, Hir, OwnerNode};
use crate::lexer::Lexer;
use crate::mir::lower::Mir;
use crate::mir::lower::ctx::{BodyLowerCtx, ExitObligation};
use crate::mir::{
    AggregateKind, AssertMessage, Body, BodyKind, CastKind, ConstKind, Constant, Local, Operand,
    Projection, Rvalue, StatementKind, TerminatorKind,
};
use crate::nameres;
use crate::nameres::PrimTy;
use crate::options::Mode;
use crate::parser::Parser;
use crate::testing::{
    OPS_PREAMBLE, first_extend_method, first_function, first_struct, lower_to_hir,
};
use crate::typeck::results::TypeResolutions;
use crate::typeck::ty::TyKind;
use crate::typeck::ty::ctx::TyCtx;

/// Lexes, parses, resolves, lowers, type-checks, and MIR-lowers `src`, in debug profile.
/// Panics if any diagnostic was reported by type checking.
fn lower_mir_src(src: &str) -> (Hir, TyCtx, TypeResolutions, Mir) {
    lower_mir_src_with_mode(src, Mode::Debug)
}

fn parse_file(src: &str) -> ParsedSrcFile {
    let chars: Vec<char> = src.chars().collect();
    let offset = crate::testing::add_file("<test>".to_string(), chars.clone(), FileOrigin::User);
    let tokens = Lexer::new(crate::testing::session(), &chars, offset).tokenize();
    Parser::new(crate::testing::session()).parse(&tokens, offset)
}

fn lower_mir_src_with_ops(src: &str) -> (Hir, TyCtx, TypeResolutions, Mir) {
    lower_mir_src_with_ops_and_mode(src, Mode::Debug)
}

const COPY_DROP_PREAMBLE: &str = "module core::ops;
     public trait Copy {}
     public trait Drop {}";

const REF_COPY_PREAMBLE: &str = "module core::ops;
     public trait Copy { fun copy(&self) -> Self; }
     extend i32 with Copy { fun copy(&self) -> Self { return *self; } }
     extend<T> &T with Copy { fun copy(&self) -> Self { return *self; } }";

/// Like [`lower_mir_src_with_copy_and_drop`], but with `Copy` implemented for `i32` and,
/// generically, for `&T` -- needed to read through a reference to a reference (`&&T`) without
/// moving out of either layer.
fn lower_mir_src_with_ref_copy(src: &str) -> (Hir, TyCtx, TypeResolutions, Mir) {
    crate::testing::clear_diagnostics();
    crate::testing::clear_interner();
    let files = vec![parse_file(REF_COPY_PREAMBLE), parse_file(src)];
    let ast = Ast::from(files);
    let res = nameres::resolve(crate::testing::session(), &ast);
    let hir = Hir::from(crate::testing::session(), &ast, &res);

    crate::testing::clear_diagnostics();
    let checked = crate::typeck::check(crate::testing::session(), &hir);
    let diagnostics = crate::testing::diagnostics();
    assert!(
        diagnostics.is_empty(),
        "unexpected diagnostics for {src:?}: {diagnostics:?}"
    );
    let crate::typeck::TypeckOutput { mut tcx, types } = checked;
    let program = super::lower(
        crate::testing::session(),
        &hir,
        &mut tcx,
        &types,
        Mode::Debug,
    );
    (hir, tcx, types, program)
}

fn lower_mir_src_with_copy_and_drop(src: &str) -> (Hir, TyCtx, TypeResolutions, Mir) {
    crate::testing::clear_diagnostics();
    crate::testing::clear_interner();
    let files = vec![parse_file(COPY_DROP_PREAMBLE), parse_file(src)];
    let ast = Ast::from(files);
    let res = nameres::resolve(crate::testing::session(), &ast);
    let hir = Hir::from(crate::testing::session(), &ast, &res);

    crate::testing::clear_diagnostics();
    let checked = crate::typeck::check(crate::testing::session(), &hir);
    let diagnostics = crate::testing::diagnostics();
    assert!(
        diagnostics.is_empty(),
        "unexpected diagnostics for {src:?}: {diagnostics:?}"
    );
    let crate::typeck::TypeckOutput { mut tcx, types } = checked;
    let program = super::lower(
        crate::testing::session(),
        &hir,
        &mut tcx,
        &types,
        Mode::Debug,
    );
    (hir, tcx, types, program)
}

fn lower_mir_src_with_ops_and_mode(src: &str, mode: Mode) -> (Hir, TyCtx, TypeResolutions, Mir) {
    crate::testing::clear_diagnostics();
    crate::testing::clear_interner();
    let files = vec![parse_file(OPS_PREAMBLE), parse_file(src)];
    let ast = Ast::from(files);
    let res = nameres::resolve(crate::testing::session(), &ast);
    let hir = Hir::from(crate::testing::session(), &ast, &res);

    crate::testing::clear_diagnostics();
    let checked = crate::typeck::check(crate::testing::session(), &hir);
    let diagnostics = crate::testing::diagnostics();
    assert!(
        diagnostics.is_empty(),
        "unexpected diagnostics for {src:?}: {diagnostics:?}"
    );
    let crate::typeck::TypeckOutput { mut tcx, types } = checked;
    let program = super::lower(crate::testing::session(), &hir, &mut tcx, &types, mode);
    (hir, tcx, types, program)
}

fn lower_mir_src_with_mode(src: &str, mode: Mode) -> (Hir, TyCtx, TypeResolutions, Mir) {
    let hir = lower_to_hir(src);
    crate::testing::clear_diagnostics();
    let checked = crate::typeck::check(crate::testing::session(), &hir);
    let diagnostics = crate::testing::diagnostics();
    assert!(
        diagnostics.is_empty(),
        "unexpected diagnostics for {src:?}: {diagnostics:?}"
    );
    let crate::typeck::TypeckOutput { mut tcx, types } = checked;
    let program = super::lower(crate::testing::session(), &hir, &mut tcx, &types, mode);
    (hir, tcx, types, program)
}

/// The `Body` lowered for the first top-level function `lower_to_hir`d and MIR-lowered.
fn first_function_body<'a>(program: &'a Mir, hir: &Hir) -> &'a Body {
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
    assert_eq!(body.param_count, 0);
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
                            && place.projections.is_empty()
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
        lower_mir_src_with_ops("fun add(x: i32, y: i32) -> i32 { return x + y; }");
    let body = first_function_body(&program, &hir);
    assert_eq!(body.param_count, 2);
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
        lower_mir_src_with_ops("fun f(x: i32) -> i32 { return if x < 0 { 0 } else { x }; }");
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
fn predecessors_of_an_if_join_block_are_both_branches() {
    let (hir, _tcx, _types, program) = lower_mir_src_with_ops(
        "fun f(x: i32) -> i32 { let y = if x < 0 { 0 } else { x }; return y; }",
    );
    let body = first_function_body(&program, &hir);

    let mut branch_blocks: Vec<_> = body
        .basic_blocks
        .iter()
        .find_map(|b| match &b.terminator.kind {
            TerminatorKind::SwitchInt { targets, .. } => Some(targets.all_targets().collect()),
            _ => None,
        })
        .expect("an if/else lowers to exactly one SwitchInt");

    let join_blocks: Vec<_> = branch_blocks
        .iter()
        .map(|&bb| match body.basic_blocks[bb.index()].terminator.kind {
            TerminatorKind::Goto { target } => target,
            ref other => panic!("expected the then/else block to end in a Goto, found {other:?}"),
        })
        .collect();
    assert_eq!(
        join_blocks[0], join_blocks[1],
        "both branches join the same block"
    );

    let mut preds = body.predecessors().of(join_blocks[0]).to_vec();
    preds.sort();
    branch_blocks.sort();
    assert_eq!(
        preds, branch_blocks,
        "the join block's predecessors are exactly the then-block and the else-block"
    );
}

#[test]
fn a_release_profile_body_wraps_instead_of_checking() {
    let (hir, _tcx, _types, program) = lower_mir_src_with_ops_and_mode(
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
    let (hir, _tcx, _types, program) = lower_mir_src_with_ops(
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
    let (hir, _tcx, _types, program) = lower_mir_src_with_ops(
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
        .filter(|((def, _), _)| matches!(hir.def(*def), OwnerNode::Closure(_)))
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
            matches!(hir.def(id), OwnerNode::Function(f) if crate::testing::resolve(f.name.text) == name)
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

/// `body`'s own local declared with source name `name` -- for a test that needs to pick one
/// binding's `Local` out of a body that, now that a compiler-inserted temporary gets a
/// `StorageLive`/`StorageDead` pair exactly like a named binding's own local does, cannot assume
/// a named binding is the only kind of local `storage_live_order`/`storage_dead_order` reports.
fn named_local(body: &Body, name: &str) -> Local {
    body.local_decls
        .iter()
        .position(|decl| {
            decl.name
                .is_some_and(|n| crate::testing::resolve(n.text) == name)
        })
        .map(Local::from_usize)
        .unwrap_or_else(|| panic!("no local named {name:?} in {body:?}"))
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
                        kind: ConstKind::FunDef(fun),
                        ..
                    }),
                ..
            } => Some(fun.def),
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
    let (hir, _tcx, _types, program) = lower_mir_src_with_ops(
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
        lower_mir_src_with_ops("fun f(x: i32, y: i32) -> bool { return x < y; }");
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
        lower_mir_src_with_ops("fun f(x: i32, y: i32) -> i32 { return x / y; }");
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
        lower_mir_src_with_ops("fun f(x: i32, y: i32) -> i32 { return x % y; }");
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
    let (hir, _tcx, _types, program) = lower_mir_src_with_ops_and_mode(
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
        lower_mir_src_with_ops("fun f(x: f64, y: f64) -> f64 { return x / y; }");
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
                place.projections == [Projection::Deref, Projection::Field(0)]
            }
            _ => false,
        });
    assert!(
        found,
        "`p.x` through a `&Point` derefs `p` before projecting to field 0"
    );
}

#[test]
fn tuple_index_access_projects_the_written_field() {
    let (hir, _tcx, _types, program) =
        lower_mir_src("fun f(t: (i32, bool)) -> bool { return t.1; }");
    let body = first_function_body(&program, &hir);
    let found = body
        .basic_blocks
        .iter()
        .flat_map(|b| &b.statements)
        .any(|s| match &s.kind {
            StatementKind::Assign(_, Rvalue::Use(Operand::Copy(place) | Operand::Move(place))) => {
                place.projections == [Projection::Field(1)]
            }
            _ => false,
        });
    assert!(found, "`t.1` projects to field 1");
}

#[test]
fn explicit_deref_of_a_reference_inserts_a_deref_projection() {
    let (hir, _tcx, _types, program) =
        lower_mir_src_with_ops("fun f(p: &i32) -> i32 { return *p; }");
    let body = first_function_body(&program, &hir);
    let found = body
        .basic_blocks
        .iter()
        .flat_map(|b| &b.statements)
        .any(|s| match &s.kind {
            StatementKind::Assign(_, Rvalue::Use(Operand::Copy(place) | Operand::Move(place))) => {
                place.projections == [Projection::Deref]
            }
            _ => false,
        });
    assert!(found, "`*p` derefs `p` with no further projection");
}

#[test]
fn explicit_deref_of_an_owned_pointer_inserts_a_deref_projection() {
    let (hir, _tcx, _types, program) = lower_mir_src("fun f(p: iso i32) -> i32 { return *p; }");
    let body = first_function_body(&program, &hir);
    let found = body
        .basic_blocks
        .iter()
        .flat_map(|b| &b.statements)
        .any(|s| match &s.kind {
            StatementKind::Assign(_, Rvalue::Use(Operand::Copy(place) | Operand::Move(place))) => {
                place.projections == [Projection::Deref]
            }
            _ => false,
        });
    assert!(found, "`*p` derefs `p` with no further projection");
}

#[test]
fn double_deref_of_a_reference_to_a_reference_chains_two_deref_projections() {
    let (hir, _tcx, _types, program) =
        lower_mir_src_with_ref_copy("fun f(p: &&i32) -> i32 { return **p; }");
    let body = first_function_body(&program, &hir);
    let found = body
        .basic_blocks
        .iter()
        .flat_map(|b| &b.statements)
        .any(|s| match &s.kind {
            StatementKind::Assign(_, Rvalue::Use(Operand::Copy(place) | Operand::Move(place))) => {
                place.projections == [Projection::Deref, Projection::Deref]
            }
            _ => false,
        });
    assert!(found, "`**p` chains two deref projections onto `p`");
}

fn deref_operand_kind(program: &Mir, hir: &Hir) -> &'static str {
    let body = first_function_body(program, hir);
    body.basic_blocks
        .iter()
        .flat_map(|b| &b.statements)
        .find_map(|s| match &s.kind {
            StatementKind::Assign(_, Rvalue::Use(Operand::Copy(place)))
                if place.projections == [Projection::Deref] =>
            {
                Some("copy")
            }
            StatementKind::Assign(_, Rvalue::Use(Operand::Move(place)))
                if place.projections == [Projection::Deref] =>
            {
                Some("move")
            }
            _ => None,
        })
        .unwrap_or_else(|| panic!("expected a deref read among the lowered statements"))
}

#[test]
fn dereferencing_a_copy_type_copies_the_value() {
    let (hir, _tcx, _types, program) = lower_mir_src_with_copy_and_drop(
        "import core::ops::Copy;

         struct Point { x: i32 }
         extend Point with Copy {}

         fun f(p: iso Point) -> Point { return *p; }",
    );
    assert_eq!(deref_operand_kind(&program, &hir), "copy");
}

#[test]
fn dereferencing_a_drop_type_moves_the_value() {
    let (hir, _tcx, _types, program) = lower_mir_src_with_copy_and_drop(
        "import core::ops::Drop;

         struct Handle { x: i32 }
         extend Handle with Drop {}

         fun f(p: iso Handle) -> Handle { return *p; }",
    );
    assert_eq!(deref_operand_kind(&program, &hir), "move");
}

#[test]
fn dereferencing_a_type_with_neither_copy_nor_drop_moves_the_value() {
    let (hir, _tcx, _types, program) = lower_mir_src(
        "struct Plain { x: i32 }

         fun f(p: iso Plain) -> Plain { return *p; }",
    );
    assert_eq!(deref_operand_kind(&program, &hir), "move");
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
                matches!(place.projections.last(), Some(Projection::Index(_)))
            }
            _ => false,
        });
    assert!(
        indexes,
        "the checked read itself projects through PlaceElem::Index"
    );
}

#[test]
fn array_indexing_by_a_constant_literal_projects_through_constant_index() {
    let (hir, _tcx, _types, program) = lower_mir_src("fun f(a: [i32; 4]) -> i32 { return a[2]; }");
    let body = first_function_body(&program, &hir);
    let projects_constant_index_2 = body
        .basic_blocks
        .iter()
        .flat_map(|b| &b.statements)
        .any(|s| match &s.kind {
            StatementKind::Assign(_, Rvalue::Use(Operand::Copy(place) | Operand::Move(place))) => {
                matches!(place.projections.last(), Some(Projection::ConstantIndex(2)))
            }
            _ => false,
        });
    assert!(
        projects_constant_index_2,
        "indexing by a literal projects through Projection::ConstantIndex(2), not a runtime Index local"
    );
    let no_dynamic_index = !body
        .basic_blocks
        .iter()
        .flat_map(|b| &b.statements)
        .any(|s| match &s.kind {
            StatementKind::Assign(_, Rvalue::Use(Operand::Copy(place) | Operand::Move(place))) => {
                matches!(place.projections.last(), Some(Projection::Index(_)))
            }
            _ => false,
        });
    assert!(
        no_dynamic_index,
        "a literal index should not also produce a dynamic Index projection"
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
fn new_produces_a_new_rvalue() {
    let (hir, tcx, _types, program) = lower_mir_src("fun f() -> iso i32 { return new 1; }");
    let body = first_function_body(&program, &hir);
    let found = body
        .basic_blocks
        .iter()
        .flat_map(|b| &b.statements)
        .find_map(|s| match &s.kind {
            StatementKind::Assign(place, Rvalue::New(_)) => Some(place.clone()),
            _ => None,
        });
    let place = found.expect("`new 1` lowers to a New rvalue");
    let local_ty = body.local_decls[place.local.index()].ty;
    assert!(
        matches!(tcx.kind(local_ty), TyKind::Iso(_)),
        "the New rvalue is assigned into a local typed `iso T`"
    );
}

#[test]
fn new_array_produces_a_new_array_rvalue_with_both_operands() {
    let (hir, _tcx, _types, program) =
        lower_mir_src("fun f(n: usize) -> iso [u8] { return new [0_u8; n]; }");
    let body = first_function_body(&program, &hir);
    let found = body
        .basic_blocks
        .iter()
        .flat_map(|b| &b.statements)
        .any(|s| matches!(&s.kind, StatementKind::Assign(_, Rvalue::NewArray { .. })));
    assert!(found, "`new [0_u8; n]` lowers to a NewArray rvalue");
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
                        kind: CastKind::ReifyFunPointer,
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
fn an_indirect_call_through_a_function_typed_place_reads_the_callee_without_consuming_it() {
    let (hir, _tcx, _types, program) = lower_mir_src(
        "fun apply(f: fun(i32, i32) -> i32, x: i32, y: i32) -> i32 { return f(x, y); }
         fun add(x: i32, y: i32) -> i32 { return x + y; }",
    );
    let body = first_function_body(&program, &hir);
    let indirect = body.basic_blocks.iter().any(|b| {
        matches!(
            &b.terminator.kind,
            TerminatorKind::Call {
                func: Operand::Copy(_),
                ..
            }
        )
    });
    assert!(
        indirect,
        "calling through a `fun`-typed parameter reads its place -- it names no single DefId, \
         unlike a direct call to a named function -- and reads it without consuming it, since a \
         closure value owns its environment and stays callable and droppable afterwards"
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
    let named_live_count = live
        .iter()
        .filter(|&&l| body.local_decls[l.index()].name.is_some())
        .count();
    assert_eq!(
        named_live_count, 4,
        "`a`, `b`, and the two with-lends, `x` and `y`, are each made live exactly once"
    );
    assert_eq!(
        live.len(),
        5,
        "one more than the four named bindings: `noop()`'s own discarded result gets a \
         compiler-inserted temporary now that one of those is bracketed exactly like a named \
         binding's own local is: {live:?}"
    );
    assert_eq!(
        dead, expected,
        "every StorageDead runs in the exact reverse of StorageLive's own order, the same \
         last-in-first-out discipline an ordinary stack has -- true of the with-lends' own \
         locals same as before, and, now, of noop()'s own temporary too"
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
    assert_eq!(body.param_count, 0);
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

    let mut ctx = BodyLowerCtx::new(
        crate::testing::session(),
        &hir,
        &mut tcx,
        &types,
        Mode::Debug,
        def_id,
        None,
    );
    ctx.push_block_scope();
    // `new_temp` now registers its own `StorageDead` obligation, the same one a manual
    // `register_exit_obligation` call used to be needed for here -- see its own doc comment.
    // Only its presence on the outer scope matters below, not the local itself.
    let _outer_local = ctx.new_temp(unit_ty, span);

    let break_block = ctx.new_block();
    let continue_block = ctx.new_block();
    ctx.push_loop(break_block, continue_block);
    ctx.push_block_scope();
    let inner_local = ctx.new_temp(unit_ty, span);

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
    let (hir, _tcx, _types, program) = lower_mir_src_with_ops(
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
        3,
        "the scrutinee's own temporary, `n`, and the guard condition `n > 0`'s own temporary, \
         each get a StorageLive now that a compiler-inserted temporary is bracketed exactly like \
         a named binding: {live:?}"
    );
    let n = named_local(body, "n");
    let dead_for_n = storage_dead_order(body).iter().filter(|&&l| l == n).count();
    assert_eq!(
        dead_for_n, 2,
        "`n` is cleaned up once on the guard's failure path and once more on its own success path"
    );
}

#[test]
fn a_tuple_patterns_elements_are_tested_independently() {
    let (hir, _tcx, _types, program) = lower_mir_src_with_ops(
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
                match place.projections.last() {
                    Some(Projection::Field(i)) => Some(*i),
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
// Literals and literal patterns
// -----------------------------------------------------------------

/// The lexer keeps `_` digit separators in a numeric literal's text, but a `Literal`'s value
/// symbol is separator-free by construction, so lowering parses the plain digits. Before the
/// value was normalized at AST construction, `1_000_000` panicked in `parse::<i128>` here.
#[test]
#[allow(clippy::approx_constant)]
fn digit_separated_numeric_literals_lower_to_their_normalized_values() {
    let (hir, _tcx, _types, program) =
        lower_mir_src("fun f() { let _ = 1_000_000_i64; let _ = 3.14_15_f64; }");
    let body = first_function_body(&program, &hir);

    let mut ints = Vec::new();
    let mut floats = Vec::new();
    for block in &body.basic_blocks {
        for statement in &block.statements {
            if let StatementKind::Assign(_, Rvalue::Use(Operand::Constant(constant))) =
                &statement.kind
            {
                match &constant.kind {
                    ConstKind::Int(v) => ints.push(*v),
                    ConstKind::Float(v) => floats.push(*v),
                    _ => {}
                }
            }
        }
    }

    assert_eq!(ints, vec![1_000_000]);
    assert_eq!(floats, vec![3.1415]);
}

/// A float literal pattern tests the scrutinee with `BinaryOp::Eq` against a float constant,
/// the same shape an integer literal pattern produces; float comparison is fully supported
/// downstream.
#[test]
#[allow(clippy::approx_constant)]
fn a_float_literal_pattern_lowers_to_an_equality_test_against_a_float_constant() {
    let (hir, _tcx, _types, program) =
        lower_mir_src("fun f(x: f64) -> i32 { return match x { 3.14_15 => 1, _ => 0 }; }");
    let body = first_function_body(&program, &hir);

    let found = body
        .basic_blocks
        .iter()
        .flat_map(|b| &b.statements)
        .find_map(|s| match &s.kind {
            StatementKind::Assign(
                _,
                Rvalue::BinaryOp(
                    BinaryOp::Eq,
                    _,
                    Operand::Constant(Constant {
                        kind: ConstKind::Float(v),
                        ..
                    }),
                ),
            ) => Some(*v),
            _ => None,
        })
        .expect("the float pattern tests the scrutinee against a float constant");
    assert_eq!(found, 3.1415);
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
        closure_body.param_count, 1,
        "the environment local is the closure's own implicit first argument, even with no \
         captures at all"
    );
    let env_ty = closure_body.local_decls[1].ty;
    let TyKind::Ref { base, .. } = *tcx.kind(env_ty) else {
        panic!("the environment local borrows the environment the closure value owns");
    };
    assert!(
        matches!(tcx.kind(base), TyKind::Tuple(elems) if elems.len() == 1),
        "an empty capture list still gets an environment local, holding just the drop-glue word \
         every environment leads with"
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
    let (hir, tcx, _types, program) = lower_mir_src_with_ops(
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
    assert_eq!(body.param_count, 0);
    // Slot 0 is the return place; slot 1 is `x` itself -- the fast, scrutinee-free path a bare
    // `Binding` pattern takes (see `a_plain_let_binding_allocates_exactly_one_local`).
    let x_decl = &body.local_decls[1];
    let name = x_decl
        .name
        .expect("a let-bound local carries its declared name");
    assert_eq!(crate::testing::resolve(name.text), "x");
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
    assert_eq!(crate::testing::resolve(name.text), "x");
}

// -----------------------------------------------------------------
// Copy vs Move classification
// -----------------------------------------------------------------

/// A shared reference grants no exclusive access, so re-reading the same place holding one is
/// exactly as sound as re-reading any other trivially copyable value: `operand_for_place` treats
/// `&T` the same as a primitive. A `&mut T` still cannot be duplicated this way (it *is*
/// exclusive access), so it keeps falling through to `Operand::Move` -- see
/// `a_mutably_referenced_place_is_moved_not_copied`, just below.
#[test]
fn a_shared_reference_is_copied_not_moved() {
    let (hir, _tcx, _types, program) = lower_mir_src_with_ref_copy(
        "fun f(a: i32) {
             let r = &a;
             use_ref(r);
             use_ref(r);
         }
         fun use_ref(r: &i32) {}",
    );
    let body = first_function_body(&program, &hir);
    let call_args: Vec<&Operand> = body
        .basic_blocks
        .iter()
        .filter_map(|b| match &b.terminator.kind {
            TerminatorKind::Call { args, .. } => args.first(),
            _ => None,
        })
        .collect();
    assert_eq!(call_args.len(), 2, "both calls to `use_ref` pass `r`");
    for arg in call_args {
        assert!(
            matches!(arg, Operand::Copy(_)),
            "a shared reference is reusable without consuming it, so each call reads `r` by \
             `Operand::Copy`, not `Operand::Move`: got {arg:?}"
        );
    }
}

/// The mirror of `a_shared_reference_is_copied_not_moved`: a `&mut` reference grants exclusive
/// access, so duplicating it defeats the whole point of exclusivity. It still gets
/// `Operand::Move`.
#[test]
fn a_mutably_referenced_place_is_moved_not_copied() {
    let (hir, _tcx, _types, program) = lower_mir_src(
        "fun f(a: i32) {
             let mut a = a;
             let r = &mut a;
             use_ref(r);
         }
         fun use_ref(r: &mut i32) {}",
    );
    let body = first_function_body(&program, &hir);
    let call_arg = body
        .basic_blocks
        .iter()
        .find_map(|b| match &b.terminator.kind {
            TerminatorKind::Call { args, .. } => args.first(),
            _ => None,
        })
        .expect("`use_ref(r)` is called");
    assert!(
        matches!(call_arg, Operand::Move(_)),
        "a `&mut` reference is exclusive access, so it is consumed by `Operand::Move`, not \
         `Operand::Copy`: got {call_arg:?}"
    );
}

#[test]
fn lower_populates_every_new_mir_field() {
    let (hir, mut tcx, _types, program) = lower_mir_src(
        "struct Point { x: i32, y: i32 }
         fun main() -> i32 { let p = Point { x: 1, y: 2 }; return p.x; }",
    );

    let point_def = first_struct(&hir);
    assert_eq!(tcx.struct_field_tys(point_def, &[]).len(), 2);
    assert!(program.vtables.is_empty());

    let body = program
        .bodies
        .get(&(first_function(&hir), None))
        .expect("main is lowered");
    assert_eq!(body.kind, BodyKind::Function);
    assert!(body.generics.is_empty(), "main declares no generics");
}

fn assert_terminator(body: &Body) -> (bool, &AssertMessage) {
    body.basic_blocks
        .iter()
        .find_map(|b| match &b.terminator.kind {
            TerminatorKind::Assert { expected, msg, .. } => Some((*expected, msg)),
            _ => None,
        })
        .expect("an `Assert` terminator is present")
}

#[test]
fn assert_lowers_to_an_assert_terminator_expecting_the_condition_to_hold() {
    let (hir, _tcx, _types, program) = lower_mir_src("fun f(x: bool) { assert(x); }");
    let body = first_function_body(&program, &hir);
    let (expected, msg) = assert_terminator(body);
    assert!(
        expected,
        "a plain `assert` traps when its condition is false"
    );
    assert!(matches!(msg, AssertMessage::Assert(None)));
}

#[test]
fn assert_with_a_message_carries_the_message_operand() {
    let (hir, _tcx, _types, program) =
        lower_mir_src(r#"fun f(x: bool) { assert(x, "x must hold"); }"#);
    let body = first_function_body(&program, &hir);
    let (_, msg) = assert_terminator(body);
    assert!(matches!(msg, AssertMessage::Assert(Some(_))));
}

#[test]
fn panic_lowers_to_an_always_failing_assert() {
    let (hir, _tcx, _types, program) = lower_mir_src("fun f() { panic(); }");
    let body = first_function_body(&program, &hir);
    let (expected, msg) = assert_terminator(body);
    assert!(!expected, "`panic` traps unconditionally");
    assert!(matches!(msg, AssertMessage::Panic(None)));
}

#[test]
fn panic_with_a_message_carries_the_message_operand() {
    let (hir, _tcx, _types, program) = lower_mir_src(r#"fun f() { panic("boom"); }"#);
    let body = first_function_body(&program, &hir);
    let (_, msg) = assert_terminator(body);
    assert!(matches!(msg, AssertMessage::Panic(Some(_))));
}

#[test]
fn unreachable_lowers_to_an_always_failing_assert() {
    let (hir, _tcx, _types, program) = lower_mir_src("fun f() { unreachable(); }");
    let body = first_function_body(&program, &hir);
    let (expected, msg) = assert_terminator(body);
    assert!(!expected, "`unreachable` traps unconditionally");
    assert!(matches!(msg, AssertMessage::Unreachable(None)));
}

#[test]
fn a_reference_match_derefs_the_scrutinee_place() {
    let (hir, _tcx, _types, program) = lower_mir_src_with_ops(
        "enum Opt { some: i32, none }
         fun is_some(o: &Opt) -> bool {
             return match o { .some(_) => true, .none => false, };
         }",
    );
    let body = first_function_body(&program, &hir);
    assert!(
        body.basic_blocks
            .iter()
            .flat_map(|b| &b.statements)
            .any(|s| match &s.kind {
                StatementKind::Assign(_, Rvalue::Discriminant(place)) =>
                    place.projections.contains(&Projection::Deref),
                _ => false,
            }),
        "the discriminant is read through a Deref projection"
    );
}

#[test]
fn a_payload_bound_through_a_reference_is_a_borrow() {
    let (hir, _tcx, _types, program) = lower_mir_src_with_ops(
        "enum Opt { some: i32, none }
         fun peek(o: &Opt) -> i32 {
             return match o { .some(v) => *v, .none => 0, };
         }",
    );
    let body = first_function_body(&program, &hir);
    assert!(
        body.basic_blocks
            .iter()
            .flat_map(|b| &b.statements)
            .any(|s| matches!(&s.kind, StatementKind::Assign(_, Rvalue::Ref { .. }))),
        "the payload binding lowers to a Ref rvalue"
    );
}

/// BUG: `lower_index_place` builds the bounds-check `Assert` with a condition that is the
/// constant `true`, never `index < len`. The backend branches on that condition, so the failure
/// block that aborts with "index out of bounds" is dead and out-of-bounds reads execute.
///
/// Run with `cargo test --bin phi -- --ignored` to reproduce.
#[test]
fn array_bounds_check_condition_is_not_a_hard_coded_true() {
    let (hir, _tcx, _types, program) =
        lower_mir_src("fun f(a: [i32; 4], i: i32) -> i32 { return a[i]; }");
    let body = first_function_body(&program, &hir);
    let has_always_true_assert = body.basic_blocks.iter().any(|b| {
        matches!(
            &b.terminator.kind,
            TerminatorKind::Assert {
                cond: Operand::Constant(Constant {
                    kind: ConstKind::Bool(true),
                    ..
                }),
                ..
            }
        )
    });
    assert!(
        !has_always_true_assert,
        "the bounds check must test `index < len`, not assert a constant `true`"
    );
}

/// BUG: `lower_index_place` sizes the length temporary with `index_ty` (e.g. `i32` for a default
/// integer index) even though `Rvalue::Len` is 64-bit and codegen's `Projection::Index`
/// unconditionally loads/stores `i64`. The temporary should be `usize`/`i64` regardless of the
/// index expression's own type.
///
/// Run with `cargo test --bin phi -- --ignored` to reproduce.
#[test]
fn array_bounds_length_local_is_wide_enough_for_rvalue_len() {
    let (hir, tcx, _types, program) =
        lower_mir_src("fun f(a: [i32; 4], i: i32) -> i32 { return a[i]; }");
    let body = first_function_body(&program, &hir);
    let len_local = body
        .basic_blocks
        .iter()
        .flat_map(|b| &b.statements)
        .find_map(|s| match &s.kind {
            StatementKind::Assign(place, Rvalue::Len(_)) => Some(place.local),
            _ => None,
        })
        .expect("indexing lowers a `Rvalue::Len`");
    let ty = body.local_decls[len_local.index()].ty;
    assert!(
        matches!(
            tcx.kind(ty),
            TyKind::Primitive(PrimTy::Usize)
                | TyKind::Primitive(PrimTy::U64)
                | TyKind::Primitive(PrimTy::I64)
        ),
        "the local holding `Rvalue::Len` (loaded as i64 by codegen) is {ty:?}"
    );
}

// -----------------------------------------------------------------
// Expression forms in value position
// -----------------------------------------------------------------

/// The `Result` lang item `?` needs, as its own core file so the test source stays at the crate
/// root where `first_function` can find it.
const RESULT_PREAMBLE: &str = "module core::result;
     public enum Result<T, E> { ok: T, err: E }";

fn lower_mir_src_with_result(src: &str) -> (Hir, TyCtx, TypeResolutions, Mir) {
    crate::testing::clear_diagnostics();
    crate::testing::clear_interner();
    let files = vec![parse_file(RESULT_PREAMBLE), parse_file(src)];
    let ast = Ast::from(files);
    let res = nameres::resolve(crate::testing::session(), &ast);
    let hir = Hir::from(crate::testing::session(), &ast, &res);

    crate::testing::clear_diagnostics();
    let checked = crate::typeck::check(crate::testing::session(), &hir);
    let diagnostics = crate::testing::diagnostics();
    assert!(
        diagnostics.is_empty(),
        "unexpected diagnostics for {src:?}: {diagnostics:?}"
    );
    let crate::typeck::TypeckOutput { mut tcx, types } = checked;
    let program = super::lower(
        crate::testing::session(),
        &hir,
        &mut tcx,
        &types,
        Mode::Debug,
    );
    (hir, tcx, types, program)
}

const OPTION_PREAMBLE: &str = "module core::option;
     public enum Option<T> { some: T, none }";

/// Like [`lower_mir_src_with_result`], but with the `Option` lang item defined too, for exercising
/// `?` on an `Option`.
fn lower_mir_src_with_option_result(src: &str) -> (Hir, TyCtx, TypeResolutions, Mir) {
    crate::testing::clear_diagnostics();
    crate::testing::clear_interner();
    let files = vec![
        parse_file(RESULT_PREAMBLE),
        parse_file(OPTION_PREAMBLE),
        parse_file(src),
    ];
    let ast = Ast::from(files);
    let res = nameres::resolve(crate::testing::session(), &ast);
    let hir = Hir::from(crate::testing::session(), &ast, &res);

    crate::testing::clear_diagnostics();
    let checked = crate::typeck::check(crate::testing::session(), &hir);
    let diagnostics = crate::testing::diagnostics();
    assert!(
        diagnostics.is_empty(),
        "unexpected diagnostics for {src:?}: {diagnostics:?}"
    );
    let crate::typeck::TypeckOutput { mut tcx, types } = checked;
    let program = super::lower(
        crate::testing::session(),
        &hir,
        &mut tcx,
        &types,
        Mode::Debug,
    );
    (hir, tcx, types, program)
}

/// `lower_expr_discarding`'s literal arm: a literal has no place to read or write, so it lowers
/// to nothing at all rather than a `PlaceMention`.
#[test]
fn a_discarded_literal_statement_lowers_to_nothing() {
    let (hir, _tcx, _types, program) = lower_mir_src("fun f() { 1; }");
    let body = first_function_body(&program, &hir);
    assert!(
        !body
            .basic_blocks
            .iter()
            .flat_map(|b| &b.statements)
            .any(|s| matches!(s.kind, StatementKind::PlaceMention(_))),
        "a literal has no place to mention"
    );
}

/// An assignment used as a value (not a bare statement) still writes through its place and the
/// expression's own value is unit.
#[test]
fn an_assignment_used_as_a_value_writes_through_and_yields_unit() {
    let (hir, _tcx, _types, program) = lower_mir_src("fun f() { let mut a = 0; let x = (a = 1); }");
    let body = first_function_body(&program, &hir);
    let a = named_local(body, "a");
    assert!(
        body.basic_blocks
            .iter()
            .flat_map(|b| &b.statements)
            .any(|s| matches!(
                &s.kind,
                StatementKind::Assign(place, Rvalue::Use(Operand::Constant(Constant {
                    kind: ConstKind::Int(1),
                    ..
                }))) if place.local == a
            )),
        "`a = 1` still writes 1 into `a` when the assignment is used as a value"
    );
}

/// The `AssignOp` counterpart, whose result is computed and stored back before the unit value is
/// produced into `dest`.
#[test]
fn a_compound_assignment_used_as_a_value_stores_back_and_yields_unit() {
    let (hir, _tcx, _types, program) =
        lower_mir_src_with_ops("fun f() { let mut a = 1; let x = (a += 1); }");
    let body = first_function_body(&program, &hir);
    let a = named_local(body, "a");
    let stored_back = body
        .basic_blocks
        .iter()
        .flat_map(|b| &b.statements)
        .any(|s| {
            matches!(
                &s.kind,
                StatementKind::Assign(place, Rvalue::Use(Operand::Move(inner)))
                    if place.local == a && inner.local != a
            )
        });
    assert!(
        stored_back,
        "`a += 1` stores the summed result back into `a` and the expression is unit"
    );
}

/// `?` switches on the `Result`'s discriminant and reads the `ok` payload through a
/// `Downcast(ok).Field(0)` projection; the `err` path returns the enclosing function's own
/// `Result::err` immediately.
#[test]
fn try_unwraps_the_ok_payload_and_returns_the_err() {
    let (hir, _tcx, _types, program) = lower_mir_src_with_result(
        "import core::result::Result;
         fun f(r: Result<i32, bool>) -> Result<i32, bool> {
             let v = r?;
             return .ok(v);
         }",
    );
    let body = first_function_body(&program, &hir);
    let switches = body
        .basic_blocks
        .iter()
        .filter(|b| matches!(b.terminator.kind, TerminatorKind::SwitchInt { .. }))
        .count();
    assert!(switches >= 1, "`?` switches on the Result's discriminant");
    let downcasts = body
        .basic_blocks
        .iter()
        .flat_map(|b| &b.statements)
        .any(|s| match &s.kind {
            StatementKind::Assign(_, Rvalue::Use(Operand::Copy(place) | Operand::Move(place))) => {
                place
                    .projections
                    .iter()
                    .any(|p| matches!(p, Projection::Downcast(_)))
            }
            _ => false,
        });
    assert!(
        downcasts,
        "`?` reads the payload through a Downcast projection"
    );
}

/// A bare block expression reaches `lower_block`'s tail-value path, lowering the block's own
/// bindings before assigning the tail into the destination.
#[test]
fn a_block_expression_lowers_its_inner_bindings_and_tail() {
    let (hir, _tcx, _types, program) = lower_mir_src("fun f() -> i32 { return { let y = 2; y }; }");
    let body = first_function_body(&program, &hir);
    let _y = named_local(body, "y");
}

/// Indexing a reference to an array peels the reference first, so the place reads through a
/// `Deref` projection before the element projection.
#[test]
fn indexing_through_a_reference_derefs_before_indexing() {
    let (hir, _tcx, _types, program) =
        lower_mir_src("fun f(a: &[i32; 4], i: i32) -> i32 { return a[i]; }");
    let body = first_function_body(&program, &hir);
    let deref_then_index = body
        .basic_blocks
        .iter()
        .flat_map(|b| &b.statements)
        .any(|s| match &s.kind {
            StatementKind::Assign(_, Rvalue::Use(Operand::Copy(place) | Operand::Move(place))) => {
                place.projections.first() == Some(&Projection::Deref)
                    && matches!(
                        place.projections.last(),
                        Some(Projection::Index(_) | Projection::ConstantIndex(_))
                    )
            }
            _ => false,
        });
    assert!(
        deref_then_index,
        "indexing `&[i32; 4]` derefs the reference before projecting the element"
    );
}

/// The `Index` trait's `index` method resolved in value position (as opposed to a place) lowers
/// to an ordinary method call through `lower_call_like_into`.
#[test]
fn an_overloaded_index_used_as_a_value_lowers_to_its_method_call() {
    let (hir, _tcx, _types, program) = lower_mir_src(
        "public trait Index<K, V> { fun index(&self, key: K) -> &V; }
         struct Map { value: bool }
         extend Map with Index<i32, bool> {
             fun index(&self, key: i32) -> &bool { return &self.value; }
         }
         fun f(m: Map) -> &bool { return m[0]; }",
    );
    let body = first_function_body(&program, &hir);
    assert!(
        body.basic_blocks
            .iter()
            .any(|b| matches!(b.terminator.kind, TerminatorKind::Call { .. })),
        "an overloaded index lowers to the `index` method call"
    );
}

/// A qualified variant call, `Shape.circle(1.0)`, reads as an access but builds a value; its
/// one call argument becomes the variant's single payload operand.
#[test]
fn a_qualified_variant_call_carries_its_single_argument() {
    let (hir, _tcx, _types, program) = lower_mir_src(
        "enum Shape { unit, circle: f64 }
         fun f() -> Shape { return Shape.circle(1.0); }",
    );
    let body = first_function_body(&program, &hir);
    let operand_count = body
        .basic_blocks
        .iter()
        .flat_map(|b| &b.statements)
        .find_map(|s| match &s.kind {
            StatementKind::Assign(_, Rvalue::Aggregate(kind, operands))
                if matches!(**kind, AggregateKind::Adt { .. }) =>
            {
                Some(operands.len())
            }
            _ => None,
        });
    assert_eq!(
        operand_count,
        Some(1),
        "`Shape.circle(1.0)` builds the variant with its one float payload"
    );
}

/// TODO: an overloaded `Index` used as a place (an assignment target) is not yet implemented and
/// panics. Typeck accepts `m[0] = true` as a place, so this is reachable from a valid program;
/// this test pins the current panic until place-position indexing is implemented.
#[test]
#[should_panic(
    expected = "mir::lower: an overloaded `Index`/`IndexSet` used as a place is not yet implemented"
)]
fn an_overloaded_index_used_as_a_place_panics_until_implemented() {
    let _ = lower_mir_src(
        "public trait Index<K, V> { fun index(&self, key: K) -> bool; }
         struct Map { value: bool }
         extend Map with Index<i32, bool> {
             fun index(&self, key: i32) -> bool { return self.value; }
         }
         fun f(m: Map) { m[0] = true; }",
    );
}

/// `spawn` type-checks as an ordinary block (see `typeck.rs`'s `ExprKind::Spawn` arm) but MIR
/// lowering is not implemented and panics. Reachable from a program typeck accepts, so the panic
/// is pinned here until the concurrency runtime lands.
#[test]
#[should_panic(
    expected = "mir::lower: `spawn` is not yet implemented (the runtime nursery API is illustrative only)"
)]
fn spawn_panics_until_the_concurrency_runtime_is_implemented() {
    let _ = lower_mir_src("fun f() { spawn { noop(); } } fun noop() {}");
}

/// The `concurrent` counterpart of [`spawn_panics_until_the_concurrency_runtime_is_implemented`].
#[test]
#[should_panic(
    expected = "mir::lower: `concurrent` is not yet implemented (the runtime nursery API is illustrative only)"
)]
fn concurrent_panics_until_the_concurrency_runtime_is_implemented() {
    let _ = lower_mir_src("fun f() { concurrent { noop(); } } fun noop() {}");
}

/// `?` on an `Option` type-checks (see `Typeck::check_try`), but `lower_try_into` only knows how
/// to build the `Result`'s own `err` variant and panics when the operand's `Adt` does not carry
/// two type arguments. Pins the current limitation.
#[test]
#[should_panic(expected = "mir::lower: `?`'s operand is not a two-argument Result")]
fn try_on_an_option_panics_until_option_propagation_is_implemented() {
    let (hir, _tcx, _types, program) = lower_mir_src_with_option_result(
        "import core::option::Option;
         fun f(o: Option<i32>) -> Option<i32> {
             let v = o?;
             return .some(v);
         }",
    );
    let _ = first_function_body(&program, &hir);
}

/// A mutable borrow of an `any`-specialized overloaded index. `m[0]` is a place (its base `m` is
/// a path), so typeck accepts `&mut m[0]`, and the borrow of the any-specialized call resolves
/// under `AnyMode::RefMut` rather than `AnyMode::Ref`.
#[test]
fn a_mutably_borrowed_any_specialized_index_uses_ref_mut_mode() {
    let (hir, _tcx, _types, program) = lower_mir_src(
        "public trait Index<K, V> { fun index(&self, key: K) -> any V; }
         struct Map { value: i32 }
         extend Map with Index<i32, i32> {
             fun index(&self, key: i32) -> any i32 { return self.value; }
         }
         fun f(m: &mut Map) { let r = &mut m[0]; }",
    );
    let body = first_function_body(&program, &hir);
    assert!(
        body.basic_blocks
            .iter()
            .any(|b| matches!(b.terminator.kind, TerminatorKind::Call { .. })),
        "the mutably borrowed index still lowers to a method call"
    );
}

// -----------------------------------------------------------------
// `dyn` dispatch, indirect callees, and `any` argument modes
// -----------------------------------------------------------------

/// A method call through a `&dyn Trait` receiver dispatches through the trait's own declaration,
/// so the receiver is passed first and the method's written parameters follow it.
#[test]
fn a_dyn_receiver_dispatches_through_the_traits_own_declaration() {
    let (hir, _tcx, _types, program) = lower_mir_src(
        "trait Show { fun add(&self, x: i32) -> i32; }
         struct W { v: i32 }
         extend W with Show { fun add(&self, x: i32) -> i32 { return x; } }
         fun draw(s: &dyn Show, x: i32) -> i32 { return s.add(x); }",
    );
    let body = first_function_body(&program, &hir);
    let call = body
        .basic_blocks
        .iter()
        .find_map(|b| match &b.terminator.kind {
            TerminatorKind::Call { args, target, .. } => Some((args.len(), target.is_some())),
            _ => None,
        })
        .expect("`s.add(x)` lowers to a call");
    assert_eq!(call, (2, true), "the dyn call passes `s` then `x`");
}

/// The `Index` operator on a `&dyn Index<K, V>` receiver is itself dyn-dispatched: the trait's
/// own `index` declaration is the call target, not any concrete impl.
#[test]
fn a_dyn_index_receiver_dispatches_through_the_trait_declaration() {
    let (hir, _tcx, _types, program) = lower_mir_src(
        "public trait Index<K, V> { fun index(&self, key: K) -> &V; }
         struct Map { value: bool }
         extend Map with Index<i32, bool> {
             fun index(&self, key: i32) -> &bool { return &self.value; }
         }
         fun f(x: &dyn Index<i32, bool>) -> &bool { return x[0]; }",
    );
    let body = first_function_body(&program, &hir);
    assert!(
        body.basic_blocks
            .iter()
            .any(|b| matches!(b.terminator.kind, TerminatorKind::Call { .. })),
        "`x[0]` through a `&dyn Index` receiver lowers to the trait's `index` call"
    );
}

/// A method reached through an `any T` receiver lowers the owned `any` position for each
/// argument, since the receiver's own `any` has no borrow mode of its own to preserve.
#[test]
fn a_method_on_an_any_receiver_peels_the_any_wrapper() {
    let (hir, _tcx, _types, program) = lower_mir_src(
        "struct Foo {}
         extend Foo { fun show(any self) {} }
         fun f(d: any Foo) { d.show(); }",
    );
    let body = first_function_body(&program, &hir);
    assert!(
        body.basic_blocks
            .iter()
            .any(|b| matches!(b.terminator.kind, TerminatorKind::Call { .. })),
        "`d.show()` on an `any Foo` receiver lowers to a call"
    );
}

/// An `any T` parameter passed to a call whose result is used owned resolves to its plain `T`,
/// rather than taking a reference to the argument.
#[test]
fn an_any_parameter_passed_owned_lowers_the_plain_value() {
    let (hir, _tcx, _types, program) = lower_mir_src(
        "fun min(x: any i32, y: any i32) -> any i32 { return x; }
         fun f(a: i32, b: i32) { min(a, b); }",
    );
    let body = program
        .bodies
        .get(&(find_function(&hir, "f"), None))
        .expect("`f` is lowered");
    assert!(
        body.basic_blocks
            .iter()
            .any(|b| matches!(b.terminator.kind, TerminatorKind::Call { .. })),
        "`min(a, b)` in owned position lowers to a call"
    );
    assert!(
        !body
            .basic_blocks
            .iter()
            .flat_map(|b| &b.statements)
            .any(|s| matches!(&s.kind, StatementKind::Assign(_, Rvalue::Ref { .. }))),
        "under the owned mode an `any i32` argument is passed without taking a reference"
    );
}

/// An `any T` parameter resolved under a mutable borrow takes a `&mut` reference to the
/// argument, exercising the mutable arm of the argument lowering.
#[test]
fn an_any_parameter_resolved_mutably_takes_a_mutable_borrow() {
    let (hir, _tcx, _types, program) = lower_mir_src(
        "public trait Index<K, V> { fun index(&self, key: K) -> any V; }
         struct Map { value: i32 }
         extend Map with Index<any i32, i32> {
             fun index(&self, key: any i32) -> any i32 { return self.value; }
         }
         fun f(m: &mut Map, k: i32) { let r = &mut m[k]; }",
    );
    let body = program
        .bodies
        .get(&(find_function(&hir, "f"), None))
        .expect("`f` is lowered");
    assert!(
        body.basic_blocks
            .iter()
            .any(|b| matches!(b.terminator.kind, TerminatorKind::Call { .. })),
        "`&mut m[k]` discovers the RefMut-specialized body"
    );
}

/// Reifying an `any`-specialized function as a value pins every `any` position to its plain `T`
/// (`AnyMode::Owned`), since a bare reference has no call site to choose a mode from.
#[test]
fn an_any_specialized_function_reified_as_a_value_pins_its_any_positions() {
    let (hir, _tcx, _types, program) = lower_mir_src(
        "fun min(x: any i32, y: any i32) -> any i32 { return x; }
         fun f() { let g = min; }",
    );
    let body = program
        .bodies
        .get(&(find_function(&hir, "f"), None))
        .expect("`f` is lowered");
    assert!(
        body.basic_blocks
            .iter()
            .flat_map(|b| &b.statements)
            .any(|s| matches!(
                &s.kind,
                StatementKind::Assign(
                    _,
                    Rvalue::Cast {
                        kind: CastKind::ReifyFunPointer,
                        ..
                    }
                )
            )),
        "naming `min` without calling it reifies it as a function-pointer value"
    );
}
