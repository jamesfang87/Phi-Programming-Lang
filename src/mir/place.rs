use crate::mir::body::LocalDecl;
use crate::mir::ids::{Local, VariantIdx};
use crate::typeck::ty::{Ty, TyKind};
use crate::typeck::tyctx::TyCtx;

/// A `Place` is a location in memory corresponding to a [`Local`]. Since the
/// memory of a `Local` can be split into subparts (such as the fields of a struct),
/// `projections` further defines the exact location in the `local`'s memory.
/// These projections are applied from left to right.
#[derive(Clone, PartialEq, Eq, Hash, Debug)]
pub struct Place {
    pub local: Local,
    pub projections: Vec<Projection>,
}

impl Place {
    /// This is the whole-value place naming `local` directly, with no projection.
    pub fn from_local(local: Local) -> Self {
        Place {
            local,
            projections: Vec::new(),
        }
    }
}

/// `Projection` represents one step of a [`Place`]'s projection.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum Projection {
    /// `Deref` represents `*p`
    Deref,
    /// `Field` addresses the `n`th field of a struct, tuple, tuple-variant payload, or
    /// record-variant payload.
    Field(u32),
    /// `Index` represents `a[i]`, where `i` is itself a local holding the index.
    Index(Local),
    /// `ConstantIndex` represents `a[N]` where N is a compile-time-constant
    ConstantIndex(u32),
    /// `Downcast` narrows an enum place to one variant's payload and is required before any
    /// `Field` projection into that payload is well-typed.
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
