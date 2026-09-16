use std::collections::HashSet;

use crate::ast::Ident;
use crate::diagnostics::mir::never_read::{report_parameter_never_read, report_value_never_read};
use crate::driver::source::SrcSpan;
use crate::mir::{
    BasicBlock, Body, Local, Operand, Place, Rvalue, Statement, StatementKind, Terminator,
    TerminatorKind, checks::lattice, lower::Mir,
};
use crate::session::Session;

type UnreadLocals = HashSet<Local>;

type Lattice = lattice::Lattice<BasicBlock, UnreadLocals>;

pub fn check(session: &Session, mir: &Mir) {
    for body in mir.bodies.values() {
        check_body(session, body);
    }
}

fn meet(current: &UnreadLocals, predecessor_states: &[&UnreadLocals]) -> UnreadLocals {
    match predecessor_states.split_first() {
        None => current.clone(),
        Some((first, rest)) => rest.iter().fold((*first).clone(), |acc, state| {
            acc.intersection(state).cloned().collect()
        }),
    }
}

fn check_body(session: &Session, body: &Body) {
    let tracked: Vec<bool> = body
        .local_decls
        .iter()
        .map(|decl| decl.name.is_some())
        .collect();
    let initial: UnreadLocals = (1..=body.param_count)
        .map(Local::from_usize)
        .filter(|local| tracked[local.index()])
        .collect();
    let lattice = fixed_point(body, &tracked, &initial);
    report_body(session, body, &tracked, &lattice);
}

fn fixed_point(body: &Body, tracked: &[bool], initial: &UnreadLocals) -> Lattice {
    lattice::solve(
        body,
        initial.clone(),
        UnreadLocals::default(),
        meet,
        |entry, _id, block| {
            let mut state = entry.clone();
            for stmt in &block.statements {
                apply_statement(&mut state, tracked, stmt);
            }
            apply_terminator(&mut state, tracked, &block.terminator);
            state
        },
    )
}

fn report_body(session: &Session, body: &Body, tracked: &[bool], lattice: &Lattice) {
    for (index, block) in body.basic_blocks.iter().enumerate() {
        let id = BasicBlock::from_usize(index);
        let mut state = lattice
            .entry(id)
            .expect("every block's entry is given above")
            .clone();
        for stmt in &block.statements {
            check_statement(session, &mut state, tracked, body, stmt);
        }
        apply_terminator(&mut state, tracked, &block.terminator);
    }
    if let Some(exit) = exit_state(body, lattice) {
        for index in 0..body.local_decls.len() {
            let local = Local::from_usize(index);
            if exit.contains(&local)
                && let Some(name) = reported_name(session, body, local)
            {
                let span = body.local_decls[index].span;
                if index <= body.param_count {
                    report_parameter_never_read(session, name, span);
                } else {
                    report_value_never_read(session, name, span);
                }
            }
        }
    }
}

fn exit_state(body: &Body, lattice: &Lattice) -> Option<UnreadLocals> {
    (0..body.basic_blocks.len())
        .map(BasicBlock::from_usize)
        .filter(|&id| body.successors(id).next().is_none())
        .filter_map(|id| lattice.exit(id).cloned())
        .reduce(|acc, state| acc.intersection(&state).cloned().collect())
}

fn reported_name(session: &Session, body: &Body, local: Local) -> Option<Ident> {
    let name = body.local_decls[local.index()].name?;
    (!session.resolve(name.text).starts_with('_')).then_some(name)
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
        Rvalue::Use(operand)
        | Rvalue::UnaryOp(_, operand)
        | Rvalue::Cast { operand, .. }
        | Rvalue::New(operand)
        | Rvalue::Unsize { operand, .. } => {
            apply_operand(unread, operand);
        }
        Rvalue::BinaryOp(_, lhs, rhs)
        | Rvalue::CheckedBinaryOp(_, lhs, rhs)
        | Rvalue::NewArray {
            elem: lhs,
            count: rhs,
        } => {
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

fn apply_statement(unread: &mut UnreadLocals, tracked: &[bool], stmt: &Statement) {
    match &stmt.kind {
        StatementKind::StorageLive(local) | StatementKind::StorageDead(local) => {
            unread.remove(local);
        }
        StatementKind::Assign(place, rvalue) => {
            apply_rvalue(unread, rvalue);
            if tracked[place.local.index()] {
                unread.insert(place.local);
            }
        }
        StatementKind::PlaceMention(place) => {
            apply_place(unread, place);
        }
        StatementKind::SetDiscriminant { .. } | StatementKind::WithLend(_) => {}
    }
}

fn apply_terminator(unread: &mut UnreadLocals, tracked: &[bool], terminator: &Terminator) {
    match &terminator.kind {
        TerminatorKind::SwitchInt { discr, .. } => apply_operand(unread, discr),
        TerminatorKind::Assert { cond, msg, .. } => {
            apply_operand(unread, cond);
            if let Some(msg) = msg.user_message() {
                apply_operand(unread, msg);
            }
        }
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
            if tracked[destination.local.index()] {
                unread.insert(destination.local);
            }
        }
        TerminatorKind::Drop { place, .. } | TerminatorKind::DropIso { place, .. } => {
            apply_place(unread, place);
        }
        TerminatorKind::Goto { .. } | TerminatorKind::Return | TerminatorKind::Unreachable => {}
    }
}

fn check_never_read(
    session: &Session,
    unread: &UnreadLocals,
    body: &Body,
    local: Local,
    span: SrcSpan,
) {
    if unread.contains(&local)
        && let Some(name) = reported_name(session, body, local)
    {
        report_value_never_read(session, name, span);
    }
}

fn check_statement(
    session: &Session,
    unread: &mut UnreadLocals,
    tracked: &[bool],
    body: &Body,
    stmt: &Statement,
) {
    match &stmt.kind {
        StatementKind::StorageDead(local) => {
            check_never_read(session, unread, body, *local, stmt.span);
            apply_statement(unread, tracked, stmt);
        }
        StatementKind::Assign(place, rvalue) => {
            apply_rvalue(unread, rvalue);
            check_never_read(session, unread, body, place.local, stmt.span);
            if tracked[place.local.index()] {
                unread.insert(place.local);
            }
        }
        StatementKind::StorageLive(_)
        | StatementKind::PlaceMention(_)
        | StatementKind::SetDiscriminant { .. }
        | StatementKind::WithLend(_) => apply_statement(unread, tracked, stmt),
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
    fn a_local_read_on_only_one_branch_of_an_if_is_fine() {
        // `a` is read whenever `cond` is true, so the assignment is not dead: liveness only
        // needs a read on some path, not every path.
        accepts("fun f(cond: bool) { let a = 1; if cond { let _ = a; } }");
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
            "fun take(_x: i32) {}
             fun f() { let a = 1; take(a); let b = 2; }",
            "value assigned to `b` is never read",
        );
    }

    #[test]
    fn a_local_whose_name_starts_with_underscore_is_never_reported() {
        accepts("fun f() { let _unused = 1; }");
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

    #[test]
    fn a_parameter_read_nowhere_in_the_body_is_rejected() {
        rejects("fun f(x: i32) {}", "parameter `x` is never read");
    }

    #[test]
    fn a_parameter_that_is_read_is_fine() {
        accepts("fun f(x: i32) { let _ = x; }");
    }

    #[test]
    fn a_parameter_read_by_a_binary_op_is_fine() {
        accepts("fun f(x: i32) { let _ = x + 1; }");
    }

    #[test]
    fn a_parameter_read_on_only_one_branch_is_fine() {
        accepts("fun f(x: i32, cond: bool) { if cond { let _ = x; } }");
    }

    #[test]
    fn a_parameter_whose_name_starts_with_underscore_is_never_reported() {
        accepts("fun f(_x: i32) {}");
    }

    #[test]
    fn an_unused_self_parameter_is_never_reported() {
        accepts(
            "struct P { v: i32 }
             extend P { fun get(&self) -> i32 { return 1; } }",
        );
    }

    #[test]
    fn only_the_unused_parameter_of_several_is_reported() {
        rejects(
            "fun f(a: i32, b: i32) { let _ = a; }",
            "parameter `b` is never read",
        );
    }
}
