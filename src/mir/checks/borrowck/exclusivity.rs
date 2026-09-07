use std::collections::{HashMap, HashSet};

use crate::ast::Mutability;
use crate::diagnostics::mir::exclusivity::report_exclusivity_violation;
use crate::driver::source::SrcSpan;
use crate::mir::checks::borrowck::lifetimes::{self, Alias, AliasId, Lifetimes};
use crate::mir::checks::borrowck::{Register, register_of};
use crate::mir::{
    BasicBlock, BasicBlockData, Body, Operand, Place, Rvalue, Statement, StatementKind, Terminator,
    TerminatorKind, lower::Mir,
};

#[derive(Clone, Copy, PartialEq, Eq)]
enum AccessKind {
    Read,
    Write,
}

pub fn check(mir: &Mir) {
    let lifetimes = lifetimes::compute(mir);
    for (key, body) in &mir.bodies {
        if let Some(body_lifetimes) = lifetimes.get(key) {
            check_body(body, body_lifetimes);
        }
    }
}

fn check_body(body: &Body, lifetimes: &Lifetimes) {
    for (index, block) in body.basic_blocks.iter().enumerate() {
        let id = BasicBlock::from_usize(index);
        let live_at_point = live_aliases_by_point(id, block, lifetimes);
        for (stmt_index, stmt) in block.statements.iter().enumerate() {
            check_statement(body, &lifetimes.aliases, &live_at_point[stmt_index], stmt);
        }
        check_terminator(
            body,
            &lifetimes.aliases,
            &live_at_point[block.statements.len()],
            &block.terminator,
        );
    }
}

fn live_aliases_by_point(
    id: BasicBlock,
    block: &BasicBlockData,
    lifetimes: &Lifetimes,
) -> Vec<HashSet<AliasId>> {
    let mut live_at = vec![HashSet::new(); block.statements.len() + 1];
    for (&alias_id, ranges) in &lifetimes.live_ranges {
        if let Some(range) = ranges.get(&id) {
            for point in range.clone() {
                live_at[point].insert(alias_id);
            }
        }
    }
    live_at
}

fn registers_conflict(a: &Register, b: &Register) -> bool {
    a.owner == b.owner
        && (a.subregister.starts_with(&b.subregister) || b.subregister.starts_with(&a.subregister))
}

fn access_kind_for_borrow(mutability: Mutability) -> AccessKind {
    match mutability {
        Mutability::Mutable => AccessKind::Write,
        Mutability::Immutable => AccessKind::Read,
    }
}

fn report_conflict(body: &Body, register: &Register, span: SrcSpan) {
    if let Some(name) = body.local_decls[register.owner.index()].name {
        report_exclusivity_violation(name, span);
    }
}

fn check_register_access(
    body: &Body,
    aliases: &HashMap<AliasId, Alias>,
    live: &HashSet<AliasId>,
    register: &Register,
    kind: AccessKind,
    span: SrcSpan,
) {
    for alias_id in live {
        let Some(alias) = aliases.get(alias_id) else {
            continue;
        };
        if !registers_conflict(register, &alias.borrows) {
            continue;
        }
        let conflicts = match kind {
            AccessKind::Read => alias.kind == Mutability::Mutable,
            AccessKind::Write => true,
        };
        if conflicts {
            report_conflict(body, register, span);
            return;
        }
    }
}

fn check_place_access(
    body: &Body,
    aliases: &HashMap<AliasId, Alias>,
    live: &HashSet<AliasId>,
    place: &Place,
    kind: AccessKind,
    span: SrcSpan,
) {
    check_register_access(body, aliases, live, &register_of(place), kind, span);
}

fn check_operand(
    body: &Body,
    aliases: &HashMap<AliasId, Alias>,
    live: &HashSet<AliasId>,
    operand: &Operand,
    span: SrcSpan,
) {
    match operand {
        Operand::Copy(place) => {
            check_place_access(body, aliases, live, place, AccessKind::Read, span)
        }
        Operand::Move(place) => {
            check_place_access(body, aliases, live, place, AccessKind::Write, span)
        }
        Operand::Constant(_) => {}
    }
}

fn check_assign_rvalue(
    body: &Body,
    aliases: &HashMap<AliasId, Alias>,
    live: &HashSet<AliasId>,
    rvalue: &Rvalue,
    span: SrcSpan,
) {
    match rvalue {
        Rvalue::Ref { mutability, place } => {
            check_place_access(
                body,
                aliases,
                live,
                place,
                access_kind_for_borrow(*mutability),
                span,
            );
        }
        Rvalue::Use(operand)
        | Rvalue::UnaryOp(_, operand)
        | Rvalue::Cast { operand, .. }
        | Rvalue::New(operand) => {
            check_operand(body, aliases, live, operand, span);
        }
        Rvalue::BinaryOp(_, lhs, rhs)
        | Rvalue::CheckedBinaryOp(_, lhs, rhs)
        | Rvalue::NewArray {
            elem: lhs,
            count: rhs,
        } => {
            check_operand(body, aliases, live, lhs, span);
            check_operand(body, aliases, live, rhs, span);
        }
        Rvalue::Aggregate(_, operands) => {
            for operand in operands {
                check_operand(body, aliases, live, operand, span);
            }
        }
        Rvalue::Discriminant(place) | Rvalue::Len(place) => {
            check_place_access(body, aliases, live, place, AccessKind::Read, span);
        }
    }
}

fn check_statement(
    body: &Body,
    aliases: &HashMap<AliasId, Alias>,
    live: &HashSet<AliasId>,
    stmt: &Statement,
) {
    match &stmt.kind {
        StatementKind::Assign(place, rvalue) => {
            check_assign_rvalue(body, aliases, live, rvalue, stmt.span);
            check_place_access(body, aliases, live, place, AccessKind::Write, stmt.span);
        }
        StatementKind::PlaceMention(place) => {
            check_place_access(body, aliases, live, place, AccessKind::Read, stmt.span);
        }
        StatementKind::SetDiscriminant { place, .. } => {
            check_place_access(body, aliases, live, place, AccessKind::Write, stmt.span);
        }
        StatementKind::StorageLive(_)
        | StatementKind::StorageDead(_)
        | StatementKind::CheckMutable(_)
        | StatementKind::WithLend(_) => {}
    }
}

fn check_terminator(
    body: &Body,
    aliases: &HashMap<AliasId, Alias>,
    live: &HashSet<AliasId>,
    terminator: &Terminator,
) {
    match &terminator.kind {
        TerminatorKind::SwitchInt { discr, .. } => {
            check_operand(body, aliases, live, discr, terminator.span);
        }
        TerminatorKind::Assert { cond, msg, .. } => {
            check_operand(body, aliases, live, cond, terminator.span);
            if let Some(msg) = msg.user_message() {
                check_operand(body, aliases, live, msg, terminator.span);
            }
        }
        TerminatorKind::Call {
            func,
            args,
            destination,
            ..
        } => {
            check_operand(body, aliases, live, func, terminator.span);
            for arg in args {
                check_operand(body, aliases, live, arg, terminator.span);
            }
            check_place_access(
                body,
                aliases,
                live,
                destination,
                AccessKind::Write,
                terminator.span,
            );
        }
        TerminatorKind::Drop { place, .. } => {
            check_place_access(
                body,
                aliases,
                live,
                place,
                AccessKind::Write,
                terminator.span,
            );
        }
        TerminatorKind::Goto { .. } | TerminatorKind::Return | TerminatorKind::Unreachable => {}
    }
}

#[cfg(test)]
mod tests {
    use crate::testing::{mir_exclusivity_accepts as accepts, mir_exclusivity_rejects as rejects};

    #[test]
    fn writing_to_a_local_with_no_live_borrows_is_fine() {
        accepts("fun f() { let mut a = 1; a = 2; let _ = a; }");
    }

    #[test]
    fn two_shared_borrows_alive_at_once_are_fine() {
        accepts(
            "fun f(a: i32) {
                 let r = &a;
                 let s = &a;
                 let _ = *r;
                 let _ = *s;
             }",
        );
    }

    #[test]
    fn writing_to_a_local_while_a_shared_borrow_of_it_is_alive_is_rejected() {
        rejects(
            "fun f() {
                 let mut a = 1;
                 let r = &a;
                 a = 2;
                 let _ = *r;
             }",
            "cannot use `a`",
        );
    }

    #[test]
    fn creating_a_mutable_borrow_while_a_shared_borrow_is_alive_is_rejected() {
        rejects(
            "fun f() {
                 let mut a = 1;
                 let r = &a;
                 let s = &mut a;
                 let _ = *r;
                 let _ = *s;
             }",
            "cannot use `a`",
        );
    }

    #[test]
    fn creating_a_second_mutable_borrow_while_the_first_is_alive_is_rejected() {
        rejects(
            "fun f() {
                 let mut a = 1;
                 let r = &mut a;
                 let s = &mut a;
                 let _ = *r;
                 let _ = *s;
             }",
            "cannot use `a`",
        );
    }

    #[test]
    fn reading_through_a_shared_borrow_while_another_shared_borrow_is_alive_is_fine() {
        accepts(
            "fun f(a: i32) {
                 let r = &a;
                 let s = &a;
                 let _ = *r + *s;
             }",
        );
    }

    #[test]
    fn using_a_mutable_borrow_after_its_prior_use_is_over_is_fine() {
        accepts(
            "fun f() {
                 let mut a = 1;
                 let r = &mut a;
                 let _ = *r;
                 a = 2;
                 let _ = a;
             }",
        );
    }

    #[test]
    fn borrowing_two_disjoint_fields_mutably_at_once_is_fine() {
        accepts(
            "struct P { x: i32, y: i32 }
             fun f(p: P) {
                 let mut p = p;
                 let rx = &mut p.x;
                 let ry = &mut p.y;
                 let _ = *rx;
                 let _ = *ry;
             }",
        );
    }

    #[test]
    fn writing_to_one_field_while_a_mutable_borrow_of_that_same_field_is_alive_is_rejected() {
        rejects(
            "struct P { x: i32, y: i32 }
             fun f(p: P) {
                 let mut p = p;
                 let r = &mut p.x;
                 p.x = 5;
                 let _ = *r;
             }",
            "cannot use `p`",
        );
    }

    #[test]
    fn writing_to_a_whole_struct_while_one_field_is_mutably_borrowed_is_rejected() {
        rejects(
            "struct P { x: i32, y: i32 }
             fun f(p: P, q: P) {
                 let mut p = p;
                 let r = &mut p.x;
                 p = q;
                 let _ = *r;
             }",
            "cannot use `p`",
        );
    }

    #[test]
    fn reading_a_field_through_a_shared_pointer_with_no_live_borrows_is_fine() {
        accepts(
            "struct S { x: i32 }
             fun f(p: &S) -> i32 { return (*p).x; }",
        );
    }

    #[test]
    fn writing_a_field_through_a_mutable_pointer_with_no_live_borrows_is_fine() {
        accepts(
            "struct S { x: i32 }
             fun f(p: &mut S) { (*p).x = 5; }",
        );
    }

    #[test]
    fn two_shared_borrows_of_the_same_field_through_a_pointer_are_fine() {
        accepts(
            "struct S { x: i32 }
             fun f(p: &S) {
                 let r = &(*p).x;
                 let s = &(*p).x;
                 let _ = *r + *s;
             }",
        );
    }

    #[test]
    fn borrowing_two_disjoint_fields_through_a_pointer_mutably_at_once_is_fine() {
        accepts(
            "struct S { x: i32, y: i32 }
             fun f(p: &mut S) {
                 let rx = &mut (*p).x;
                 let ry = &mut (*p).y;
                 let _ = *rx;
                 let _ = *ry;
             }",
        );
    }

    #[test]
    fn mutably_borrowing_a_field_through_a_pointer_while_a_shared_borrow_of_it_is_alive_is_rejected()
     {
        rejects(
            "struct S { x: i32 }
             fun f(p: &mut S) {
                 let r = &(*p).x;
                 let s = &mut (*p).x;
                 let _ = *r;
                 let _ = *s;
             }",
            "cannot use `p`",
        );
    }

    #[test]
    fn writing_a_field_through_a_pointer_while_a_shared_borrow_of_that_field_is_alive_is_rejected()
    {
        rejects(
            "struct S { x: i32 }
             fun f(p: &mut S) {
                 let r = &(*p).x;
                 (*p).x = 5;
                 let _ = *r;
             }",
            "cannot use `p`",
        );
    }

    #[test]
    fn an_immutable_reborrow_of_the_whole_pointee_conflicts_with_writing_one_of_its_fields() {
        rejects(
            "struct S { x: i32 }
             fun f(p: &mut S) {
                 let r = &*p;
                 (*p).x = 5;
                 let _ = (*r).x;
             }",
            "cannot use `p`",
        );
    }

    #[test]
    fn a_mutable_reborrow_of_the_whole_pointee_conflicts_with_reading_one_of_its_fields() {
        rejects(
            "struct S { x: i32 }
             fun f(p: &mut S) -> i32 {
                 let r = &mut *p;
                 let v = (*p).x;
                 let _ = (*r).x;
                 return v;
             }",
            "cannot use `p`",
        );
    }

    #[test]
    fn a_mutable_borrow_of_a_field_conflicts_with_a_mutable_reborrow_of_the_whole_pointee() {
        rejects(
            "struct S { x: i32 }
             fun f(p: &mut S) {
                 let r = &mut (*p).x;
                 let s = &mut *p;
                 let _ = *r;
                 let _ = (*s).x;
             }",
            "cannot use `p`",
        );
    }

    #[test]
    fn two_immutable_reborrows_of_the_whole_pointee_are_fine() {
        accepts(
            "struct S { x: i32 }
             fun f(p: &S) -> i32 {
                 let q = &*p;
                 return (*p).x + (*q).x;
             }",
        );
    }

    #[test]
    fn a_ref_self_call_through_a_pointer_is_fine_with_another_shared_borrow_alive() {
        accepts(
            "struct Counter { n: i32 }
             extend Counter { fun peek(&self) -> i32 { return self.n; } }
             fun f(p: &Counter) -> i32 {
                 let r = &(*p).n;
                 let v = p.peek();
                 return v + *r;
             }",
        );
    }

    #[test]
    fn a_mut_self_call_through_a_pointer_reborrows_the_whole_pointee_and_conflicts_with_a_live_field_borrow()
     {
        rejects(
            "struct Counter { n: i32 }
             extend Counter { fun bump(&mut self) { self.n = self.n + 1; } }
             fun f(p: &mut Counter) {
                 let r = &(*p).n;
                 p.bump();
                 let _ = *r;
             }",
            "cannot use `p`",
        );
    }

    #[test]
    fn a_ref_self_call_through_a_pointer_conflicts_with_a_live_mutable_field_borrow() {
        rejects(
            "struct Counter { n: i32 }
             extend Counter { fun peek(&self) -> i32 { return self.n; } }
             fun f(p: &mut Counter) -> i32 {
                 let r = &mut (*p).n;
                 let v = p.peek();
                 let _ = *r;
                 return v;
             }",
            "cannot use `p`",
        );
    }

    #[test]
    fn a_mut_self_call_through_a_pointer_is_fine_once_the_earlier_borrow_is_over() {
        accepts(
            "struct Counter { n: i32 }
             extend Counter { fun bump(&mut self) { self.n = self.n + 1; } }
             fun f(p: &mut Counter) {
                 let r = &(*p).n;
                 let _ = *r;
                 p.bump();
             }",
        );
    }
}
