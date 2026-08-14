//! This module defines [`Place`], everything a MIR statement or `Rvalue` can address as a source
//! or a destination, and [`PlaceElem`], the projections that reach into a place's parts.

use crate::mir::ids::{Local, VariantIdx};

/// `Place` is a location in memory: a [`Local`], plus zero or more projections that reach into
/// it. A bare local by itself only reaches a whole value. `projection` is what reaches a field, a
/// dereferenced pointee, an array element, or a narrowed enum payload instead.
/// `Place { local, projection: vec![] }` is the whole-value case, addressing `local` directly.
#[derive(Clone, PartialEq, Eq, Hash, Debug)]
pub struct Place {
    pub local: Local,
    pub projection: Vec<PlaceElem>,
}

impl Place {
    /// This is the whole-value place naming `local` directly, with no projection.
    pub fn from_local(local: Local) -> Self {
        Place {
            local,
            projection: Vec::new(),
        }
    }
}

/// `PlaceElem` is one step of a [`Place`]'s projection, applied left to right onto whatever the
/// previous step, or the bare local for the first step, addresses.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum PlaceElem {
    /// `Deref` represents `*p`, following a projection.
    Deref,
    /// `Field` addresses the `n`th field of a struct, tuple, tuple-variant payload, or
    /// record-variant payload.
    Field(u32),
    /// `Index` represents `a[i]`, where `i` is itself a local holding the index.
    Index(Local),
    /// `ConstantIndex` represents `a[N]` for a compile-time-constant `N`. Lowering uses it for
    /// the desugared per-element accesses that a fixed-size array's own lowering introduces,
    /// rather than for a user-written `a[i]`.
    ConstantIndex { offset: u32, from_end: bool },
    /// `Downcast` narrows an enum place to one variant's payload, and is required before any
    /// `Field` projection into that payload is well-formed. Match lowering produces it once a
    /// `SwitchInt` on the place's discriminant has confirmed which variant the place holds.
    Downcast(VariantIdx),
}
