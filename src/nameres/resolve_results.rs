use std::collections::HashMap;

use crate::ast::Symbol;
use crate::hir::{DefId, HirId};
use crate::langitems::LangItems;

/// A primitive, built-in type such as `i32` or `bool` -- these never get a `DefId` of their own.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
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
    /// A generic type parameter, such as `T` in `fun identity<T>(x: T) -> T`, addressed by the
    /// [`HirId`] of its [`Node::Generic`](crate::hir::Node::Generic).
    TyParam(HirId),
    /// Resolution failed; a diagnostic has already been reported.
    Err,
}

/// The output of name resolution: every name- or path-carrying [`HirId`] in the program, mapped
/// to what it resolved to.
pub struct NameResolverResults {
    res: HashMap<HirId, Res>,

    /// The core library definitions the compiler knows by name. These aren't reached through a
    /// path the user wrote, but they are resolved the same way and against the same namespaces,
    /// so they're part of the same answer to "what does this name mean" that the rest of this
    /// struct records.
    lang_items: LangItems,

    /// What `Self` stands for inside each definition that introduces it: a struct, an enum, a
    /// trait, or an `extend` block. Definitions that don't introduce a `Self` of their own (a
    /// function, a closure, a module) are absent -- a reference inside one of those looks the
    /// answer up by walking its parent chain, see [`NameResolver::self_ty`].
    ///
    /// [`NameResolver::self_ty`]: crate::name_res::NameResolver::self_ty
    self_tys: HashMap<DefId, Res>,

    /// The generic type parameters each definition declares for itself, keyed by name. A
    /// definition that declares no generics of its own is absent -- a reference inside one of
    /// those (or inside a definition nested in it, such as a method's body) looks the answer up
    /// by walking its parent chain, see [`NameResolver::generic_ty`].
    ///
    /// [`NameResolver::generic_ty`]: crate::nameres::NameResolver::generic_ty
    generics: HashMap<DefId, HashMap<Symbol, Res>>,
}

impl NameResolverResults {
    pub fn new(lang_items: LangItems) -> Self {
        Self {
            res: HashMap::new(),
            lang_items,
            self_tys: HashMap::new(),
            generics: HashMap::new(),
        }
    }

    /// The core library definitions the compiler knows by name.
    pub fn lang_items(&self) -> &LangItems {
        &self.lang_items
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

    /// Records the generic type parameters `def_id` declares for itself, keyed by name.
    pub fn add_generics(&mut self, def_id: DefId, params: HashMap<Symbol, Res>) {
        self.generics.insert(def_id, params);
    }

    /// Looks `name` up among the generic type parameters `def_id` declares for itself -- not
    /// those of any enclosing definition, see [`NameResolver::generic_ty`].
    ///
    /// [`NameResolver::generic_ty`]: crate::nameres::NameResolver::generic_ty
    pub fn generic(&self, def_id: DefId, name: Symbol) -> Option<Res> {
        self.generics.get(&def_id)?.get(&name).copied()
    }
}

impl Default for NameResolverResults {
    fn default() -> Self {
        Self::new(LangItems::default())
    }
}
