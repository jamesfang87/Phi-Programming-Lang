use crate::mir::ids::{Local, VariantIdx};

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
    ConstantIndex { offset: u32, from_end: bool },
    /// `Downcast` narrows an enum place to one variant's payload and is required before any
    /// `Field` projection into that payload is well-typed.
    Downcast(VariantIdx),
}
