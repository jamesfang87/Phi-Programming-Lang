use crate::diagnostics::mir::definite_init::report_move_out_of_array;
use crate::driver::source::SrcSpan;
use crate::mir::checks::borrowck::element_of_array;
use crate::mir::{Body, Operand, StatementKind, lower::Mir, place_ty};
use crate::typeck::tyctx::TyCtx;

pub fn check(tcx: &mut TyCtx, mir: &Mir) {
    for body in mir.bodies.values() {
        check_body(tcx, body);
    }
}

fn check_body(tcx: &mut TyCtx, body: &Body) {
    for block in &body.basic_blocks {
        for statement in &block.statements {
            if let StatementKind::Assign(_, rvalue) = &statement.kind {
                for operand in rvalue.operands() {
                    check_operand(tcx, body, operand, statement.span);
                }
            }
        }
        for operand in block.terminator.kind.operands() {
            check_operand(tcx, body, operand, block.terminator.span);
        }
    }
}

fn check_operand(tcx: &mut TyCtx, body: &Body, operand: &Operand, span: SrcSpan) {
    let Operand::Move(place) = operand else {
        return;
    };
    if !element_of_array(place) {
        return;
    }
    let moved_ty = place_ty(tcx, &body.local_decls, place);
    if !tcx.needs_drop(moved_ty) {
        return;
    }
    let name = body.local_decls[place.local.index()]
        .name
        .unwrap_or_else(|| panic!("an indexed place always names a user-written local"));
    report_move_out_of_array(name, span);
}

#[cfg(test)]
mod tests {
    use crate::testing::{
        mir_element_moves_accepts as accepts, mir_element_moves_rejects as rejects,
    };

    #[test]
    fn copying_an_array_element_is_fine() {
        accepts("fun f(n: usize) { let a = new [1; n]; let x = (*a)[0]; let _ = x; }");
    }

    #[test]
    fn moving_a_whole_array_is_fine() {
        accepts("fun take(a: iso [i32]) {} fun f(n: usize) { let a = new [1; n]; take(a); }");
    }

    #[test]
    fn moving_an_element_that_owns_nothing_is_fine() {
        accepts(
            "struct Plain { v: i32 }
             fun f(n: usize) { let a = new [Plain { v: 1 }; n]; let x = (*a)[0]; let _ = x; }",
        );
    }

    #[test]
    fn moving_an_owning_element_out_of_an_array_is_rejected() {
        rejects(
            "struct Owner { h: iso i32 }
             fun take(o: Owner) {}
             fun f(a: iso [Owner]) { take((*a)[0]); }",
            "cannot move a value out of `a`, which is an array",
        );
    }

    #[test]
    fn moving_an_owning_element_at_a_computed_index_is_rejected_too() {
        rejects(
            "struct Owner { h: iso i32 }
             fun take(o: Owner) {}
             fun f(a: iso [Owner], i: usize) { take((*a)[i]); }",
            "cannot move a value out of `a`, which is an array",
        );
    }
}
