use std::collections::HashMap;

use crate::ast::Symbol;
use crate::hir::{DefId, HirId};

/// A primitive, built-in type such as `i32` or `bool`
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
    Def(DefId),
    Local(HirId),
    Generic(HirId),
    Variant(HirId),
    PrimTy(PrimTy),
    /// `Self`, inside a `struct`/`enum`/`trait`/`extend` body. `adt` is the concrete type `Self`
    /// stands for (the struct/enum itself, or the type an `extend` targets); `trait_` is set
    /// when the enclosing trait is known too (inside a trait's own body, or an
    /// `extend ... with Trait`).
    ///
    SelfTy {
        adt: DefId,
        trait_: Option<DefId>,
    },
    /// The `self` parameter of a method, addressed by the [`HirId`] of its
    /// [`Node::SelfParam`](crate::hir::Node::SelfParam).
    ///
    /// This is the value-namespace counterpart of [`Res::SelfTy`], and is kept apart from
    /// [`Res::Local`] because `self` isn't an ordinary local: it carries a
    /// [`SelfMode`](crate::ast::SelfMode) rather than a declared type, and its type is the
    /// enclosing item's `Self` -- so every consumer has to handle it specially anyway. Matching
    /// on it exhaustively is what forces that.
    SelfVal(HirId),

    Err,
}

/// The output of name resolution: every name- or path-carrying [`HirId`] in the program, mapped
/// to what it resolved to.
pub struct NameResolutions {
    res: HashMap<HirId, Res>,

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

impl NameResolutions {
    pub fn new() -> Self {
        Self {
            res: HashMap::new(),
            self_tys: HashMap::new(),
            generics: HashMap::new(),
        }
    }

    /// Records what the name or path at `id` resolved to.
    ///
    /// A resolution that points at `id` itself -- a binding that is its own declaration, or an
    /// `extend` block's own `<T>` entry, which name resolution binds to the very node it is
    /// written on -- is stored like any other. Dropping those as redundant would make absence
    /// mean two different things, "resolved to itself" and "never resolved", leaving every
    /// consumer to tell them apart from context it does not have.
    pub fn record(&mut self, id: HirId, res: Res) {
        self.res.insert(id, res);
    }

    /// Records what `Self` means inside `def_id`'s own body.
    pub fn record_self_ty(&mut self, def_id: DefId, res: Res) {
        self.self_tys.insert(def_id, res);
    }

    /// Records the generic type parameters `def_id` declares for itself, keyed by name.
    pub fn record_generic(&mut self, def_id: DefId, params: HashMap<Symbol, Res>) {
        self.generics.insert(def_id, params);
    }

    pub fn res(&self, reference: HirId) -> Option<Res> {
        self.res.get(&reference).copied()
    }

    /// What `Self` means inside `def_id`'s own body, if `def_id` introduces one at all.
    pub fn self_ty(&self, def_id: DefId) -> Option<Res> {
        self.self_tys.get(&def_id).copied()
    }

    /// Looks `name` up among the generic type parameters `def_id` declares for itself -- not
    /// those of any enclosing definition, see [`NameResolver::generic_ty`].
    ///
    /// [`NameResolver::generic_ty`]: crate::nameres::NameResolver::generic_ty
    pub fn generic(&self, def_id: DefId, name: Symbol) -> Option<Res> {
        self.generics.get(&def_id)?.get(&name).copied()
    }

    /// Iterates every name- or path-carrying [`HirId`] recorded so far, alongside what it
    /// resolved to. Used by the `--nameres` debug dump; see
    /// [`crate::driver::emit_debug::print_nameres`].
    pub fn iter(&self) -> impl Iterator<Item = (HirId, Res)> + '_ {
        self.res.iter().map(|(&id, &res)| (id, res))
    }

    /// Iterates every definition that introduces a `Self`, alongside what `Self` means inside
    /// its own body. Used by the `--nameres` debug dump.
    pub fn iter_self_tys(&self) -> impl Iterator<Item = (DefId, Res)> + '_ {
        self.self_tys.iter().map(|(&id, &res)| (id, res))
    }

    /// Iterates every definition that declares generics of its own, alongside the name -> [`Res`]
    /// map for those generics. Used by the `--nameres` debug dump.
    pub fn iter_generics(&self) -> impl Iterator<Item = (DefId, &HashMap<Symbol, Res>)> + '_ {
        self.generics.iter().map(|(&id, params)| (id, params))
    }
}

impl Default for NameResolutions {
    fn default() -> Self {
        Self::new()
    }
}
