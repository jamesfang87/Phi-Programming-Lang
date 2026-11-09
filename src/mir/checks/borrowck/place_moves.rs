use crate::diagnostics::display::DisplayCtx;
use crate::diagnostics::mir::definite_init::report_move_out_of_array;
use crate::diagnostics::typeck::expr::{
    report_move_out_of_reference, report_owned_element_in_new_array,
};
use crate::driver::source::SrcSpan;
use crate::hir::Hir;
use crate::mir::{Body, Operand, Rvalue, StatementKind, lower::Mir, place_ty};
use crate::mir::{Place, Projection};
use crate::session::Session;
use crate::typeck::ty::ctx::TyCtx;
use crate::typeck::ty::{Ty, TyKind};

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
                    check_operand(session, hir, tcx, body, operand, statement.span);
                }
            }
        }
        for operand in block.terminator.kind.operands() {
            check_operand(session, hir, tcx, body, operand, block.terminator.span);
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
    hir: &Hir,
    tcx: &mut TyCtx,
    body: &Body,
    operand: &Operand,
    span: SrcSpan,
) {
    let Operand::Move(place) = operand else {
        return;
    };
    let moved_ty = place_ty(tcx, &body.local_decls, place);
    if tcx.is_copy(moved_ty) {
        return;
    }
    if moves_out_of_reference(tcx, body, place) {
        let cx = DisplayCtx::new(session, hir, tcx);
        report_move_out_of_reference(cx, moved_ty, span);
        return;
    }
    if element_of_array(place) && tcx.needs_drop(moved_ty) {
        let name = body.local_decls[place.local.index()]
            .name
            .unwrap_or_else(|| panic!("an indexed place always names a user-written local"));
        report_move_out_of_array(session, name, span);
    }
}

/// Returns whether `place` reaches the moved value through a dereference of a reference.
/// A dereference of an owned pointer does not count, because the pointer owns what it points at.
fn moves_out_of_reference(tcx: &mut TyCtx, body: &Body, place: &Place) -> bool {
    let last = place.projections.len().saturating_sub(1);
    for (index, projection) in place.projections.iter().enumerate() {
        if index == last || !matches!(projection, Projection::Deref) {
            continue;
        }
        let before = prefix_place(place, index);
        let ty = place_ty(tcx, &body.local_decls, &before);
        if matches!(tcx.kind(ty), TyKind::Ref { .. }) {
            return true;
        }
    }
    false
}

fn prefix_place(place: &Place, end: usize) -> Place {
    Place {
        local: place.local,
        projections: place.projections[..end].to_vec(),
    }
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
    use crate::testing::{mir_place_moves_accepts as accepts, mir_place_moves_rejects as rejects};

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

    #[test]
    fn moving_an_owning_field_out_of_a_reference_is_rejected() {
        rejects(
            "struct Holder { h: iso i32 }
             fun steal(r: &Holder) -> iso i32 { return r.h; }",
            "out of a reference",
        );
    }

    #[test]
    fn moving_a_nested_field_out_of_a_reference_is_rejected() {
        rejects(
            "struct Inner { h: iso i32 }
             struct Outer { inner: Inner }
             fun steal(o: &Outer) -> iso i32 { return o.inner.h; }",
            "out of a reference",
        );
    }

    #[test]
    fn moving_a_tuple_element_out_of_a_reference_is_rejected() {
        rejects(
            "fun pick(r: &(iso i32, i32)) -> iso i32 { return r.0; }",
            "out of a reference",
        );
    }

    #[test]
    fn copying_a_field_out_of_a_reference_is_fine() {
        accepts("struct Holder { n: i32 } fun f(r: &Holder) -> i32 { return r.n; }");
    }

    #[test]
    fn moving_a_field_out_of_an_owned_pointer_is_fine() {
        accepts(
            "struct Holder { h: iso i32 }
             fun f(r: iso Holder) -> iso i32 { return (*r).h; }",
        );
    }
}
