use crate::diagnostics::mir::always_return::report_not_all_paths_return;
use crate::mir::{BasicBlock, Body, TerminatorKind, checks::lattice, lower::Mir};
use crate::session::Session;

#[derive(Clone, Copy, PartialEq)]
enum State {
    DoesReturn,
    DoesNotReturn,
}

pub fn check(session: &Session, mir: &Mir) {
    for body in mir.bodies.values() {
        if !check_body(body) {
            report_not_all_paths_return(session, body.span);
        }
    }
}

fn meet(cur_entry: State, predecessor_states: &[&State]) -> State {
    if predecessor_states.is_empty() {
        return cur_entry;
    }
    if predecessor_states
        .iter()
        .all(|state| **state == State::DoesReturn)
    {
        State::DoesReturn
    } else {
        State::DoesNotReturn
    }
}

fn check_body(body: &Body) -> bool {
    let lattice = lattice::solve(
        body,
        State::DoesReturn,
        State::DoesNotReturn,
        |current, pred_states| meet(*current, pred_states),
        |entry, _id, block| match block.terminator.kind {
            TerminatorKind::Return | TerminatorKind::Assert { .. } => State::DoesReturn,
            _ => *entry,
        },
    );

    (0..body.basic_blocks.len())
        .map(BasicBlock::from_usize)
        .filter(|&id| body.successors(id).next().is_none())
        .all(|id| lattice.exit(id) == Some(&State::DoesReturn))
}

#[cfg(test)]
mod tests {
    use super::check_body;
    use crate::testing::{OPS_PREAMBLE, first_function, lower_mir_src_files};

    fn always_returns(src: &str) -> bool {
        let (hir, _tcx, _types, program, _instances) = lower_mir_src_files(&[OPS_PREAMBLE, src]);
        let def_id = first_function(&hir);
        let body = program
            .bodies
            .get(&(def_id, None))
            .unwrap_or_else(|| panic!("no lowered body for the first function in {src:?}"));
        check_body(body)
    }

    #[test]
    fn a_bare_return_always_returns() {
        assert!(always_returns("fun f() -> i32 { return 1; }"));
    }

    #[test]
    fn falling_off_the_end_of_a_unit_function_always_returns() {
        assert!(always_returns("fun f() { let x = 1; }"));
    }

    #[test]
    fn an_if_else_that_returns_in_both_arms_always_returns() {
        assert!(always_returns(
            "fun f(x: i32) -> i32 { if x < 0 { return 0; } else { return x; } }"
        ));
    }

    #[test]
    fn an_if_with_no_else_that_falls_through_to_a_later_return_always_returns() {
        assert!(always_returns(
            "fun f(x: i32) -> i32 { if x < 0 { return 0; } return x; }"
        ));
    }

    #[test]
    fn an_if_with_no_else_in_a_unit_function_always_returns() {
        assert!(always_returns(
            "fun f(x: i32) { if x < 0 { return; } let y = 1; }"
        ));
    }

    #[test]
    fn nested_if_else_that_returns_on_every_path_always_returns() {
        assert!(always_returns(
            "fun f(x: i32) -> i32 {
                 if x < 0 {
                     if x < -10 { return -1; } else { return -2; }
                 } else {
                     return x;
                 }
             }"
        ));
    }

    #[test]
    fn a_returned_if_expression_always_returns() {
        assert!(always_returns(
            "fun f(x: i32) -> i32 { return if x < 0 { 0 } else { x }; }"
        ));
    }

    #[test]
    fn a_returned_match_over_an_enum_always_returns() {
        assert!(always_returns(
            "struct Rectangle { public l: f64, public w: f64 }
             enum Shape { rectangle: Rectangle, circle: f64 }
             fun area(s: Shape) -> f64 {
                 return match s {
                     .rectangle(r) => r.l * r.w,
                     .circle(radius) => radius,
                 };
             }"
        ));
    }

    #[test]
    fn a_match_with_a_return_in_every_arm_always_returns() {
        assert!(always_returns(
            "enum Shape { rectangle, circle }
             fun classify(s: Shape) -> i32 {
                 match s {
                     .rectangle => { return 1; }
                     .circle => { return 2; }
                 }
                 return 0;
             }"
        ));
    }

    #[test]
    fn a_while_loop_followed_by_a_return_always_returns() {
        assert!(always_returns(
            "fun f(x: i32) -> i32 {
                 while x > 0 {
                     x = x - 1;
                 }
                 return x;
             }"
        ));
    }

    #[test]
    fn an_early_return_inside_a_loop_followed_by_a_later_return_always_returns() {
        assert!(always_returns(
            "fun f(x: i32) -> i32 {
                 while x > 0 {
                     if x == 5 { return 5; }
                     x = x - 1;
                 }
                 return x;
             }"
        ));
    }

    #[test]
    fn checked_arithmetic_that_may_assert_still_always_returns() {
        assert!(always_returns(
            "fun add(x: i32, y: i32) -> i32 { return x + y; }"
        ));
    }

    #[test]
    fn a_recursive_function_always_returns() {
        assert!(always_returns(
            "fun fact(n: i32) -> i32 { if n <= 1 { return 1; } return n * fact(n - 1); }"
        ));
    }

    #[test]
    fn an_empty_match_that_proves_the_rest_of_the_function_unreachable_still_always_returns() {
        assert!(always_returns("fun f(x: i32) -> i32 { match x {}; }"));
    }

    #[test]
    fn a_function_that_only_panics_always_returns() {
        assert!(always_returns(
            r#"fun f() -> i32 { panic("not implemented"); }"#
        ));
    }

    #[test]
    fn a_function_that_only_reaches_unreachable_always_returns() {
        assert!(always_returns("fun f() -> i32 { unreachable(); }"));
    }
}
