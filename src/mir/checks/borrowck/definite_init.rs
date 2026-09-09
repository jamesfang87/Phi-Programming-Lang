use std::collections::HashSet;

use crate::diagnostics::mir::definite_init::report_use_of_moved_value;
use crate::driver::source::SrcSpan;
use crate::mir::checks::borrowck::{Register, register_of};
use crate::mir::{
    BasicBlock, Body, Operand, Place, Rvalue, Statement, StatementKind, Terminator, TerminatorKind,
    checks::lattice, lower::Mir,
};

/// The set of registers currently dead. A register's absence means it is live
type DeadRegisters = HashSet<Register>;
type Lattice = lattice::Lattice<BasicBlock, DeadRegisters>;

pub fn check(mir: &Mir) {
    for body in mir.bodies.values() {
        check_body(body);
    }
}

fn meet(predecessor_states: &[&DeadRegisters]) -> DeadRegisters {
    let mut merged = DeadRegisters::new();
    for state in predecessor_states {
        merged.extend((*state).iter().cloned());
    }
    merged
}

fn check_body(body: &Body) {
    let lattice = fixed_point(body);
    report_body(body, &lattice);
}

/// Terminates when the lattice settles into a fixed point.
/// This does not report any diagnostics.
fn fixed_point(body: &Body) -> Lattice {
    let mut lattice: Lattice = Default::default();
    let preds = body.predecessors();

    // We have a guess of "nothing moved yet" for entry and exit of all blocks
    for index in 0..body.basic_blocks.len() {
        let id = BasicBlock::from_usize(index);
        lattice.set_entry(id, DeadRegisters::default());
        lattice.set_exit(id, DeadRegisters::default());
    }

    let mut changed = true;
    while changed {
        changed = false;

        for (index, block) in body.basic_blocks.iter().enumerate() {
            let id = BasicBlock::from_usize(index);

            // First figure out the entry from the predecessors
            let pred_states: Vec<&DeadRegisters> = preds
                .of(id)
                .iter()
                .filter_map(|&pred| lattice.exit(pred))
                .collect();
            let old_entry = lattice
                .entry(id)
                .expect("every block's entry is given above")
                .clone();
            let new_entry = meet(&pred_states);

            // Update `changed` flag if changed
            if old_entry != new_entry {
                changed = true;
            }
            lattice.set_entry(id, new_entry.clone());

            // Now the transfer function from entry to exit for this block
            let mut new_exit = new_entry;
            for stmt in &block.statements {
                apply_statement(&mut new_exit, stmt);
            }
            apply_terminator(&mut new_exit, &block.terminator);

            // Update `changed` flag if needed
            let old_exit = lattice.exit(id).expect("every block's exit is given above");
            if *old_exit != new_exit {
                changed = true;
            }
            lattice.set_exit(id, new_exit);
        }
    }

    lattice
}

/// Report use-free errors in the body.
fn report_body(body: &Body, lattice: &Lattice) {
    for (index, block) in body.basic_blocks.iter().enumerate() {
        let id = BasicBlock::from_usize(index);
        let mut state = lattice
            .entry(id)
            .expect("every block's entry is given above")
            .clone();
        for stmt in &block.statements {
            check_statement(&mut state, body, stmt);
        }
        check_terminator(&mut state, body, &block.terminator);
    }
}

/// Checks if a register is dead.
fn is_dead(dead: &DeadRegisters, register: &Register) -> bool {
    (0..=register.subregister.len()).any(|len| {
        dead.contains(&Register {
            owner: register.owner,
            subregister: register.subregister[..len].to_vec(),
        })
    }) || dead
        .iter()
        .any(|d| d.owner == register.owner && d.subregister.starts_with(&register.subregister))
}

/// Sets the state of `register` to live
fn mark_live(dead: &mut DeadRegisters, register: &Register) {
    dead.retain(|d| d.owner != register.owner || !d.subregister.starts_with(&register.subregister));
}

fn apply_operand(dead: &mut DeadRegisters, operand: &Operand) {
    if let Operand::Move(place) = operand {
        dead.insert(register_of(place));
    }
}

fn apply_rvalue(dead: &mut DeadRegisters, rvalue: &Rvalue) {
    match rvalue {
        Rvalue::Use(operand)
        | Rvalue::UnaryOp(_, operand)
        | Rvalue::Cast { operand, .. }
        | Rvalue::New(operand) => {
            apply_operand(dead, operand);
        }
        Rvalue::BinaryOp(_, lhs, rhs)
        | Rvalue::CheckedBinaryOp(_, lhs, rhs)
        | Rvalue::NewArray {
            elem: lhs,
            count: rhs,
        } => {
            apply_operand(dead, lhs);
            apply_operand(dead, rhs);
        }
        Rvalue::Aggregate(_, operands) => {
            for operand in operands {
                apply_operand(dead, operand);
            }
        }
        Rvalue::Ref { .. } | Rvalue::Discriminant(_) | Rvalue::Len(_) => {}
    }
}

fn apply_statement(dead: &mut DeadRegisters, stmt: &Statement) {
    match &stmt.kind {
        StatementKind::StorageLive(local) | StatementKind::StorageDead(local) => {
            // We do the same for StorageLive for re-entry into loops
            dead.retain(|r| r.owner != *local);
            dead.insert(Register {
                owner: *local,
                subregister: Vec::new(),
            });
        }
        StatementKind::Assign(place, rvalue) => {
            apply_rvalue(dead, rvalue);
            mark_live(dead, &register_of(place));
        }
        StatementKind::PlaceMention(_)
        | StatementKind::SetDiscriminant { .. }
        | StatementKind::WithLend(_) => {}
    }
}

fn apply_terminator(dead: &mut DeadRegisters, terminator: &Terminator) {
    match &terminator.kind {
        TerminatorKind::SwitchInt { discr, .. } => apply_operand(dead, discr),
        TerminatorKind::Assert { cond, msg, .. } => {
            apply_operand(dead, cond);
            if let Some(msg) = msg.user_message() {
                apply_operand(dead, msg);
            }
        }
        TerminatorKind::Call {
            func,
            args,
            destination,
            ..
        } => {
            apply_operand(dead, func);
            for arg in args {
                apply_operand(dead, arg);
            }
            mark_live(dead, &register_of(destination));
        }
        TerminatorKind::Drop { place, .. } | TerminatorKind::DropIso { place, .. } => {
            dead.insert(register_of(place));
        }
        TerminatorKind::Goto { .. } | TerminatorKind::Return | TerminatorKind::Unreachable => {}
    }
}

fn check_read(dead: &DeadRegisters, body: &Body, place: &Place, span: SrcSpan) {
    let register = register_of(place);
    if is_dead(dead, &register) {
        let name = body.local_decls[register.owner.index()]
            .name
            .unwrap_or_else(|| {
                panic!("a local reachable through a use of a moved value is always named")
            });
        report_use_of_moved_value(name, span);
    }
}

fn check_operand(dead: &mut DeadRegisters, body: &Body, operand: &Operand, span: SrcSpan) {
    let place = match operand {
        Operand::Copy(place) | Operand::Move(place) => place,
        Operand::Constant(_) => return,
    };
    check_read(dead, body, place, span);
    apply_operand(dead, operand);
}

fn check_rvalue(dead: &mut DeadRegisters, body: &Body, rvalue: &Rvalue, span: SrcSpan) {
    match rvalue {
        Rvalue::Use(operand)
        | Rvalue::UnaryOp(_, operand)
        | Rvalue::Cast { operand, .. }
        | Rvalue::New(operand) => {
            check_operand(dead, body, operand, span);
        }
        Rvalue::BinaryOp(_, lhs, rhs)
        | Rvalue::CheckedBinaryOp(_, lhs, rhs)
        | Rvalue::NewArray {
            elem: lhs,
            count: rhs,
        } => {
            check_operand(dead, body, lhs, span);
            check_operand(dead, body, rhs, span);
        }
        Rvalue::Ref { place, .. } | Rvalue::Discriminant(place) | Rvalue::Len(place) => {
            check_read(dead, body, place, span);
        }
        Rvalue::Aggregate(_, operands) => {
            for operand in operands {
                check_operand(dead, body, operand, span);
            }
        }
    }
}

fn check_statement(dead: &mut DeadRegisters, body: &Body, stmt: &Statement) {
    match &stmt.kind {
        StatementKind::Assign(place, rvalue) => {
            check_rvalue(dead, body, rvalue, stmt.span);
            mark_live(dead, &register_of(place));
        }
        StatementKind::PlaceMention(place) => {
            check_read(dead, body, place, stmt.span);
        }
        StatementKind::StorageLive(_)
        | StatementKind::StorageDead(_)
        | StatementKind::SetDiscriminant { .. }
        | StatementKind::WithLend(_) => apply_statement(dead, stmt),
    }
}

fn check_terminator(dead: &mut DeadRegisters, body: &Body, terminator: &Terminator) {
    match &terminator.kind {
        TerminatorKind::SwitchInt { discr, .. } => {
            check_operand(dead, body, discr, terminator.span);
        }
        TerminatorKind::Assert { cond, msg, .. } => {
            check_operand(dead, body, cond, terminator.span);
            if let Some(msg) = msg.user_message() {
                check_operand(dead, body, msg, terminator.span);
            }
        }
        TerminatorKind::Call {
            func,
            args,
            destination,
            ..
        } => {
            check_operand(dead, body, func, terminator.span);
            for arg in args {
                check_operand(dead, body, arg, terminator.span);
            }
            mark_live(dead, &register_of(destination));
        }
        TerminatorKind::Drop { .. }
        | TerminatorKind::DropIso { .. }
        | TerminatorKind::Goto { .. }
        | TerminatorKind::Return
        | TerminatorKind::Unreachable => apply_terminator(dead, terminator),
    }
}

#[cfg(test)]
mod tests {
    use crate::testing::{
        mir_definite_init_accepts as accepts, mir_definite_init_rejects as rejects,
    };

    #[test]
    fn using_a_local_before_any_move_is_fine() {
        accepts("fun f() { let a = 1; let b = a; let _ = b; }");
    }

    #[test]
    fn a_let_bound_to_a_diverging_call_is_never_flagged_on_the_unreachable_fallthrough() {
        accepts("fun f() -> i32 { let x = panic(); return x; }");
    }

    #[test]
    fn using_a_local_after_it_was_moved_wholesale_is_rejected() {
        rejects(
            "struct P { x: i32, y: i32 }
             fun f() { let p = P { x: 1, y: 2 }; let q = p; let _ = q; let _ = p; }",
            "use of moved value `p`",
        );
    }

    #[test]
    fn moving_one_field_still_permits_reading_a_sibling_field() {
        accepts(
            "struct Inner { v: i32 }
             struct P { x: Inner, y: Inner }
             fun f() {
                 let p = P { x: Inner { v: 1 }, y: Inner { v: 2 } };
                 let x = p.x;
                 let _ = x;
                 let _ = p.y;
             }",
        );
    }

    #[test]
    fn moving_one_field_forbids_reading_that_same_field_again() {
        rejects(
            "struct Inner { v: i32 }
             struct P { x: Inner, y: Inner }
             fun f() {
                 let p = P { x: Inner { v: 1 }, y: Inner { v: 2 } };
                 let x = p.x;
                 let _ = x;
                 let _ = p.x;
             }",
            "use of moved value `p`",
        );
    }

    #[test]
    fn moving_one_field_forbids_using_the_whole_place_afterward() {
        rejects(
            "struct Inner { v: i32 }
             struct P { x: Inner, y: Inner }
             fun take(p: P) {}
             fun f() {
                 let p = P { x: Inner { v: 1 }, y: Inner { v: 2 } };
                 let x = p.x;
                 let _ = x;
                 take(p);
             }",
            "use of moved value `p`",
        );
    }

    #[test]
    fn a_move_on_only_one_branch_of_an_if_leaves_the_value_dead_afterward() {
        rejects(
            "struct P { v: i32 }
             fun f(cond: bool) {
                 let p = P { v: 1 };
                 if cond {
                     let q = p;
                     let _ = q;
                 }
                 let _ = p;
             }",
            "use of moved value `p`",
        );
    }

    #[test]
    fn a_move_on_every_branch_of_an_if_else_still_leaves_the_value_dead_afterward() {
        rejects(
            "struct P { v: i32 }
             fun f(cond: bool) {
                 let p = P { v: 1 };
                 if cond {
                     let q = p;
                     let _ = q;
                 } else {
                     let r = p;
                     let _ = r;
                 }
                 let _ = p;
             }",
            "use of moved value `p`",
        );
    }

    #[test]
    fn reassigning_after_a_move_cures_it() {
        accepts(
            "struct P { v: i32 }
             fun f(cond: bool) {
                 let mut p = P { v: 1 };
                 if cond {
                     let q = p;
                     let _ = q;
                 }
                 p = P { v: 2 };
                 let _ = p;
             }",
        );
    }

    #[test]
    fn copying_a_local_never_moves_it() {
        accepts("fun f() { let a = 1; let b = a; let c = a; let _ = b; let _ = c; }");
    }

    #[test]
    fn a_moved_local_reinitialized_by_a_loop_iteration_is_fine_on_the_next_one() {
        accepts(
            "struct P { v: i32 }
             fun f(n: i32) {
                 let mut i = 0;
                 while i < n {
                     let p = P { v: i };
                     let q = p;
                     let _ = q;
                     i = i + 1;
                 }
             }",
        );
    }

    #[test]
    fn reading_a_field_through_a_pointer_before_any_move_is_fine() {
        accepts(
            "struct S { x: i32 }
             fun f(p: &S) -> i32 { return (*p).x; }",
        );
    }

    #[test]
    fn moving_one_field_through_a_pointer_still_permits_reading_a_sibling_field() {
        accepts(
            "struct Inner { v: i32 }
             struct P { x: Inner, y: Inner }
             fun f(p: &mut P) {
                 let x = (*p).x;
                 let _ = x;
                 let _ = (*p).y;
             }",
        );
    }

    #[test]
    fn moving_one_field_through_a_pointer_forbids_reading_that_same_field_again() {
        rejects(
            "struct Inner { v: i32 }
             struct P { x: Inner, y: Inner }
             fun f(p: &mut P) {
                 let x = (*p).x;
                 let _ = x;
                 let _ = (*p).x;
             }",
            "use of moved value `p`",
        );
    }

    #[test]
    fn moving_a_field_through_a_pointer_on_only_one_branch_leaves_it_dead_afterward() {
        rejects(
            "struct Inner { v: i32 }
             struct P { x: Inner, y: Inner }
             fun f(p: &mut P, cond: bool) {
                 if cond {
                     let q = (*p).x;
                     let _ = q;
                 }
                 let _ = (*p).x;
             }",
            "use of moved value `p`",
        );
    }

    #[test]
    fn reassigning_a_field_through_a_pointer_after_moving_it_cures_it() {
        accepts(
            "struct Inner { v: i32 }
             struct P { x: Inner, y: Inner }
             fun f(p: &mut P) {
                 let q = (*p).x;
                 let _ = q;
                 (*p).x = Inner { v: 2 };
                 let _ = (*p).x;
             }",
        );
    }
}
