#[derive(Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Debug)]
pub struct DefId(u32);

impl DefId {
    pub(crate) fn from_usize(index: usize) -> Self {
        DefId(index as u32)
    }

    pub fn index(self) -> usize {
        self.0 as usize
    }

    pub fn owner_id(self) -> HirId {
        HirId::new(self, LocalId::OWNER)
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Debug)]
pub struct LocalId(u32);

impl LocalId {
    /// Every arena's own owner node (a `Function`, a `Struct`, a `Module`, and so on) is stored
    /// at this id, index zero, within that arena.
    pub const OWNER: LocalId = LocalId(0);

    pub(crate) fn from_usize(index: usize) -> Self {
        LocalId(index as u32)
    }

    pub fn index(self) -> usize {
        self.0 as usize
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub struct HirId {
    pub owner: DefId,
    pub local_id: LocalId,
}

impl HirId {
    pub fn new(owner: DefId, local_id: LocalId) -> Self {
        HirId { owner, local_id }
    }
}

macro_rules! typed_id {
    ($name:ident) => {
        #[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
        pub struct $name(HirId);

        #[allow(dead_code)]
        impl $name {
            pub fn hir_id(self) -> HirId {
                self.0
            }

            pub fn owner(self) -> DefId {
                self.0.owner
            }
        }

        impl From<HirId> for $name {
            fn from(id: HirId) -> Self {
                Self(id)
            }
        }

        impl From<$name> for HirId {
            fn from(id: $name) -> Self {
                id.0
            }
        }
    };
}

typed_id!(ExprId);
typed_id!(TyId);
typed_id!(PatId);
typed_id!(BlockId);
typed_id!(StmtId);
typed_id!(ArmId);
