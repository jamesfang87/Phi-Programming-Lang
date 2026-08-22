use crate::diagnostics::mir::always_return::report_not_all_paths_return;
use crate::mir::{checks::lattice, lower::Mir, BasicBlock, Body, TerminatorKind};

#[derive(Clone, Copy, PartialEq)]
enum State {
    DoesReturn,
    DoesNotReturn,
}

type Lattice = lattice::Lattice<State>;

pub fn check(mir: &Mir) {
    for body in mir.bodies.values() {
        if !check_body(body) {
            report_not_all_paths_return(body.span);
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
    let mut lattice: Lattice = Default::default();
    let preds = body.predecessors();

    // We have a guess of `DoesReturn` for entry of all blocks
    // and a guess of `DoesNotReturn` for exit of all blocks
    for index in 0..body.basic_blocks.len() {
        let id = BasicBlock::from_usize(index);
        lattice.set_entry(id, State::DoesReturn);
        lattice.set_exit(id, State::DoesNotReturn);
    }

    let mut changed = true;
    while changed {
        changed = false;

        for (index, _basic_block) in body.basic_blocks.iter().enumerate() {
            let id = BasicBlock::from_usize(index);

            // First figure out the entry from the predecessors
            let pred_states: Vec<&State> = preds
                .of(id)
                .iter()
                .filter_map(|&pred| lattice.exit(pred))
                .collect();
            let old_entry = *lattice
                .entry(id)
                .expect("every block's entry is given above");
            let new_entry = meet(old_entry, &pred_states);
            lattice.set_entry(id, new_entry);

            // Update `changed` flag if changed
            if old_entry != new_entry {
                changed = true;
            }

            // Now the transfer function from entry to exit for this block
            let old_exit = *lattice.exit(id).expect("every block's exit is given above");
            let new_exit = match _basic_block.terminator.kind {
                TerminatorKind::Return | TerminatorKind::Assert { .. } => State::DoesReturn,
                _ => new_entry,
            };
            lattice.set_exit(id, new_exit);

            // Update `changed` flag if needed
            if old_exit != new_exit {
                changed = true;
            }
        }
    }

    (0..body.basic_blocks.len())
        .map(BasicBlock::from_usize)
        .filter(|&id| body.successors(id).next().is_none())
        .all(|id| lattice.exit(id) == Some(&State::DoesReturn))
}

#[cfg(test)]
mod tests {
    use super::check_body;
    use crate::diagnostics::DiagCtx;
    use crate::driver::cli::Mode;
    use crate::mir::lower::lower_program;
    use crate::testing::{first_function, resolve_src};
    use crate::typeck::{self, TypeckOutput};

    fn always_returns(src: &str) -> bool {
        let hir = resolve_src(src);
        DiagCtx::clear();
        let checked = typeck::check(&hir);
        let diagnostics = DiagCtx::diagnostics();
        assert!(diagnostics.is_empty(), "{src:?}: {diagnostics:?}");
        let TypeckOutput { mut tcx, types } = checked;
        let program = lower_program(&hir, &mut tcx, &types, Mode::Debug);
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
}
