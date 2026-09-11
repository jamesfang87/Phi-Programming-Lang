use crate::diagnostics::mir::captures::{
    report_captured_reference, report_move_out_of_environment,
};
use crate::diagnostics::typeck::display::DisplayCx;
use crate::driver::source::SrcSpan;
use crate::mir::{Body, DefKind, Local, Operand, StatementKind, lower::Mir};
use crate::typeck::ty::TyKind;
use crate::typeck::tyctx::TyCtx;

const ENVIRONMENT: Local = Local::ENVIRONMENT;

pub fn check(tcx: &mut TyCtx, mir: &Mir) {
    for (&(def, _), body) in &mir.bodies {
        if mir.def_infos.kind(def) == DefKind::Closure {
            check_captured_types(tcx, mir, body);
            check_body(body);
        }
    }
}

fn check_captured_types(tcx: &TyCtx, mir: &Mir, body: &Body) {
    let environment = body.local_decls[ENVIRONMENT.index()].ty;
    let TyKind::Ref { base, .. } = *tcx.kind(environment) else {
        panic!("a closure body's environment local is a reference to the environment it borrows");
    };
    let TyKind::Tuple(fields) = tcx.kind(base).clone() else {
        panic!("a closure's environment is a tuple of its drop-glue word and its captures");
    };
    for &capture in fields.iter().skip(1) {
        if tcx.contains_ref(capture) {
            report_captured_reference(DisplayCx::for_mir(&mir.def_names, tcx), capture, body.span);
        }
    }
}

fn check_body(body: &Body) {
    for block in &body.basic_blocks {
        for statement in &block.statements {
            if let StatementKind::Assign(_, rvalue) = &statement.kind {
                for operand in rvalue.operands() {
                    check_operand(operand, statement.span);
                }
            }
        }
        for operand in block.terminator.kind.operands() {
            check_operand(operand, block.terminator.span);
        }
    }
}

fn check_operand(operand: &Operand, span: SrcSpan) {
    if let Operand::Move(place) = operand
        && place.local == ENVIRONMENT
    {
        report_move_out_of_environment(span);
    }
}

#[cfg(test)]
mod tests {
    use crate::testing::{mir_captures_accepts as accepts, mir_captures_rejects as rejects};

    #[test]
    fn reading_a_captured_value_is_fine() {
        accepts("fun f() -> i32 { let n = 1; let g = || n + 1; return g(); }");
    }

    #[test]
    fn reading_through_a_captured_owning_value_is_fine() {
        accepts("fun f() -> i32 { let n = new 1; let g = || *n; return g(); }");
    }

    #[test]
    fn a_closure_that_captures_nothing_has_nothing_to_move_out() {
        accepts("fun f() -> i32 { let g = || 1; return g(); }");
    }

    #[test]
    fn capturing_a_reference_is_rejected() {
        rejects(
            "fun f() -> fun() -> i32 { let n = 7; let r = &n; return || *r; }",
            "a closure cannot capture a reference",
        );
    }

    #[test]
    fn capturing_an_owned_value_is_fine() {
        accepts("fun f() -> fun() -> i32 { let n = new 7; return || *n; }");
    }

    #[test]
    fn moving_a_captured_owning_value_out_is_rejected() {
        rejects(
            "fun consume(p: iso i32) {}
             fun f() { let n = new 1; let g = || consume(n); g(); }",
            "cannot move a captured value out of a closure's environment",
        );
    }

    #[test]
    fn moving_a_captured_value_into_a_binding_is_rejected_too() {
        rejects(
            "struct Owner { h: iso i32 }
             fun f() { let o = Owner { h: new 1 }; let g = || { let taken = o; }; g(); }",
            "cannot move a captured value out of a closure's environment",
        );
    }
}
