use crate::hir::DefId;
use crate::mir::checks::borrowck::{Register, to_register};
use crate::mir::{Body, Local, Place, Projection, VariantIdx};
use crate::typeck::ty::ctx::TyCtx;
use crate::typeck::ty::{Ty, TyKind};

use super::move_state::{MoveState, Ownership};

/// A tree of drop plans: glue, allocation frees, per-variant arms, and flag-guarded subtrees,
/// sequenced in the order they must run.
#[derive(Debug)]
pub(super) enum DropNode {
    Glue(Place),
    FreeAllocation(Place),
    Seq(Vec<DropNode>),
    Variants {
        place: Place,
        arms: Vec<(VariantIdx, DropNode)>,
    },
    Flagged {
        flag: Local,
        inner: Box<DropNode>,
    },
}

/// The drop-flag locals a body's plan reserves, and the registers they stand for.
pub(super) struct FlagLocals {
    bool_ty: Ty,
    first: usize,
    reserved: Vec<(Register, Local)>,
}

impl FlagLocals {
    pub(super) fn new(bool_ty: Ty, body: &Body) -> FlagLocals {
        FlagLocals {
            bool_ty,
            first: body.local_decls.len(),
            reserved: Vec::new(),
        }
    }

    /// Returns the flag local for `place`, reserving one if this is the first request for it.
    fn reserve_flag(&mut self, place: &Place) -> Local {
        let register = to_register(place);
        if let Some((_, local)) = self.reserved.iter().find(|(held, _)| *held == register) {
            return *local;
        }
        let local = Local::from_usize(self.first + self.reserved.len());
        self.reserved.push((register, local));
        local
    }

    pub(super) fn flags(&self) -> &[(Register, Local)] {
        &self.reserved
    }

    pub(super) fn declare(&self, body: &mut Body) {
        for (_, local) in &self.reserved {
            assert_eq!(
                body.local_decls.len(),
                local.index(),
                "a drop flag was reserved on a slot something else has since taken"
            );
            body.local_decls.push(crate::mir::LocalDecl {
                ty: self.bool_ty,
                name: None,
                span: body.span,
            });
        }
    }
}

/// Returns the plan that drops `place`, or `None` when `place` never owns anything to drop.
pub(super) fn plan(
    tcx: &mut TyCtx,
    state: &MoveState,
    flags: &mut FlagLocals,
    place: Place,
    ty: Ty,
    flagged: Option<&Register>,
) -> Option<DropNode> {
    if !tcx.needs_drop(ty) {
        return None;
    }
    match ownership_of(state, &place, flagged) {
        Ownership::Moved => None,
        Ownership::MaybeMoved => plan_flagged(tcx, state, flags, place, ty),
        Ownership::Owned => plan_owned(tcx, state, flags, place, ty, flagged),
    }
}

/// Returns whether `place` may already have been moved, checking within `flagged` when the place
/// sits under a drop flag.
fn ownership_of(state: &MoveState, place: &Place, flagged: Option<&Register>) -> Ownership {
    match flagged {
        Some(covered) => state.ownership_within(place, covered),
        None => state.ownership_of(place),
    }
}

/// Returns a plan guarded by a drop flag, since `place` was only maybe moved.
fn plan_flagged(
    tcx: &mut TyCtx,
    state: &MoveState,
    flags: &mut FlagLocals,
    place: Place,
    ty: Ty,
) -> Option<DropNode> {
    let flag = flags.reserve_flag(&place);
    let covered = to_register(&place);
    let inner = plan(tcx, state, flags, place, ty, Some(&covered))?;
    Some(DropNode::Flagged {
        flag,
        inner: Box::new(inner),
    })
}

/// Returns the plan for a place known to still own its value.
fn plan_owned(
    tcx: &mut TyCtx,
    state: &MoveState,
    flags: &mut FlagLocals,
    place: Place,
    ty: Ty,
    flagged: Option<&Register>,
) -> Option<DropNode> {
    match tcx.kind(ty).clone() {
        TyKind::Iso(pointee) => plan_iso(tcx, state, flags, place, pointee, flagged),
        TyKind::Array { .. } | TyKind::Fun { .. } => Some(DropNode::Glue(place)),
        TyKind::Tuple(elems) => plan_fields(tcx, state, flags, place, elems, flagged),
        TyKind::Adt { def, args } => plan_adt(tcx, state, flags, place, def, args, flagged),
        other => unreachable!("no drop plan for {other:?}, which `needs_drop` says owns something"),
    }
}

/// Returns the plan for an `iso`: free the allocation whether or not the pointee is dropped.
fn plan_iso(
    tcx: &mut TyCtx,
    state: &MoveState,
    flags: &mut FlagLocals,
    place: Place,
    pointee: Ty,
    flagged: Option<&Register>,
) -> Option<DropNode> {
    if state.owns_every_part(&place) {
        return Some(DropNode::Glue(place));
    }
    let pointee_place = project_place(&place, Projection::Deref);
    let surviving = plan(tcx, state, flags, pointee_place, pointee, flagged);
    let free = DropNode::FreeAllocation(place);
    Some(match surviving {
        Some(surviving) => DropNode::Seq(vec![surviving, free]),
        None => free,
    })
}

/// Returns the plan that drops each field of a struct or tuple in turn.
fn plan_fields(
    tcx: &mut TyCtx,
    state: &MoveState,
    flags: &mut FlagLocals,
    place: Place,
    field_tys: Vec<Ty>,
    flagged: Option<&Register>,
) -> Option<DropNode> {
    sequence_drops(
        field_tys
            .into_iter()
            .enumerate()
            .filter_map(|(index, field_ty)| {
                let field = project_place(&place, Projection::Field(index as u32));
                plan(tcx, state, flags, field, field_ty, flagged)
            }),
    )
}

fn plan_adt(
    tcx: &mut TyCtx,
    state: &MoveState,
    flags: &mut FlagLocals,
    place: Place,
    def: DefId,
    args: Vec<Ty>,
    flagged: Option<&Register>,
) -> Option<DropNode> {
    match tcx.enum_variant_count(def) {
        None => {
            let field_tys = tcx.struct_field_tys(def, &args);
            plan_fields(tcx, state, flags, place, field_tys, flagged)
        }
        Some(variant_count) => {
            plan_variants(tcx, state, flags, place, def, &args, variant_count, flagged)
        }
    }
}

/// Returns the plan that drops whichever variant a `place` holds, one arm per variant.
#[allow(clippy::too_many_arguments)]
fn plan_variants(
    tcx: &mut TyCtx,
    state: &MoveState,
    flags: &mut FlagLocals,
    place: Place,
    def: DefId,
    args: &[Ty],
    variant_count: usize,
    flagged: Option<&Register>,
) -> Option<DropNode> {
    let mut arms = Vec::new();
    for variant in 0..variant_count {
        let field_tys = tcx.variant_field_tys(def, args, variant);
        let fields = field_tys
            .into_iter()
            .enumerate()
            .filter_map(|(index, field_ty)| {
                let mut field = project_place(
                    &place,
                    Projection::Downcast(VariantIdx::from_usize(variant)),
                );
                field.projections.push(Projection::Field(index as u32));
                plan(tcx, state, flags, field, field_ty, flagged)
            });
        if let Some(arm) = sequence_drops(fields) {
            arms.push((VariantIdx::from_usize(variant), arm));
        }
    }
    (!arms.is_empty()).then_some(DropNode::Variants { place, arms })
}

/// Returns `place` with `projection` pushed onto it.
fn project_place(place: &Place, projection: Projection) -> Place {
    let mut projected = place.clone();
    projected.projections.push(projection);
    projected
}

/// Returns a sequence of the planned drops, or `None` when there are none.
fn sequence_drops(nodes: impl Iterator<Item = DropNode>) -> Option<DropNode> {
    let nodes: Vec<DropNode> = nodes.collect();
    (!nodes.is_empty()).then_some(DropNode::Seq(nodes))
}
