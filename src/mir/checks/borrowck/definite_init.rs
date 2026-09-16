use std::collections::HashSet;

use crate::diagnostics::mir::definite_init::report_use_of_moved_value;
use crate::driver::source::SrcSpan;
use crate::mir::checks::borrowck::{Register, register_of, trivially_copyable};
use crate::mir::{
    BasicBlock, Body, Operand, Place, Rvalue, Statement, StatementKind, Terminator, TerminatorKind,
    checks::lattice, lower::Mir,
};
use crate::session::Session;
use crate::typeck::ty::ctx::TyCtx;

/// The set of registers currently dead. A register's absence means it is live.
type DeadRegisters = HashSet<Register>;
type Lattice = lattice::Lattice<BasicBlock, DeadRegisters>;

/// Per local: whether the local's value can be moved at all. A trivially copyable local cannot,
/// so it is excluded from the state; see [`trivially_copyable`].
type Movable = [bool];

pub fn check(session: &Session, tcx: &TyCtx, mir: &Mir) {
    for body in mir.bodies.values() {
        check_body(session, tcx, body);
    }
}

fn meet(predecessor_states: &[&DeadRegisters]) -> DeadRegisters {
    let mut merged = DeadRegisters::new();
    for state in predecessor_states {
        merged.extend((*state).iter().cloned());
    }
    merged
}

fn check_body(session: &Session, tcx: &TyCtx, body: &Body) {
    let movable: Vec<bool> = body
        .local_decls
        .iter()
        .map(|decl| decl.name.is_some() && !trivially_copyable(tcx, decl.ty))
        .collect();
    let lattice = fixed_point(body, &movable);
    report_body(session, body, &movable, &lattice);
}

fn fixed_point(body: &Body, movable: &Movable) -> Lattice {
    lattice::solve(
        body,
        DeadRegisters::default(),
        DeadRegisters::default(),
        |_current, pred_states| meet(pred_states),
        |entry, _id, block| {
            let mut state = entry.clone();
            for stmt in &block.statements {
                apply_statement(&mut state, movable, stmt);
            }
            apply_terminator(&mut state, movable, &block.terminator);
            state
        },
    )
}

/// Report use-free errors in the body.
fn report_body(session: &Session, body: &Body, movable: &Movable, lattice: &Lattice) {
    for (index, block) in body.basic_blocks.iter().enumerate() {
        let id = BasicBlock::from_usize(index);
        let mut state = lattice
            .entry(id)
            .expect("every block's entry is given above")
            .clone();
        for stmt in &block.statements {
            check_statement(session, &mut state, movable, body, stmt);
        }
        check_terminator(session, &mut state, movable, body, &block.terminator);
    }
}

/// Checks if a register is dead.
fn is_dead(dead: &DeadRegisters, movable: &Movable, register: &Register) -> bool {
    if !movable[register.owner.index()] {
        return false;
    }
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
fn mark_live(dead: &mut DeadRegisters, movable: &Movable, register: &Register) {
    if !movable[register.owner.index()] {
        return;
    }
    dead.retain(|d| d.owner != register.owner || !d.subregister.starts_with(&register.subregister));
}

/// Records `register` as dead, unless its owner cannot move.
fn mark_dead(dead: &mut DeadRegisters, movable: &Movable, register: Register) {
    if movable[register.owner.index()] {
        dead.insert(register);
    }
}

fn apply_operand(dead: &mut DeadRegisters, movable: &Movable, operand: &Operand) {
    if let Operand::Move(place) = operand {
        mark_dead(dead, movable, register_of(place));
    }
}

fn apply_rvalue(dead: &mut DeadRegisters, movable: &Movable, rvalue: &Rvalue) {
    match rvalue {
        Rvalue::Use(operand)
        | Rvalue::UnaryOp(_, operand)
        | Rvalue::Cast { operand, .. }
        | Rvalue::New(operand)
        | Rvalue::Unsize { operand, .. } => {
            apply_operand(dead, movable, operand);
        }
        Rvalue::BinaryOp(_, lhs, rhs)
        | Rvalue::CheckedBinaryOp(_, lhs, rhs)
        | Rvalue::NewArray {
            elem: lhs,
            count: rhs,
        } => {
            apply_operand(dead, movable, lhs);
            apply_operand(dead, movable, rhs);
        }
        Rvalue::Aggregate(_, operands) => {
            for operand in operands {
                apply_operand(dead, movable, operand);
            }
        }
        Rvalue::Ref { .. } | Rvalue::Discriminant(_) | Rvalue::Len(_) => {}
    }
}

fn apply_statement(dead: &mut DeadRegisters, movable: &Movable, stmt: &Statement) {
    match &stmt.kind {
        StatementKind::StorageLive(local) | StatementKind::StorageDead(local) => {
            if movable[local.index()] {
                // We do the same for StorageLive for re-entry into loops
                dead.retain(|r| r.owner != *local);
                dead.insert(Register {
                    owner: *local,
                    subregister: Vec::new(),
                });
            }
        }
        StatementKind::Assign(place, rvalue) => {
            apply_rvalue(dead, movable, rvalue);
            mark_live(dead, movable, &register_of(place));
        }
        StatementKind::PlaceMention(_)
        | StatementKind::SetDiscriminant { .. }
        | StatementKind::WithLend(_) => {}
    }
}

fn apply_terminator(dead: &mut DeadRegisters, movable: &Movable, terminator: &Terminator) {
    match &terminator.kind {
        TerminatorKind::SwitchInt { discr, .. } => apply_operand(dead, movable, discr),
        TerminatorKind::Assert { cond, msg, .. } => {
            apply_operand(dead, movable, cond);
            if let Some(msg) = msg.user_message() {
                apply_operand(dead, movable, msg);
            }
        }
        TerminatorKind::Call {
            func,
            args,
            destination,
            ..
        } => {
            apply_operand(dead, movable, func);
            for arg in args {
                apply_operand(dead, movable, arg);
            }
            mark_live(dead, movable, &register_of(destination));
        }
        TerminatorKind::Drop { place, .. } | TerminatorKind::DropIso { place, .. } => {
            mark_dead(dead, movable, register_of(place));
        }
        TerminatorKind::Goto { .. } | TerminatorKind::Return | TerminatorKind::Unreachable => {}
    }
}

fn check_read(
    session: &Session,
    dead: &DeadRegisters,
    movable: &Movable,
    body: &Body,
    place: &Place,
    span: SrcSpan,
) {
    let register = register_of(place);
    if is_dead(dead, movable, &register) {
        let name = body.local_decls[register.owner.index()]
            .name
            .unwrap_or_else(|| {
                panic!("a local reachable through a use of a moved value is always named")
            });
        report_use_of_moved_value(session, name, span);
    }
}

fn check_operand(
    session: &Session,
    dead: &mut DeadRegisters,
    movable: &Movable,
    body: &Body,
    operand: &Operand,
    span: SrcSpan,
) {
    let place = match operand {
        Operand::Copy(place) | Operand::Move(place) => place,
        Operand::Constant(_) => return,
    };
    check_read(session, dead, movable, body, place, span);
    apply_operand(dead, movable, operand);
}

fn check_rvalue(
    session: &Session,
    dead: &mut DeadRegisters,
    movable: &Movable,
    body: &Body,
    rvalue: &Rvalue,
    span: SrcSpan,
) {
    match rvalue {
        Rvalue::Use(operand)
        | Rvalue::UnaryOp(_, operand)
        | Rvalue::Cast { operand, .. }
        | Rvalue::New(operand)
        | Rvalue::Unsize { operand, .. } => {
            check_operand(session, dead, movable, body, operand, span);
        }
        Rvalue::BinaryOp(_, lhs, rhs)
        | Rvalue::CheckedBinaryOp(_, lhs, rhs)
        | Rvalue::NewArray {
            elem: lhs,
            count: rhs,
        } => {
            check_operand(session, dead, movable, body, lhs, span);
            check_operand(session, dead, movable, body, rhs, span);
        }
        Rvalue::Ref { place, .. } | Rvalue::Discriminant(place) | Rvalue::Len(place) => {
            check_read(session, dead, movable, body, place, span);
        }
        Rvalue::Aggregate(_, operands) => {
            for operand in operands {
                check_operand(session, dead, movable, body, operand, span);
            }
        }
    }
}

fn check_statement(
    session: &Session,
    dead: &mut DeadRegisters,
    movable: &Movable,
    body: &Body,
    stmt: &Statement,
) {
    match &stmt.kind {
        StatementKind::Assign(place, rvalue) => {
            check_rvalue(session, dead, movable, body, rvalue, stmt.span);
            mark_live(dead, movable, &register_of(place));
        }
        StatementKind::PlaceMention(place) => {
            check_read(session, dead, movable, body, place, stmt.span);
        }
        StatementKind::StorageLive(_)
        | StatementKind::StorageDead(_)
        | StatementKind::SetDiscriminant { .. }
        | StatementKind::WithLend(_) => apply_statement(dead, movable, stmt),
    }
}

fn check_terminator(
    session: &Session,
    dead: &mut DeadRegisters,
    movable: &Movable,
    body: &Body,
    terminator: &Terminator,
) {
    match &terminator.kind {
        TerminatorKind::SwitchInt { discr, .. } => {
            check_operand(session, dead, movable, body, discr, terminator.span);
        }
        TerminatorKind::Assert { cond, msg, .. } => {
            check_operand(session, dead, movable, body, cond, terminator.span);
            if let Some(msg) = msg.user_message() {
                check_operand(session, dead, movable, body, msg, terminator.span);
            }
        }
        TerminatorKind::Call {
            func,
            args,
            destination,
            ..
        } => {
            check_operand(session, dead, movable, body, func, terminator.span);
            for arg in args {
                check_operand(session, dead, movable, body, arg, terminator.span);
            }
            mark_live(dead, movable, &register_of(destination));
        }
        TerminatorKind::Drop { .. }
        | TerminatorKind::DropIso { .. }
        | TerminatorKind::Goto { .. }
        | TerminatorKind::Return
        | TerminatorKind::Unreachable => apply_terminator(dead, movable, terminator),
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
