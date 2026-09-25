use std::collections::{HashMap, HashSet};

use crate::diagnostics::mir::returned_reference::report_returned_local_reference;
use crate::mir::checks::borrowck::lifetimes::{Alias, AliasId, Lifetimes, LifetimesMap};
use crate::mir::checks::borrowck::{Register, SubRegisters, register_of};
use crate::mir::lower::Mir;
use crate::mir::{Body, Local, Operand, Place, Rvalue, StatementKind, TerminatorKind};
use crate::session::Session;
use crate::typeck::ty::TyKind;
use crate::typeck::ty::ctx::TyCtx;

pub fn check(session: &Session, tcx: &TyCtx, mir: &Mir, lifetimes: &LifetimesMap) {
    for (key, body) in &mir.bodies {
        if let Some(body_lifetimes) = lifetimes.get(key) {
            check_body(session, tcx, body, body_lifetimes);
        }
    }
}

fn check_body(session: &Session, tcx: &TyCtx, body: &Body, lifetimes: &Lifetimes) {
    let tainted = tainted_locals(tcx, body, &lifetimes.aliases);

    for alias in lifetimes.aliases.values() {
        if !alias.attached.contains(&return_place()) {
            continue;
        }
        if reaches_local(body, &lifetimes.aliases, &tainted, &alias.register) {
            report_returned_local_reference(session, local_name(body, &alias.register), alias.span);
        }
    }

    if tainted.contains(&Local::RETURN_PLACE) {
        report_returned_local_reference(session, None, return_span(body));
    }
}

fn tainted_locals(tcx: &TyCtx, body: &Body, aliases: &HashMap<AliasId, Alias>) -> HashSet<Local> {
    let mut tainted = HashSet::new();
    loop {
        let mut changed = false;
        for block in &body.basic_blocks {
            for stmt in &block.statements {
                let StatementKind::Assign(
                    dest,
                    Rvalue::Use(Operand::Copy(src) | Operand::Move(src)),
                ) = &stmt.kind
                else {
                    continue;
                };
                if dest.projections.is_empty()
                    && src.projections.is_empty()
                    && tainted.contains(&src.local)
                {
                    changed |= tainted.insert(dest.local);
                }
            }

            let TerminatorKind::Call {
                args, destination, ..
            } = &block.terminator.kind
            else {
                continue;
            };
            if is_reference_typed(tcx, body, destination)
                && args
                    .iter()
                    .any(|arg| reaches_through_argument(body, aliases, &tainted, arg))
            {
                changed |= tainted.insert(destination.local);
            }
        }
        if !changed {
            return tainted;
        }
    }
}

fn reaches_local(
    body: &Body,
    aliases: &HashMap<AliasId, Alias>,
    tainted: &HashSet<Local>,
    register: &Register,
) -> bool {
    reaches_local_rec(body, aliases, tainted, register, &mut HashSet::new())
}

fn reaches_local_rec(
    body: &Body,
    aliases: &HashMap<AliasId, Alias>,
    tainted: &HashSet<Local>,
    register: &Register,
    on_stack: &mut HashSet<AliasId>,
) -> bool {
    if register.subregister.first() != Some(&SubRegisters::Deref) {
        return register.owner.index() > body.param_count;
    }

    let base = Register {
        owner: register.owner,
        subregister: Vec::new(),
    };
    let subregisters = &register.subregister[1..];
    for alias in aliases.values() {
        if !alias.attached.contains(&base) || !on_stack.insert(alias.id) {
            continue;
        }
        let mut pointee = alias.register.clone();
        pointee.subregister.extend_from_slice(subregisters);
        let found = reaches_local_rec(body, aliases, tainted, &pointee, on_stack);
        on_stack.remove(&alias.id);
        if found {
            return true;
        }
    }

    tainted.contains(&register.owner)
}

fn reaches_through_argument(
    body: &Body,
    aliases: &HashMap<AliasId, Alias>,
    tainted: &HashSet<Local>,
    operand: &Operand,
) -> bool {
    let (Operand::Copy(place) | Operand::Move(place)) = operand else {
        return false;
    };

    let register = register_of(place);
    let borrows = aliases.values().any(|alias| {
        alias.attached.contains(&register) && reaches_local(body, aliases, tainted, &alias.register)
    });
    borrows || tainted.contains(&place.local)
}

fn is_reference_typed(tcx: &TyCtx, body: &Body, place: &Place) -> bool {
    matches!(
        tcx.kind(body.local_decls[place.local.index()].ty),
        TyKind::Ref { .. }
    )
}

fn return_place() -> Register {
    Register {
        owner: Local::RETURN_PLACE,
        subregister: Vec::new(),
    }
}

fn return_span(body: &Body) -> crate::driver::source::SrcSpan {
    body.basic_blocks
        .iter()
        .find_map(|block| match block.terminator.kind {
            TerminatorKind::Return => Some(block.terminator.span),
            _ => None,
        })
        .unwrap_or(body.span)
}

fn local_name(body: &Body, register: &Register) -> Option<crate::ast::Ident> {
    let reaches_through_reference = register
        .subregister
        .iter()
        .any(|projection| matches!(projection, SubRegisters::Deref));
    if reaches_through_reference {
        return None;
    }
    body.local_decls[register.owner.index()].name
}

#[cfg(test)]
mod tests {
    use crate::testing::{
        mir_returned_reference_accepts as accepts, mir_returned_reference_rejects as rejects,
    };

    #[test]
    fn returning_a_borrow_of_a_local_is_rejected() {
        rejects(
            "fun f() -> &i32 { let x = 5; return &x; }",
            "cannot return a reference to `x`",
        );
    }

    #[test]
    fn returning_a_local_that_holds_a_borrow_of_another_local_is_rejected() {
        rejects(
            "fun f() -> &i32 { let x = 5; let r = &x; return r; }",
            "cannot return a reference to `x`",
        );
    }

    #[test]
    fn returning_a_borrow_of_a_temporary_is_rejected() {
        rejects(
            "fun f() -> &i32 { return &5; }",
            "cannot return a reference to a local variable",
        );
    }

    #[test]
    fn returning_a_reborrow_through_a_local_reference_is_rejected() {
        rejects(
            "fun f() -> &i32 { let x = 5; let r = &x; return &*r; }",
            "cannot return a reference to a local variable",
        );
    }

    #[test]
    fn returning_a_reborrow_through_a_chain_of_local_references_is_rejected() {
        rejects(
            "fun f() -> &i32 { let x = 5; let r = &x; let s = &*r; return &*s; }",
            "cannot return a reference to a local variable",
        );
    }

    #[test]
    fn returning_a_reference_parameter_is_fine() {
        accepts("fun f(p: &i32) -> &i32 { return p; }");
        accepts("fun f(p: &i32) -> &i32 { let r = p; return r; }");
    }

    #[test]
    fn returning_a_field_of_a_reference_parameter_is_fine() {
        accepts("struct S { x: i32 } fun f(p: &S) -> &i32 { return &p.x; }");
    }

    #[test]
    fn returning_a_reborrow_of_a_reference_parameter_is_fine() {
        accepts("fun f(p: &i32) -> &i32 { return &*p; }");
    }

    #[test]
    fn returning_a_reborrow_through_a_local_bound_to_a_parameter_is_fine() {
        accepts("fun f(p: &i32) -> &i32 { let r = p; return &*r; }");
        accepts("struct S { x: i32 } fun f(p: &S) -> &i32 { let r = p; return &r.x; }");
    }

    #[test]
    fn a_local_borrow_on_one_return_path_is_rejected_even_when_another_path_is_fine() {
        rejects(
            "fun f(c: bool, p: &i32) -> &i32 {
                 if c { let x = 5; return &x; } else { return p; }
             }",
            "cannot return a reference to `x`",
        );
    }

    #[test]
    fn a_local_borrow_on_an_early_return_path_is_rejected() {
        rejects(
            "fun f(c: bool, p: &i32) -> &i32 {
                 if c { return p; }
                 let x = 7;
                 return &x;
             }",
            "cannot return a reference to `x`",
        );
    }

    #[test]
    fn every_return_path_may_hand_back_a_reference_parameter() {
        accepts(
            "fun f(c: bool, p: &i32, q: &i32) -> &i32 {
                 if c { return p; } else { return q; }
             }",
        );
    }

    #[test]
    fn returning_a_call_result_that_projects_a_local_argument_is_rejected() {
        rejects(
            "fun foo(x: &i32) -> &i32 { return x; }
             fun bar() -> &i32 { let local = 5; return foo(&local); }",
            "cannot return a reference to a local variable",
        );
    }

    #[test]
    fn returning_a_call_result_stored_in_a_local_is_rejected() {
        rejects(
            "fun foo(x: &i32) -> &i32 { return x; }
             fun bar() -> &i32 { let local = 5; let r = foo(&local); return r; }",
            "cannot return a reference to a local variable",
        );
    }

    #[test]
    fn returning_a_call_result_through_a_local_reference_is_rejected() {
        rejects(
            "fun foo(x: &i32) -> &i32 { return x; }
             fun bar() -> &i32 { let local = 5; let r = foo(&local); return &*r; }",
            "cannot return a reference to a local variable",
        );
    }

    #[test]
    fn a_call_result_that_is_not_returned_is_fine() {
        accepts(
            "fun foo(x: &i32) -> &i32 { return x; }
             fun bar(p: &i32) -> i32 {
                 let local = 5;
                 let r = foo(&local);
                 return *r + *p;
             }",
        );
    }

    #[test]
    fn passing_a_reference_parameter_through_a_call_is_fine() {
        accepts(
            "fun foo(x: &i32) -> &i32 { return x; }
             fun bar(p: &i32) -> &i32 { return foo(p); }",
        );
    }

    #[test]
    fn passing_a_local_to_a_call_that_may_return_a_different_argument_is_conservatively_rejected() {
        // A deliberate cost of the conservative rule: `foo` returns `p`, not `&local`, but every
        // reference argument is assumed returnable.
        rejects(
            "fun foo(x: &i32, y: &i32) -> &i32 { return y; }
             fun bar(p: &i32) -> &i32 { let local = 5; return foo(&local, p); }",
            "cannot return a reference to a local variable",
        );
    }

    #[test]
    fn returning_a_call_result_of_an_unknown_callee_with_a_local_argument_is_rejected() {
        rejects(
            "fun apply(f: fun(&i32) -> &i32, x: &i32) -> &i32 { return f(x); }
             fun bar() -> &i32 {
                 let local = 5;
                 return apply(|x: &i32| -> &i32 { return x; }, &local);
             }",
            "cannot return a reference to a local variable",
        );
    }
}
