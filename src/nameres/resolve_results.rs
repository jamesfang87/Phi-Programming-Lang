use std::collections::HashMap;

use crate::hir::{DefId, HirId};

/// A primitive, built-in type such as `i32` or `bool` -- these never get a `DefId` of their own.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PrimTy {
    I8,
    I16,
    I32,
    I64,
    U8,
    U16,
    U32,
    U64,
    F32,
    F64,
    Bool,
    Char,
}

/// What a name or path resolved to.
#[derive(Clone, Copy, Debug)]
pub enum Res {
    /// A top-level item: a function, struct, enum, trait, or extend block.
    Def(DefId),
    /// A local binding: a function/closure parameter, or a `let`/`with` binding.
    Local(HirId),
    /// The `self` parameter of a method, addressed by the [`HirId`] of its
    /// [`Node::SelfParam`](crate::hir::Node::SelfParam).
    ///
    /// This is the value-namespace counterpart of [`Res::SelfTy`], and is kept apart from
    /// [`Res::Local`] because `self` isn't an ordinary local: it carries a
    /// [`SelfMode`](crate::ast::SelfMode) rather than a declared type, and its type is the
    /// enclosing item's `Self` -- so every consumer has to handle it specially anyway. Matching
    /// on it exhaustively is what forces that.
    SelfVal(HirId),
    /// An enum variant. Variants don't get a `DefId` of their own (see [`crate::hir::Variant`]),
    /// so they're addressed by their `HirId` inside their enum's own arena instead.
    Variant(HirId),
    /// A primitive type, such as `i32` or `bool`.
    PrimTy(PrimTy),
    /// `Self`, inside a `struct`/`enum`/`trait`/`extend` body. `adt` is the concrete type `Self`
    /// stands for (the struct/enum itself, or the type an `extend` targets); `trait_` is set
    /// when the enclosing trait is known too (inside a trait's own body, or an
    /// `extend ... with Trait`).
    SelfTy { adt: DefId, trait_: Option<DefId> },
    /// Resolution failed; a diagnostic has already been reported.
    Err,
}

/// The output of name resolution: every name- or path-carrying [`HirId`] in the program, mapped
/// to what it resolved to.
pub struct NameResolverResults {
    res: HashMap<HirId, Res>,

    /// What `Self` stands for inside each definition that introduces it: a struct, an enum, a
    /// trait, or an `extend` block. Definitions that don't introduce a `Self` of their own (a
    /// function, a closure, a module) are absent -- a reference inside one of those looks the
    /// answer up by walking its parent chain, see [`NameResolver::self_ty`].
    ///
    /// [`NameResolver::self_ty`]: crate::name_res::NameResolver::self_ty
    self_tys: HashMap<DefId, Res>,
}

impl NameResolverResults {
    pub fn new() -> Self {
        Self {
            res: HashMap::new(),
            self_tys: HashMap::new(),
        }
    }

    pub fn add(&mut self, reference: HirId, res: Res) {
        self.res.insert(reference, res);
    }

    pub fn get(&self, reference: HirId) -> Option<Res> {
        self.res.get(&reference).copied()
    }

    /// Records what `Self` means inside `def_id`'s own body.
    pub fn add_self_ty(&mut self, def_id: DefId, res: Res) {
        self.self_tys.insert(def_id, res);
    }

    /// What `Self` means inside `def_id`'s own body, if `def_id` introduces one at all.
    pub fn self_ty(&self, def_id: DefId) -> Option<Res> {
        self.self_tys.get(&def_id).copied()
    }
}

impl Default for NameResolverResults {
    fn default() -> Self {
        Self::new()
    }
}
