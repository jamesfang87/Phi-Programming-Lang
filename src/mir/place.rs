use crate::mir::body::LocalDecl;
use crate::mir::ids::{Local, VariantIdx};
use crate::typeck::ty::ctx::TyCtx;
use crate::typeck::ty::{Ty, TyKind};

#[derive(Clone, PartialEq, Eq, Hash, Debug)]
pub struct Place {
    pub local: Local,
    pub projections: Vec<Projection>,
}

impl Place {
    pub fn from_local(local: Local) -> Self {
        Place {
            local,
            projections: Vec::new(),
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum Projection {
    Deref,

    Field(u32),

    Index(Local),

    ConstantIndex(u32),

    Downcast(VariantIdx),
}

pub fn place_ty(tcx: &mut TyCtx, local_decls: &[LocalDecl], place: &Place) -> Ty {
    let mut ty = local_decls[place.local.index()].ty;
    let mut variant: Option<usize> = None;
    for projection in &place.projections {
        let (next, next_variant) = match (projection, tcx.kind(ty).clone()) {
            (Projection::Deref, TyKind::Ref { base, .. } | TyKind::Iso(base)) => (base, None),
            (Projection::Field(index), TyKind::Tuple(elems)) => (elems[*index as usize], None),
            (Projection::Field(index), TyKind::Adt { def, args }) => {
                let field = match variant {
                    Some(variant) => tcx.variant_field_tys(def, &args, variant)[*index as usize],
                    None => tcx.struct_field_tys(def, &args)[*index as usize],
                };
                (field, None)
            }
            (Projection::Index(_) | Projection::ConstantIndex(_), TyKind::Array { elem, .. }) => {
                (elem, None)
            }
            (Projection::Downcast(downcast), _) => (ty, Some(downcast.index())),
            (projection, kind) => {
                panic!("place_ty: {projection:?} does not apply to a value of type {kind:?}")
            }
        };
        ty = next;
        variant = next_variant;
    }
    ty
}
