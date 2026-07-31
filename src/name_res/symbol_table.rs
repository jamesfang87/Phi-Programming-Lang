use std::collections::HashMap;

use crate::ast::interner::Interner;
use crate::ast::{Ident, Path, Symbol};
use crate::diag::{DiagCtx, Diagnostic};
use crate::hir::{DefId, Hir, HirId, Import, Node, OwnerNode};
use crate::name_res::resolve_results::Res;
use std::collections::hash_map::Entry;

struct Scope {
    scope: HashMap<Symbol, Res>,
}

impl Scope {
    fn new() -> Self {
        Self {
            scope: HashMap::new(),
        }
    }
}

/// One module's declared items, split into its value namespace (functions) and its type
/// namespace (structs, enums, traits), keyed by name.
struct ModuleNamespace {
    values: HashMap<Symbol, DefId>,
    types: HashMap<Symbol, DefId>,
    mods: HashMap<Symbol, DefId>,
}

impl ModuleNamespace {
    /// Inserts `name` into the value namespace, reporting a conflict (and keeping the earlier
    /// binding) if `name` is already declared there.
    fn insert_value(&mut self, name: Ident, def_id: DefId) {
        match self.values.entry(name.text) {
            Entry::Occupied(_) => SymbolTable::report_conflict(name),
            Entry::Vacant(e) => {
                e.insert(def_id);
            }
        }
    }

    /// Same as [`Self::insert_value`], but for the type namespace.
    fn insert_type(&mut self, name: Ident, def_id: DefId) {
        match self.types.entry(name.text) {
            Entry::Occupied(_) => SymbolTable::report_conflict(name),
            Entry::Vacant(e) => {
                e.insert(def_id);
            }
        }
    }

    fn insert_mod(&mut self, name: Ident, def_id: DefId) {
        match self.mods.entry(name.text) {
            Entry::Occupied(_) => SymbolTable::report_conflict(name),
            Entry::Vacant(e) => {
                e.insert(def_id);
            }
        }
    }
}

pub struct SymbolTable<'hir> {
    scopes: Vec<Scope>,
    modules: HashMap<DefId, ModuleNamespace>,
    hir: &'hir Hir,
}

impl<'hir> SymbolTable<'hir> {
    pub fn new(hir: &'hir Hir) -> Self {
        let mut modules = HashMap::new();
        Self::collect_module(hir, hir.root_id(), &mut modules);
        let mut table = Self {
            scopes: Vec::new(),
            modules,
            hir,
        };
        table.resolve_imports(hir.root_id());
        table
    }

    fn collect_module(
        hir: &'hir Hir,
        module_id: DefId,
        modules: &mut HashMap<DefId, ModuleNamespace>,
    ) {
        let OwnerNode::Module(module) = hir.owner(module_id) else {
            unreachable!("{module_id:?} does not name a module");
        };

        let mut namespace = ModuleNamespace {
            values: HashMap::new(),
            types: HashMap::new(),
            mods: HashMap::new(),
        };
        for &item in &module.items {
            match hir.owner(item) {
                OwnerNode::Function(f) => {
                    namespace.insert_value(f.name, item);
                }
                OwnerNode::Struct(s) => {
                    namespace.insert_type(s.name, item);
                }
                OwnerNode::Enum(e) => {
                    namespace.insert_type(e.name, item);
                }
                OwnerNode::Trait(t) => {
                    namespace.insert_type(t.name, item);
                }
                OwnerNode::Module(child) => {
                    let name = *child
                        .path
                        .segments
                        .last()
                        .expect("a module's path always has at least one segment");
                    namespace.insert_mod(name, item);
                    Self::collect_module(hir, item, modules);
                }
                // `extend` blocks and closures aren't named, so neither namespace can hold them.
                OwnerNode::Extend(_) | OwnerNode::Closure(_) => {}
            }
        }
        modules.insert(module_id, namespace);
    }

    /// Resolves every `import` statement in `module_id` and its descendants, inserting each one
    /// into the *importing* module's own namespace -- so that after this runs, an imported name
    /// is looked up exactly like a name the module declared itself, with no separate "imports"
    /// concept anywhere else in the resolver.
    ///
    /// This must run after [`Self::collect_module`] has built every module's namespace, not just
    /// `module_id`'s: an import can name any module in the tree by its absolute path (see
    /// [`Self::resolve_import`]), including one this pass hasn't recursed into yet.
    /// [`Self::new`] enforces that ordering by calling this only once `collect_module` has
    /// finished for the whole tree.
    fn resolve_imports(&mut self, module_id: DefId) {
        let arena = self.hir.arena(module_id);
        let OwnerNode::Module(module) = arena.owner() else {
            unreachable!("{module_id:?} does not name a module");
        };

        for &import_id in &module.imports {
            let Node::Import(import) = arena.get(import_id) else {
                unreachable!("Node that is not an import found in a module's import list");
            };
            self.resolve_import(module_id, import);
        }

        for &item in &module.items {
            if let OwnerNode::Module(_) = self.hir.owner(item) {
                self.resolve_imports(item);
            }
        }
    }

    /// Resolves one `import` statement declared inside `importing_module` and binds it there.
    ///
    /// Every import path is absolute -- resolved from the root module down, per its own path
    /// segments -- regardless of where the `import` statement itself appears, which is why the
    /// lookups below start from [`Hir::root_id`] rather than `importing_module`.
    fn resolve_import(&mut self, importing_module: DefId, import: &Import) {
        let root = self.hir.root_id();

        if import.glob {
            let Some(source) = self.lookup_mod_path(root, &import.path) else {
                Self::report_not_found(
                    *import
                        .path
                        .segments
                        .last()
                        .expect("a path always has at least one segment"),
                );
                return;
            };
            self.import_glob(importing_module, source, import);
            return;
        }

        let type_res = self.lookup_type_path(root, &import.path);
        let val_res = self.lookup_value_path(root, &import.path);
        let mod_res = self.lookup_mod_path(root, &import.path);

        let name = import.alias.unwrap_or(
            *import
                .path
                .segments
                .last()
                .expect("a path always has at least one segment"),
        );

        match (type_res, val_res, mod_res) {
            (Some(def_id), None, None) => self
                .modules
                .get_mut(&importing_module)
                .unwrap()
                .insert_type(name, def_id),
            (None, Some(def_id), None) => self
                .modules
                .get_mut(&importing_module)
                .unwrap()
                .insert_value(name, def_id),
            (None, None, Some(def_id)) => self
                .modules
                .get_mut(&importing_module)
                .unwrap()
                .insert_mod(name, def_id),
            (None, None, None) => Self::report_not_found(name),
            _ => Self::report_ambiguous_import(name),
        }
    }

    /// Copies every name `source` declares -- values, types, and submodules alike -- into
    /// `into`'s own namespace, for a glob import (`import math::*;`).
    ///
    /// Each name goes through the same [`ModuleNamespace::insert_value`]/`insert_type`/
    /// `insert_mod` conflict check as an ordinary declaration, so a name the glob brings in that
    /// collides with something `into` already has -- declared directly, or brought in by an
    /// earlier import -- is reported exactly like any other redefinition. There's no separate
    /// "imports don't conflict with declarations" carve-out: the point of the glob is to behave
    /// as if its contents had been spelled out by hand, and a hand-written duplicate would
    /// conflict too.
    fn import_glob(&mut self, into: DefId, source: DefId, import: &Import) {
        let (values, types, mods) = {
            let source = self
                .modules
                .get(&source)
                .expect("every module in the tree has a namespace by the time imports resolve");
            (
                source.values.clone(),
                source.types.clone(),
                source.mods.clone(),
            )
        };

        // A glob import doesn't name each item itself, so there's no per-name span to blame a
        // conflict on -- the import statement's own span is the closest thing to one.
        let dest = self.modules.get_mut(&into).unwrap();
        for (text, def_id) in values {
            dest.insert_value(
                Ident {
                    text,
                    span: import.span,
                },
                def_id,
            );
        }
        for (text, def_id) in types {
            dest.insert_type(
                Ident {
                    text,
                    span: import.span,
                },
                def_id,
            );
        }
        for (text, def_id) in mods {
            dest.insert_mod(
                Ident {
                    text,
                    span: import.span,
                },
                def_id,
            );
        }
    }

    pub fn push_scope(&mut self) {
        self.scopes.push(Scope::new());
    }

    pub fn pop_scope(&mut self) {
        self.scopes.pop();
    }

    /// Binds `name` to `res` in the innermost scope, opening one first if none is open yet (e.g.
    /// for module-level bindings like imports, resolved before any block scope exists).
    pub fn bind(&mut self, name: Ident, res: Res) {
        if self.scopes.is_empty() {
            self.push_scope();
        }
        self.scopes.last_mut().unwrap().scope.insert(name.text, res);
    }

    /// Looks `name` up in every open scope, innermost first.
    pub fn lookup(&self, name: Symbol) -> Option<Res> {
        self.scopes
            .iter()
            .rev()
            .find_map(|s| s.scope.get(&name).copied())
    }

    /// Looks `name` up among `module`'s function items.
    pub fn lookup_value(&self, module: DefId, name: Symbol) -> Option<DefId> {
        self.modules.get(&module)?.values.get(&name).copied()
    }

    /// Looks `name` up among `module`'s struct/enum/trait items. Submodules live in their own
    /// namespace -- see [`Self::lookup_mod`].
    pub fn lookup_type(&self, module: DefId, name: Symbol) -> Option<DefId> {
        self.modules.get(&module)?.types.get(&name).copied()
    }

    /// Resolves a (possibly multi-segment) path against the value namespace: every segment but
    /// the last is stepped through as a submodule, and the final segment is looked up among that
    /// module's functions. `from` is the definition the path is written in -- see
    /// [`Self::module_chain`] for which modules that makes the path relative to.
    pub fn lookup_value_path(&self, from: DefId, path: &Path) -> Option<DefId> {
        let (name, modules) = path.segments.split_last()?;
        self.in_module_chain(from, |base| {
            let module = self.walk_modules(base, modules)?;
            self.lookup_value(module, name.text)
        })
    }

    /// Same as [`Self::lookup_value_path`], but against the type namespace.
    pub fn lookup_type_path(&self, from: DefId, path: &Path) -> Option<DefId> {
        let (name, modules) = path.segments.split_last()?;
        self.in_module_chain(from, |base| {
            let module = self.walk_modules(base, modules)?;
            self.lookup_type(module, name.text)
        })
    }

    pub fn lookup_mod_path(&self, from: DefId, path: &Path) -> Option<DefId> {
        let (name, modules) = path.segments.split_last()?;
        self.in_module_chain(from, |base| {
            let module = self.walk_modules(base, modules)?;
            self.lookup_mod(module, name.text)
        })
    }

    /// The modules a path written inside `from` is resolved against, innermost first: the module
    /// `from` is declared in, then each of its ancestors, ending at the root.
    ///
    /// Resolving relative to the enclosing module first is what makes a reference to a sibling
    /// item work without qualification; falling back through the ancestors, and finally the
    /// root, is what keeps a fully-qualified path (`math::vector::dot`) resolving from anywhere.
    fn module_chain(&self, from: DefId) -> Vec<DefId> {
        let mut chain = Vec::new();
        let mut current = Some(self.hir.module_of(from));
        while let Some(module) = current {
            chain.push(module);
            // A module's parent is always the module that encloses it.
            current = self.hir.parent(module);
        }
        chain
    }

    /// Runs `lookup` against each module in `from`'s chain in turn, yielding the first hit.
    fn in_module_chain<T>(&self, from: DefId, lookup: impl Fn(DefId) -> Option<T>) -> Option<T> {
        self.module_chain(from).into_iter().find_map(lookup)
    }

    /// Steps through each of `segments` as a submodule name, starting from `base`.
    fn walk_modules(&self, base: DefId, segments: &[Ident]) -> Option<DefId> {
        let mut current = base;
        for segment in segments {
            current = self.step_into_module(current, segment.text)?;
        }
        Some(current)
    }

    /// Looks `name` up among `module`'s submodules and steps into it, failing if `name` doesn't
    /// name one.
    fn step_into_module(&self, module: DefId, name: Symbol) -> Option<DefId> {
        let next = self.lookup_mod(module, name)?;
        debug_assert!(matches!(self.hir.owner(next), OwnerNode::Module(_)));
        Some(next)
    }

    /// Looks `name` up among `module`'s submodules.
    pub fn lookup_mod(&self, module: DefId, name: Symbol) -> Option<DefId> {
        self.modules.get(&module)?.mods.get(&name).copied()
    }

    /// Looks `name` up among `enum_def`'s variants.
    ///
    /// A `.variant` names no enum of its own, so there is deliberately no way to search for a
    /// variant by name alone: the enum comes from the expected type, and typeck calls this once
    /// it knows it. Scanning every enum in scope for a matching variant name -- which is what
    /// bare, undotted variant names would require -- is exactly the ambiguity the leading `.`
    /// exists to avoid.
    pub fn lookup_variant(&self, enum_def: DefId, name: Symbol) -> Option<Res> {
        let OwnerNode::Enum(enum_) = self.hir.owner(enum_def) else {
            return None;
        };

        for &variant_id in &enum_.variants {
            let hir_id = HirId {
                owner: enum_def,
                local_id: variant_id,
            };
            let Node::Variant(variant) = self.hir.node(hir_id) else {
                unreachable!("an enum's variant list only names variant nodes");
            };
            if variant.name.text == name {
                return Some(Res::Variant(hir_id));
            }
        }
        None
    }

    pub fn report_not_found(name: Ident) {
        DiagCtx::emit(
            Diagnostic::error(
                format!(
                    "cannot find `{}` in this scope",
                    Interner::resolve(name.text)
                ),
                name.span,
            )
            .with_label("not found in this scope"),
        );
    }

    pub fn report_conflict(name: Ident) {
        DiagCtx::emit(
            Diagnostic::error(
                format!(
                    "the name `{}` is defined multiple times",
                    Interner::resolve(name.text)
                ),
                name.span,
            )
            .with_label("redefined here")
            .with_help("a name with the same spelling is already in scope"),
        );
    }

    /// Reports an import whose path matches more than one namespace at once -- e.g. a value and
    /// a type both named the same thing -- so there's no single answer for what the imported
    /// name should mean.
    pub fn report_ambiguous_import(name: Ident) {
        DiagCtx::emit(
            Diagnostic::error(
                format!(
                    "ambiguous import: `{}` refers to more than one item",
                    Interner::resolve(name.text)
                ),
                name.span,
            )
            .with_label("ambiguous import")
            .with_help(
                "this path matches more than one declaration; use a more specific path to disambiguate",
            ),
        );
    }
}
