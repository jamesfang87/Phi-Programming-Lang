use crate::diagnostics::display::DisplayCtx;
use crate::diagnostics::mir::definite_init::report_move_out_of_array;
use crate::diagnostics::typeck::expr::report_owned_element_in_new_array;
use crate::driver::source::SrcSpan;
use crate::hir::Hir;
use crate::mir::{Body, Operand, Rvalue, StatementKind, lower::Mir, place_ty};
use crate::mir::{Place, Projection};
use crate::session::Session;
use crate::typeck::ty::Ty;
use crate::typeck::ty::ctx::TyCtx;

pub fn check(session: &Session, hir: &Hir, tcx: &mut TyCtx, mir: &Mir) {
    for body in mir.bodies.values() {
        check_body(session, hir, tcx, body);
    }
}

fn check_body(session: &Session, hir: &Hir, tcx: &mut TyCtx, body: &Body) {
    for block in &body.basic_blocks {
        for statement in &block.statements {
            if let StatementKind::Assign(_, rvalue) = &statement.kind {
                if let Rvalue::NewArray { elem, .. } = rvalue {
                    check_new_array_repeat(session, hir, tcx, body, elem, statement.span);
                }
                for operand in rvalue.operands() {
                    check_operand(session, tcx, body, operand, statement.span);
                }
            }
        }
        for operand in block.terminator.kind.operands() {
            check_operand(session, tcx, body, operand, block.terminator.span);
        }
    }
}

fn check_new_array_repeat(
    session: &Session,
    hir: &Hir,
    tcx: &mut TyCtx,
    body: &Body,
    elem: &Operand,
    span: SrcSpan,
) {
    let elem_ty = operand_ty(tcx, body, elem);
    if tcx.needs_drop(elem_ty) {
        report_owned_element_in_new_array(DisplayCtx::new(session, hir, tcx), elem_ty, span);
    }
}

fn operand_ty(tcx: &mut TyCtx, body: &Body, operand: &Operand) -> Ty {
    match operand {
        Operand::Copy(place) | Operand::Move(place) => place_ty(tcx, &body.local_decls, place),
        Operand::Constant(constant) => constant.ty,
    }
}

fn check_operand(
    session: &Session,
    tcx: &mut TyCtx,
    body: &Body,
    operand: &Operand,
    span: SrcSpan,
) {
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
    report_move_out_of_array(session, name, span);
}

pub(crate) fn element_of_array(place: &Place) -> bool {
    place.projections.iter().any(|projection| {
        matches!(
            projection,
            Projection::Index(_) | Projection::ConstantIndex(_)
        )
    })
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

    #[test]
    fn repeating_an_owning_element_is_rejected() {
        rejects(
            "fun f(n: usize) { let y = new [new 1; n]; }",
            "cannot repeat an owning element",
        );
    }

    #[test]
    fn repeating_an_element_with_an_owning_field_is_rejected() {
        rejects(
            "struct Handle { owned: iso i32 }
             fun f(n: usize) { let y = new [Handle { owned: new 1 }; n]; }",
            "cannot repeat an owning element",
        );
    }

    #[test]
    fn repeating_a_plain_element_is_allowed() {
        accepts("fun f(n: usize) { let y = new [0_u8; n]; }");
    }
}
