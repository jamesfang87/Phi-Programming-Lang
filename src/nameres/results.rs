use std::collections::HashMap;

use crate::ast::Symbol;
use crate::hir::{DefId, HirId};
use crate::langitems::LangItems;

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

/// What a name or path written in *value* position resolved to: the callee of a call, the
/// subject of a field access, a bare identifier used as a value.
///
/// Kept apart from [`TypeRes`] because the two namespaces are searched separately and can never
/// produce each other's answers. Consumers used to match one enum covering both and prove the
/// impossible half away by hand, with an `unreachable!` arm listing the four variants that could
/// not turn up. Splitting the enum hands that proof to the compiler.
#[derive(Clone, Copy, Debug)]
pub enum ValueRes {
    /// A `let` or `with` binding, a function parameter, or a pattern binding, addressed by the
    /// [`HirId`] of the node that introduces it.
    Local(HirId),
    /// The `self` parameter of a method, addressed by the [`HirId`] of its
    /// [`Node::SelfParam`](crate::hir::Node::SelfParam).
    ///
    /// Kept apart from [`ValueRes::Local`] because `self` isn't an ordinary local: it carries a
    /// [`SelfMode`](crate::ast::SelfMode) rather than a declared type, and its type is the
    /// enclosing item's `Self` -- so every consumer has to handle it specially anyway. Matching
    /// on it exhaustively is what forces that.
    SelfVal(HirId),
    /// An enum variant named through its type, such as the `circle` in `Shape.circle(1.24)`,
    /// addressed by the [`HirId`] of its [`Node::Variant`](crate::hir::Node::Variant).
    Variant(HirId),
    /// A `fun` item.
    Def(DefId),
    Err,
}

/// What a path written in *type* position resolved to: a parameter's or field's annotation, a
/// return type, a generic argument, a struct literal's path. See [`ValueRes`] for why the two
/// namespaces have separate types.
///
/// `Self` is deliberately absent. It is not something a path resolves to -- it is a property of
/// the definition the path was written inside -- so it lives in its own table, keyed by `DefId`;
/// see [`SelfTyRes`]. Keeping it out of here is what leaves this enum exactly the set of answers
/// a type path lookup can produce, with no arm for a consumer to rule out by hand.
#[derive(Clone, Copy, Debug)]
pub enum TypeRes {
    /// A built-in type such as `i32` or `bool`, which never gets a `DefId`.
    PrimTy(PrimTy),
    /// A generic type parameter, addressed by the node that declares it: a
    /// [`Node::Generic`](crate::hir::Node::Generic), or the bare
    /// [`Node::Ty`](crate::hir::Node::Ty) standing in for one in an `extend` block's own `<T>`
    /// list.
    Generic(HirId),
    /// A `struct`, `enum`, or `trait` item.
    Def(DefId),
    Err,
}

/// What `Self` stands for inside a definition that introduces one.
#[derive(Clone, Copy, Debug)]
pub enum SelfTyRes {
    /// `adt` is the concrete type `Self` stands for -- the struct or enum itself, or the type an
    /// `extend` targets. `trait_` is set when the enclosing trait is known too: inside a trait's
    /// own body, or an `extend ... with Trait`.
    Ty { adt: DefId, trait_: Option<DefId> },
    /// The definition introduces a `Self`, but what it stands for could not be worked out -- an
    /// `extend` whose target path did not resolve. Recorded rather than left absent so that a
    /// `Self` written inside the block is not reported a second time.
    Err,
}

/// The output of name resolution: every name- or path-carrying [`HirId`] in the program, mapped
/// to what it resolved to.
pub struct NameResolutions {
    /// Every name or path written in value position, mapped to what it named.
    values: HashMap<HirId, ValueRes>,

    /// Every path written in type position, mapped to what it named. Separate from
    /// [`NameResolutions::values`] rather than sharing one table with a wider enum, so that
    /// asking the wrong namespace is a type error rather than an arm to rule out; see
    /// [`ValueRes`]. No [`HirId`] appears in both, since a node is written in one position or
    /// the other.
    types: HashMap<HirId, TypeRes>,

    /// What `Self` stands for inside each definition that introduces it: a struct, an enum, a
    /// trait, or an `extend` block. Definitions that don't introduce a `Self` of their own (a
    /// function, a closure, a module) are absent -- a reference inside one of those looks the
    /// answer up by walking its parent chain, see [`NameResolver::self_ty`].
    ///
    /// [`NameResolver::self_ty`]: crate::nameres::NameResolver::self_ty
    self_tys: HashMap<DefId, SelfTyRes>,

    /// What each trait bound written on a generic type parameter named, keyed by the
    /// [`Node::Generic`](crate::hir::Node::Generic) that carries them and ordered as they were
    /// written.
    ///
    /// A bound is a bare [`Path`](crate::ast::Path) hanging off the parameter, not a node of its
    /// own, so there is no [`HirId`] to key it under in [`NameResolutions::types`] the way every
    /// other type-position path is. Resolving one and dropping the answer -- which is what this
    /// pass used to do -- means `fun f<T: Show>(..)` promises nothing to anybody downstream: the
    /// trait solver's `ParamEnv` is built out of exactly this table.
    ///
    /// A parameter with no bounds is absent rather than present-and-empty; see
    /// [`NameResolutions::bounds`].
    bounds: HashMap<HirId, Vec<TypeRes>>,

    /// The generic type parameters each definition declares for itself, keyed by name. A
    /// definition that declares no generics of its own is absent -- a reference inside one of
    /// those (or inside a definition nested in it, such as a method's body) looks the answer up
    /// by walking its parent chain, see [`NameResolver::generic_ty`].
    ///
    /// [`NameResolver::generic_ty`]: crate::nameres::NameResolver::generic_ty
    generics: HashMap<DefId, HashMap<Symbol, TypeRes>>,

    /// The core-library definitions the compiler itself knows by name -- the enums `?` and `for`
    /// desugar through, and the traits the operators dispatch to.
    ///
    /// They live here because resolving them is name resolution's job and can only be done while
    /// the symbol table exists, but every consumer of them is a later pass. Carrying them out
    /// with the rest of this pass's output is what makes them reachable at all; see
    /// [`crate::langitems`].
    lang_items: LangItems,
}

impl NameResolutions {
    pub fn new() -> Self {
        Self {
            values: HashMap::new(),
            types: HashMap::new(),
            self_tys: HashMap::new(),
            bounds: HashMap::new(),
            generics: HashMap::new(),
            lang_items: LangItems::default(),
        }
    }

    /// Records the lang items resolved against the finished module namespaces.
    pub fn record_lang_items(&mut self, lang_items: LangItems) {
        self.lang_items = lang_items;
    }

    /// The core-library definitions the compiler knows by name.
    pub fn lang_items(&self) -> &LangItems {
        &self.lang_items
    }

    /// Records what the name or path at `id` named in value position.
    ///
    /// A resolution that points at `id` itself -- a binding that is its own declaration -- is
    /// stored like any other. Dropping those as redundant would make absence mean two different
    /// things, "resolved to itself" and "never resolved", leaving every consumer to tell them
    /// apart from context it does not have.
    pub fn record_value(&mut self, id: HirId, res: ValueRes) {
        self.values.insert(id, res);
    }

    /// Records what the path at `id` named in type position. See
    /// [`NameResolutions::record_value`] on why a self-referential resolution is kept: an
    /// `extend` block's own `<T>` entry binds to the very node it is written on.
    pub fn record_type(&mut self, id: HirId, res: TypeRes) {
        self.types.insert(id, res);
    }

    /// Records what `Self` means inside `def_id`'s own body.
    pub fn record_self_ty(&mut self, def_id: DefId, res: SelfTyRes) {
        self.self_tys.insert(def_id, res);
    }

    /// Records what each trait bound written on the generic parameter `id` named, in the order
    /// they were written. Called once per parameter, even when it carries no bounds at all, so
    /// that a parameter whose bounds were resolved is distinguishable from one that was never
    /// reached.
    pub fn record_bounds(&mut self, id: HirId, bounds: Vec<TypeRes>) {
        self.bounds.insert(id, bounds);
    }

    /// Records the generic type parameters `def_id` declares for itself, keyed by name.
    pub fn record_generic(&mut self, def_id: DefId, params: HashMap<Symbol, TypeRes>) {
        self.generics.insert(def_id, params);
    }

    /// What the name or path at `id` named in value position.
    pub fn value(&self, id: HirId) -> Option<ValueRes> {
        self.values.get(&id).copied()
    }

    /// What the path at `id` named in type position.
    pub fn ty(&self, id: HirId) -> Option<TypeRes> {
        self.types.get(&id).copied()
    }

    /// What `Self` means inside `def_id`'s own body, if `def_id` introduces one at all.
    pub fn self_ty(&self, def_id: DefId) -> Option<SelfTyRes> {
        self.self_tys.get(&def_id).copied()
    }

    /// What the trait bounds written on the generic parameter `id` named, in source order.
    ///
    /// An unbounded parameter answers with an empty slice rather than `None`: "declares no
    /// bounds" and "was never resolved" are the same thing to every consumer, since both mean
    /// there is nothing to assume about the parameter.
    pub fn bounds(&self, id: HirId) -> &[TypeRes] {
        self.bounds.get(&id).map_or(&[], Vec::as_slice)
    }

    /// Looks `name` up among the generic type parameters `def_id` declares for itself -- not
    /// those of any enclosing definition, see [`NameResolver::generic_ty`].
    ///
    /// [`NameResolver::generic_ty`]: crate::nameres::NameResolver::generic_ty
    pub fn generic(&self, def_id: DefId, name: Symbol) -> Option<TypeRes> {
        self.generics.get(&def_id)?.get(&name).copied()
    }

    /// Iterates every value-position name recorded so far, alongside what it resolved to. Used
    /// by the `--nameres` debug dump; see [`crate::driver::emit_debug::print_nameres`].
    pub fn iter_values(&self) -> impl Iterator<Item = (HirId, ValueRes)> + '_ {
        self.values.iter().map(|(&id, &res)| (id, res))
    }

    /// Iterates every type-position path recorded so far, alongside what it resolved to. Used by
    /// the `--nameres` debug dump.
    pub fn iter_types(&self) -> impl Iterator<Item = (HirId, TypeRes)> + '_ {
        self.types.iter().map(|(&id, &res)| (id, res))
    }

    /// Iterates every definition that introduces a `Self`, alongside what `Self` means inside
    /// its own body. Used by the `--nameres` debug dump.
    pub fn iter_self_tys(&self) -> impl Iterator<Item = (DefId, SelfTyRes)> + '_ {
        self.self_tys.iter().map(|(&id, &res)| (id, res))
    }

    /// Iterates every definition that declares generics of its own, alongside the
    /// name -> [`TypeRes`] map for those generics. Used by the `--nameres` debug dump.
    pub fn iter_generics(&self) -> impl Iterator<Item = (DefId, &HashMap<Symbol, TypeRes>)> + '_ {
        self.generics.iter().map(|(&id, params)| (id, params))
    }
}

impl Default for NameResolutions {
    fn default() -> Self {
        Self::new()
    }
}
