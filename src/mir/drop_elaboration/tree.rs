use crate::mir::checks::borrowck::{Register, register_of};
use crate::mir::{Body, Local, Place, Projection, VariantIdx};
use crate::typeck::ty::{Ty, TyKind};
use crate::typeck::tyctx::TyCtx;

use super::move_state::{MoveState, Ownership};

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

    fn flag_for(&mut self, place: &Place) -> Local {
        let register = register_of(place);
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

fn variant_field_ty(tcx: &mut TyCtx, enum_ty: Ty, variant: usize, field: usize) -> Ty {
    let TyKind::Adt { def, args } = tcx.kind(enum_ty).clone() else {
        panic!("variant_field_ty: {enum_ty:?} is not an enum");
    };
    tcx.variant_field_tys(def, &args, variant)[field]
}

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

    let status = match flagged {
        Some(covered) => state.status_within(&place, covered),
        None => state.status(&place),
    };
    match status {
        Ownership::Moved => return None,
        Ownership::Maybe => {
            let flag = flags.flag_for(&place);
            let covered = register_of(&place);
            let inner = plan(tcx, state, flags, place, ty, Some(&covered))?;
            return Some(DropNode::Flagged {
                flag,
                inner: Box::new(inner),
            });
        }
        Ownership::Owned => {}
    }

    match tcx.kind(ty).clone() {
        TyKind::Iso(pointee) => {
            if state.owns_every_part(&place) {
                return Some(DropNode::Glue(place));
            }
            let pointee_place = projected(&place, Projection::Deref);
            let surviving = plan(tcx, state, flags, pointee_place, pointee, flagged);
            let free = DropNode::FreeAllocation(place);
            Some(match surviving {
                Some(surviving) => DropNode::Seq(vec![surviving, free]),
                None => free,
            })
        }
        TyKind::Array { .. } => Some(DropNode::Glue(place)),
        TyKind::Fun { .. } => Some(DropNode::Glue(place)),
        TyKind::Tuple(elems) => {
            let field_tys: Vec<Ty> = elems;
            seq(field_tys
                .into_iter()
                .enumerate()
                .filter_map(|(index, field_ty)| {
                    let field = projected(&place, Projection::Field(index as u32));
                    plan(tcx, state, flags, field, field_ty, flagged)
                }))
        }
        TyKind::Adt { def, args } => match tcx.enum_variant_count(def) {
            None => {
                let field_tys = tcx.struct_field_tys(def, &args);
                seq(field_tys
                    .into_iter()
                    .enumerate()
                    .filter_map(|(index, field_ty)| {
                        let field = projected(&place, Projection::Field(index as u32));
                        plan(tcx, state, flags, field, field_ty, flagged)
                    }))
            }
            Some(variant_count) => {
                let mut arms = Vec::new();
                for variant in 0..variant_count {
                    let field_count = tcx.variant_field_tys(def, &args, variant).len();
                    let fields = (0..field_count).filter_map(|index| {
                        let field_ty = variant_field_ty(tcx, ty, variant, index);
                        let mut field = projected(
                            &place,
                            Projection::Downcast(VariantIdx::from_usize(variant)),
                        );
                        field.projections.push(Projection::Field(index as u32));
                        plan(tcx, state, flags, field, field_ty, flagged)
                    });
                    if let Some(arm) = seq(fields) {
                        arms.push((VariantIdx::from_usize(variant), arm));
                    }
                }
                (!arms.is_empty()).then_some(DropNode::Variants { place, arms })
            }
        },
        other => unreachable!("no drop plan for {other:?}, which `needs_drop` says owns something"),
    }
}

fn projected(place: &Place, projection: Projection) -> Place {
    let mut projected = place.clone();
    projected.projections.push(projection);
    projected
}

fn seq(nodes: impl Iterator<Item = DropNode>) -> Option<DropNode> {
    let nodes: Vec<DropNode> = nodes.collect();
    (!nodes.is_empty()).then_some(DropNode::Seq(nodes))
}
