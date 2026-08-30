use std::collections::HashSet;

use crate::diagnostics::mir::never_read::report_value_never_read;
use crate::driver::source::SrcSpan;
use crate::mir::{
    BasicBlock, Body, Local, Operand, Place, Rvalue, Statement, StatementKind, Terminator,
    TerminatorKind, checks::lattice, lower::Mir,
};

type UnreadLocals = HashSet<Local>;

type Lattice = lattice::Lattice<BasicBlock, UnreadLocals>;

pub fn check(mir: &Mir) {
    for body in mir.bodies.values() {
        check_body(body);
    }
}

fn meet(predecessor_states: &[&UnreadLocals]) -> UnreadLocals {
    let mut merged = UnreadLocals::new();
    for state in predecessor_states {
        merged.extend((*state).iter().cloned());
    }
    merged
}

fn check_body(body: &Body) {
    let lattice = fixed_point(body);
    report_body(body, &lattice);
}

fn fixed_point(body: &Body) -> Lattice {
    let mut lattice: Lattice = Default::default();
    let preds = body.predecessors();

    for index in 0..body.basic_blocks.len() {
        let id = BasicBlock::from_usize(index);
        lattice.set_entry(id, UnreadLocals::default());
        lattice.set_exit(id, UnreadLocals::default());
    }

    let mut changed = true;
    while changed {
        changed = false;

        for (index, block) in body.basic_blocks.iter().enumerate() {
            let id = BasicBlock::from_usize(index);

            let pred_states: Vec<&UnreadLocals> = preds
                .of(id)
                .iter()
                .filter_map(|&pred| lattice.exit(pred))
                .collect();
            let old_entry = lattice
                .entry(id)
                .expect("every block's entry is given above")
                .clone();
            let new_entry = meet(&pred_states);

            if old_entry != new_entry {
                changed = true;
            }
            lattice.set_entry(id, new_entry.clone());

            let mut new_exit = new_entry;
            for stmt in &block.statements {
                apply_statement(&mut new_exit, stmt);
            }
            apply_terminator(&mut new_exit, &block.terminator);

            let old_exit = lattice.exit(id).expect("every block's exit is given above");
            if *old_exit != new_exit {
                changed = true;
            }
            lattice.set_exit(id, new_exit);
        }
    }

    lattice
}

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
        apply_terminator(&mut state, &block.terminator);
    }
}

fn apply_place(unread: &mut UnreadLocals, place: &Place) {
    unread.remove(&place.local);
}

fn apply_operand(unread: &mut UnreadLocals, operand: &Operand) {
    match operand {
        Operand::Copy(place) | Operand::Move(place) => apply_place(unread, place),
        Operand::Constant(_) => {}
    }
}

fn apply_rvalue(unread: &mut UnreadLocals, rvalue: &Rvalue) {
    match rvalue {
        Rvalue::Use(operand) | Rvalue::UnaryOp(_, operand) | Rvalue::Cast { operand, .. } => {
            apply_operand(unread, operand);
        }
        Rvalue::BinaryOp(_, lhs, rhs) | Rvalue::CheckedBinaryOp(_, lhs, rhs) => {
            apply_operand(unread, lhs);
            apply_operand(unread, rhs);
        }
        Rvalue::Aggregate(_, operands) => {
            for operand in operands {
                apply_operand(unread, operand);
            }
        }
        Rvalue::Ref { place, .. } | Rvalue::Discriminant(place) | Rvalue::Len(place) => {
            apply_place(unread, place);
        }
    }
}

fn apply_statement(unread: &mut UnreadLocals, stmt: &Statement) {
    match &stmt.kind {
        StatementKind::StorageLive(local) | StatementKind::StorageDead(local) => {
            unread.remove(local);
        }
        StatementKind::Assign(place, rvalue) => {
            apply_rvalue(unread, rvalue);
            unread.insert(place.local);
        }
        StatementKind::PlaceMention(place) => {
            apply_place(unread, place);
        }
        StatementKind::SetDiscriminant { .. } | StatementKind::CheckMutable(_) => {}
    }
}

fn apply_terminator(unread: &mut UnreadLocals, terminator: &Terminator) {
    match &terminator.kind {
        TerminatorKind::SwitchInt { discr, .. } => apply_operand(unread, discr),
        TerminatorKind::Assert { cond, .. } => apply_operand(unread, cond),
        TerminatorKind::Call {
            func,
            args,
            destination,
            ..
        } => {
            apply_operand(unread, func);
            for arg in args {
                apply_operand(unread, arg);
            }
            unread.insert(destination.local);
        }
        TerminatorKind::Drop { place, .. } => {
            apply_place(unread, place);
        }
        TerminatorKind::Goto { .. } | TerminatorKind::Return | TerminatorKind::Unreachable => {}
    }
}

fn check_never_read(unread: &UnreadLocals, body: &Body, local: Local, span: SrcSpan) {
    if unread.contains(&local)
        && let Some(name) = body.local_decls[local.index()].name
    {
        report_value_never_read(name, span);
    }
}

fn check_statement(unread: &mut UnreadLocals, body: &Body, stmt: &Statement) {
    match &stmt.kind {
        StatementKind::StorageDead(local) => {
            check_never_read(unread, body, *local, stmt.span);
            apply_statement(unread, stmt);
        }
        StatementKind::Assign(place, rvalue) => {
            apply_rvalue(unread, rvalue);
            check_never_read(unread, body, place.local, stmt.span);
            unread.insert(place.local);
        }
        StatementKind::StorageLive(_)
        | StatementKind::PlaceMention(_)
        | StatementKind::SetDiscriminant { .. }
        | StatementKind::CheckMutable(_) => apply_statement(unread, stmt),
    }
}

#[cfg(test)]
mod tests {
    use crate::testing::{mir_never_read_accepts as accepts, mir_never_read_rejects as rejects};

    #[test]
    fn a_local_that_is_read_after_being_assigned_is_fine() {
        accepts("fun f() { let a = 1; let _ = a; }");
    }

    #[test]
    fn a_local_assigned_and_never_read_before_it_goes_out_of_scope_is_rejected() {
        rejects(
            "fun f() { let a = 1; }",
            "value assigned to `a` is never read",
        );
    }

    #[test]
    fn a_local_read_by_a_binary_op_is_fine() {
        accepts("fun f() { let a = 1; let b = 2; let _ = a + b; }");
    }

    #[test]
    fn reassigning_a_local_before_it_is_read_is_still_rejected() {
        rejects(
            "fun f() { let mut a = 1; a = 2; let _ = a; }",
            "value assigned to `a` is never read",
        );
    }

    #[test]
    fn a_local_read_on_only_one_branch_of_an_if_is_still_rejected() {
        rejects(
            "fun f(cond: bool) { let a = 1; if cond { let _ = a; } }",
            "value assigned to `a` is never read",
        );
    }

    #[test]
    fn a_local_read_on_every_branch_of_an_if_else_is_fine() {
        accepts(
            "fun f(cond: bool) {
                 let a = 1;
                 if cond { let _ = a; } else { let _ = a; }
             }",
        );
    }

    #[test]
    fn a_local_read_through_a_reference_is_fine() {
        accepts("fun f() { let a = 1; let _ = &a; }");
    }

    #[test]
    fn moving_a_local_into_a_call_counts_as_reading_it() {
        rejects(
            "fun take(x: i32) {}
             fun f() { let a = 1; take(a); let b = 2; }",
            "value assigned to `b` is never read",
        );
    }

    #[test]
    fn reading_one_field_of_a_struct_local_counts_as_reading_the_whole_local() {
        accepts(
            "struct P { x: i32, y: i32 }
             fun f() {
                 let p = P { x: 1, y: 2 };
                 let _ = p.x;
             }",
        );
    }
}
