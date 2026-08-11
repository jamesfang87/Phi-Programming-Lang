//! The symbol table AST-level name resolution builds and queries.
//!
//! [`SymbolTable::new`] builds the table in three phases: [`SymbolTable::collect`] walks the
//! AST's module tree once, building one [`ModuleScope`] per module out of that module's own
//! declarations; [`SymbolTable::resolve_imports`] then resolves every `import` statement into
//! the importing module's own scope, because an import can name any module in the tree by an
//! absolute path -- resolving one needs every module's scope to already exist, not just the
//! importing module's; and [`SymbolTable::find_prelude`] locates `core::prelude` last, since the
//! prelude's own scope is filled in by the imports it declares.

use std::collections::HashMap;
use std::collections::hash_map::Entry;

use crate::ast::interner::Interner;
use crate::ast::{Ast, Ident, Import, Item, ItemKind, NodeId, Path, Symbol};
use crate::nameres::diagnostics::{
    report_ambiguous_import, report_conflict, report_dyn_not_trait, report_not_found,
    report_self_unavailable,
};
use crate::nameres::res::PrimTy;
use crate::nameres::res::{Local, Res, TyDef, Type};

/// The module unqualified lookups fall back to once the enclosing module chain is exhausted.
/// It re-exports the core library's most-used items, so a program can name `Option` or `Add`
/// without importing anything.
const PRELUDE_PATH: [&str; 2] = ["core", "prelude"];

pub struct SymbolTable<'ast> {
    local_scopes: Vec<HashMap<Symbol, Local>>,
    generic_scopes: Vec<HashMap<Symbol, Type>>,
    self_scopes: Vec<Option<TyDef>>,

    modules: HashMap<NodeId, ModuleScope>,
    items: HashMap<NodeId, &'ast Item>,

    by_path: HashMap<Box<[Symbol]>, NodeId>,

    prelude: Option<NodeId>,
    ast: &'ast Ast,
}

/// One module's declared items, split by namespace.
///
/// `types` holds `TyDef`, not `Type`: `Prim` and `Generic` can never live in a module's
/// namespace, and the narrower type says so.
struct ModuleScope {
    functions: HashMap<Symbol, NodeId>,
    types: HashMap<Symbol, TyDef>,
    mods: HashMap<Symbol, NodeId>,
}

impl ModuleScope {
    fn new() -> Self {
        Self {
            functions: HashMap::new(),
            types: HashMap::new(),
            mods: HashMap::new(),
        }
    }

    /// Inserts `name` into the function namespace, reporting a conflict (and keeping the
    /// earlier binding) if `name` is already declared there.
    fn insert_function(&mut self, name: Ident, id: NodeId) {
        match self.functions.entry(name.text) {
            Entry::Occupied(_) => report_conflict(name),
            Entry::Vacant(e) => {
                e.insert(id);
            }
        }
    }

    /// Inserts `name` into the type namespace, reporting a conflict if already declared.
    fn insert_type(&mut self, name: Ident, def: TyDef) {
        match self.types.entry(name.text) {
            Entry::Occupied(_) => report_conflict(name),
            Entry::Vacant(e) => {
                e.insert(def);
            }
        }
    }

    /// Inserts `name` into the module namespace, reporting a conflict if already declared.
    fn insert_mod(&mut self, name: Ident, id: NodeId) {
        match self.mods.entry(name.text) {
            Entry::Occupied(_) => report_conflict(name),
            Entry::Vacant(e) => {
                e.insert(id);
            }
        }
    }
}

/// The primitive named by `name`, if any. Type lookup consults this first, which is sound
/// because `insert_*` rejects any declaration that would shadow one.
pub fn prim_ty(name: Symbol) -> Option<PrimTy> {
    Some(match Interner::resolve(name) {
        "i8" => PrimTy::I8,
        "i16" => PrimTy::I16,
        "i32" => PrimTy::I32,
        "i64" => PrimTy::I64,
        "u8" => PrimTy::U8,
        "u16" => PrimTy::U16,
        "u32" => PrimTy::U32,
        "u64" => PrimTy::U64,
        "f32" => PrimTy::F32,
        "f64" => PrimTy::F64,
        "bool" => PrimTy::Bool,
        "char" => PrimTy::Char,
        _ => return None,
    })
}

impl<'ast> SymbolTable<'ast> {
    /// Builds the full symbol table: every module's own declarations, then every `import`
    /// resolved into the scope of the module that wrote it, then the prelude.
    ///
    /// The three phases run in that order and only in that order. [`Self::collect`] must finish
    /// for the whole tree before [`Self::resolve_imports`] starts, because an import can name
    /// any module in the tree by an absolute path -- including one `collect` has not yet
    /// visited if it ran interleaved. And [`Self::find_prelude`] must run last because the
    /// prelude's own scope is filled in by the imports it declares, so it is not usable as a
    /// fallback until those have resolved.
    pub fn new(ast: &'ast Ast) -> Self {
        let mut table = Self::collect(ast);
        table.resolve_imports();
        table.prelude = table.find_prelude();
        table
    }

    /// The prelude module, if this unit has a core library. `None` if it does not -- which
    /// should not happen in a real build, but leaves the resolver working (minus the prelude)
    /// rather than panicking if it is ever driven without one.
    ///
    /// No caller outside this module's own tests yet -- part of the public API the design spec
    /// lists, kept for a future consumer.
    #[allow(dead_code)]
    pub fn prelude(&self) -> Option<NodeId> {
        self.prelude
    }

    /// Walks [`PRELUDE_PATH`] down from the root to find the prelude module.
    ///
    /// This must run after imports, because the prelude's own namespace *is* the set of imports
    /// it declares.
    fn find_prelude(&self) -> Option<NodeId> {
        let mut current = self.ast.root_id();
        for segment in PRELUDE_PATH {
            current = self.lookup_mod(current, Interner::intern(segment))?;
        }
        Some(current)
    }

    /// Walks `ast`'s module tree, building one [`ModuleScope`] per module out of that module's
    /// own declared items and submodules.
    ///
    /// This is a partial constructor: it does not resolve imports or find the prelude, so
    /// looking a name up right after `collect` only finds what a module declared directly.
    /// [`Self::new`] layers imports and the prelude on top.
    pub fn collect(ast: &'ast Ast) -> Self {
        let mut table = Self {
            local_scopes: Vec::new(),
            generic_scopes: Vec::new(),
            self_scopes: Vec::new(),
            modules: HashMap::new(),
            items: HashMap::new(),
            by_path: HashMap::new(),
            prelude: None,
            ast,
        };
        table.collect_module(ast.root_id());
        table
    }

    /// Builds `module_id`'s [`ModuleScope`] from its own items, records its canonical path in
    /// [`Self::by_path`], and recurses into each of its submodules.
    fn collect_module(&mut self, module_id: NodeId) {
        let module = self.ast.module(module_id);

        let path: Box<[Symbol]> = module.path.segments.iter().map(|seg| seg.text).collect();
        self.by_path.insert(path, module_id);

        let mut scope = ModuleScope::new();
        for item in &module.items {
            self.items.insert(item.id, item);
            match &item.kind {
                ItemKind::Function(f) => scope.insert_function(f.name, item.id),
                ItemKind::Struct(s) => scope.insert_type(s.name, TyDef::Struct(item.id)),
                ItemKind::Enum(e) => scope.insert_type(e.name, TyDef::Enum(item.id)),
                ItemKind::Trait(t) => scope.insert_type(t.name, TyDef::Trait(item.id)),
                // `extend` blocks are unnamed, so neither namespace can hold them.
                ItemKind::Extend(_) => {}
                ItemKind::ModuleDecl(_) | ItemKind::Import(_) | ItemKind::Error => {}
            }
        }

        // Submodules come from `Module::children`, not `items`. A module's own item list
        // keeps only its direct declarations, not children.
        let children = module.children.clone();
        for &child_id in &children {
            let child = self.ast.module(child_id);
            let name = *child
                .path
                .segments
                .last()
                .expect("a module's path always has at least one segment");
            scope.insert_mod(name, child_id);
        }

        self.modules.insert(module_id, scope);

        for &child_id in &children {
            self.collect_module(child_id);
        }
    }

    /// Resolves every `import` statement in the tree, inserting each one into the importing
    /// module's scope. After this runs, imported names are looked up exactly like names the
    /// module declared itself, with no separate "imports" concept in the resolver.
    ///
    /// This must run after [`Self::collect`] finishes for every module. An import can name any
    /// module by its absolute path (see [`Self::resolve_import`]), including one `collect`
    /// reaches after the importing module. [`Self::new`] enforces this ordering. Using
    /// [`Ast::mod_ids`] instead of recursing from the root avoids re-deriving tree shape: every
    /// module is already in [`Self::modules`], so the flat iteration suffices.
    fn resolve_imports(&mut self) {
        for module_id in self.ast.mod_ids() {
            let module = self.ast.module(module_id);
            for import in &module.imports {
                self.resolve_import(module_id, import);
            }
        }
    }

    /// Resolves one `import` statement declared inside `importing_module` and binds it into that
    /// module's own scope.
    ///
    /// Every import path is absolute -- resolved from the root module down, by its own path
    /// segments -- regardless of where the `import` statement itself appears, which is why the
    /// lookups below start from [`Ast::root_id`] rather than `importing_module`.
    fn resolve_import(&mut self, importing_module: NodeId, import: &Import) {
        let root = self.ast.root_id();

        if import.glob {
            let Some(source) = self.resolve_import_mod_path(root, &import.path) else {
                report_not_found(
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

        let type_res = self.resolve_import_type_path(root, &import.path);
        let val_res = self.resolve_import_value_path(root, &import.path);
        let mod_res = self.resolve_import_mod_path(root, &import.path);

        let name = import.alias.unwrap_or(
            *import
                .path
                .segments
                .last()
                .expect("a path always has at least one segment"),
        );

        match (type_res, val_res, mod_res) {
            (Some(def), None, None) => self
                .modules
                .get_mut(&importing_module)
                .unwrap()
                .insert_type(name, def),
            (None, Some(id), None) => self
                .modules
                .get_mut(&importing_module)
                .unwrap()
                .insert_function(name, id),
            (None, None, Some(id)) => self
                .modules
                .get_mut(&importing_module)
                .unwrap()
                .insert_mod(name, id),
            (None, None, None) => report_not_found(name),
            _ => report_ambiguous_import(name),
        }
    }

    /// Copies every name `source` declares (functions, types, and submodules) into `into`'s
    /// scope for a glob import (`import math::*;`).
    ///
    /// Each name goes through the same [`ModuleScope::insert_function`]/`insert_type`/
    /// `insert_mod` conflict check as any ordinary declaration. A collision is reported like
    /// any other redefinition, whether with a direct declaration or an earlier import. Glob
    /// imports have no special carve-out: their names enter the module's scope and collide
    /// just as hand-written duplicates would.
    fn import_glob(&mut self, into: NodeId, source: NodeId, import: &Import) {
        let (functions, types, mods) = {
            let source = self
                .modules
                .get(&source)
                .expect("every module in the tree has a scope by the time imports resolve");
            (
                source.functions.clone(),
                source.types.clone(),
                source.mods.clone(),
            )
        };

        // A glob import doesn't name each item itself, so there's no per-name span to blame a
        // conflict on -- the import statement's own span is the closest thing to one.
        let dest = self.modules.get_mut(&into).unwrap();
        for (text, id) in functions {
            dest.insert_function(
                Ident {
                    text,
                    span: import.span,
                },
                id,
            );
        }
        for (text, def) in types {
            dest.insert_type(
                Ident {
                    text,
                    span: import.span,
                },
                def,
            );
        }
        for (text, id) in mods {
            dest.insert_mod(
                Ident {
                    text,
                    span: import.span,
                },
                id,
            );
        }
    }

    /// Resolves an absolute path (all import paths are absolute; see [`Self::resolve_import`])
    /// against the function namespace: steps through every segment except the last as a
    /// submodule starting from `base` (the root), then looks up the final segment among that
    /// module's function declarations.
    ///
    /// This performs a single walk from `base` without consulting the local scope chain or
    /// prelude fallback, unlike [`Self::lookup_value_path`] which a written path in source code
    /// traverses. Import path resolution never needs these fallbacks: the path is already
    /// absolute by construction, and the goal is to populate the importing module's namespace,
    /// not to consult scope chains. Additionally, consulting the prelude would be incorrect
    /// because the prelude is not populated until all imports have resolved.
    fn resolve_import_value_path(&self, base: NodeId, path: &Path) -> Option<NodeId> {
        let (name, modules) = path.segments.split_last()?;
        let module = self.walk_modules(base, modules)?;
        self.lookup_function(module, name.text)
    }

    /// Resolves an absolute path against the type namespace. Like [`Self::resolve_import_value_path`]
    /// but for types.
    fn resolve_import_type_path(&self, base: NodeId, path: &Path) -> Option<TyDef> {
        let (name, modules) = path.segments.split_last()?;
        let module = self.walk_modules(base, modules)?;
        self.lookup_type_name(module, name.text)
    }

    /// Resolves an absolute path against the module namespace. Like [`Self::resolve_import_value_path`]
    /// but for modules.
    fn resolve_import_mod_path(&self, base: NodeId, path: &Path) -> Option<NodeId> {
        let (name, modules) = path.segments.split_last()?;
        let module = self.walk_modules(base, modules)?;
        self.lookup_mod(module, name.text)
    }

    /// The modules a path written inside `from` resolves against, innermost first: `from`, then
    /// each ancestor in turn, then the root.
    ///
    /// Resolving against the enclosing module first is what makes a reference to a sibling item
    /// work unqualified. Falling through the ancestors to the root is what keeps a
    /// fully-qualified path (`math::vector::dot`) resolving from anywhere.
    fn module_chain(&self, from: NodeId) -> Vec<NodeId> {
        let mut chain = Vec::new();
        let mut current = Some(from);
        while let Some(module) = current {
            chain.push(module);
            current = self.ast.parent(module);
        }
        chain
    }

    /// Runs `lookup` against each module in `from`'s chain, yielding the first hit, and falls
    /// back to the prelude if none of them has one.
    ///
    /// The prelude comes last, after the root, so it can only ever supply a name nothing else
    /// in scope already does: an item the user declares shadows the core library's of the same
    /// name rather than colliding with it. That is exactly the opposite of a glob import, whose
    /// names enter the module's own scope and therefore do collide.
    fn in_module_chain<T>(&self, from: NodeId, lookup: impl Fn(NodeId) -> Option<T>) -> Option<T> {
        self.module_chain(from)
            .into_iter()
            .chain(self.prelude)
            .find_map(lookup)
    }

    /// Resolves a written path in *value* position: `local_scopes` (innermost first, and only
    /// for a single-segment path -- a local can never be named by a qualified path), then each
    /// module in `from`'s chain's function namespace, then the prelude.
    ///
    /// `from` is the enclosing module's `NodeId`, which the AST traversal already tracks --
    /// unlike the HIR-side resolver, nothing here needs to walk up from a non-module node to
    /// find one.
    pub fn lookup_value_path(&self, from: NodeId, path: &Path) -> Option<Res> {
        let (last, prefix) = path.segments.split_last()?;

        if prefix.is_empty()
            && let Some(local) = self.lookup_local(last.text)
        {
            return Some(Res::Local(local));
        }

        self.in_module_chain(from, |base| {
            let module = self.walk_modules(base, prefix)?;
            self.lookup_function(module, last.text).map(Res::Function)
        })
    }

    /// Resolves a written path in *type* position: `Prim`, then `generic_scopes` (innermost
    /// first), then `Self`, then the module chain, then the prelude -- but the first three only
    /// for a single-segment path. A multi-segment path such as `math::T` can never name a
    /// primitive or a generic parameter, so it skips straight to the module walk.
    ///
    /// Primitives are checked first: a primitive name can never shadow anything in a namespace
    /// because it never lexes as `TokenKind::Identifier` (each has its own token kind, e.g.
    /// `TokenKind::I32`). No source text both parses as a declaration and names one. Checking
    /// primitives first is cheaper than making every `i32` walk the module chain to fail.
    pub fn lookup_type_path(&self, from: NodeId, path: &Path) -> Option<Type> {
        let (last, prefix) = path.segments.split_last()?;

        if prefix.is_empty() {
            if let Some(prim) = prim_ty(last.text) {
                return Some(Type::Prim(prim));
            }
            if let Some(generic) = self.lookup_generic(last.text) {
                return Some(generic);
            }
            if last.text == Interner::intern("Self") {
                return self.current_self().map(Type::Def);
            }
        }

        self.in_module_chain(from, |base| {
            let module = self.walk_modules(base, prefix)?;
            self.lookup_type_name(module, last.text).map(Type::Def)
        })
    }

    /// Resolves a written path in *module* position: the plain module chain walk, with no
    /// local/generic/prim/`Self` special-casing -- a module can never be shadowed by any of
    /// those.
    ///
    /// No caller and no test anywhere yet: nothing in the AST currently writes a path in module
    /// position (an `import`'s path resolves through [`Self::resolve_import_mod_path`] instead,
    /// which needs no chain or prelude fallback). Part of the public API the design spec lists,
    /// kept for a future consumer -- flag this if it stays uncalled much longer.
    #[allow(dead_code)]
    pub fn lookup_mod_path(&self, from: NodeId, path: &Path) -> Option<NodeId> {
        let (last, prefix) = path.segments.split_last()?;
        self.in_module_chain(from, |base| {
            let module = self.walk_modules(base, prefix)?;
            self.lookup_mod(module, last.text)
        })
    }

    /// Resolves a type-position path, reporting and returning [`Res::Err`] on failure rather
    /// than `None`.
    ///
    /// This, not [`Self::lookup_type_path`], is the entry point the AST traversal calls: a
    /// failed resolution has to be *recorded*, so that absence from `NameResolutions` keeps
    /// meaning "never reached" rather than "resolved, unsuccessfully" (see the module doc on
    /// [`Res::Err`]).
    ///
    /// A bare path resolving to a trait is **legal** here (it means static dispatch; the
    /// function monomorphizes over the concrete type, like Rust's `impl Trait`). `dyn` is the
    /// dynamic-dispatch form (a distinct node kind, handled by [`Self::lookup_dyn_path`]). The
    /// two are told apart by which method the caller reaches, not by inspecting the `Res`.
    ///
    /// `Self` is special-cased here rather than left to [`Self::lookup_type_path`]'s own `Self`
    /// handling, so that writing `Self` with an empty `self_scopes` stack gets its own
    /// diagnostic ("`Self` is not available here") instead of the generic "cannot find `Self`
    /// in this scope".
    ///
    /// An empty stack and a stack topped with [`Self::push_self_unresolved`]'s `None` are told
    /// apart here. The first (empty) stack reports "not available" because nothing else can
    /// blame the error. The second (topped with `None`) sits inside a definition (an `extend`
    /// block) whose target already failed and was reported, so reporting `Self` would duplicate
    /// the error for the same root cause.
    pub fn resolve_type_path(&self, from: NodeId, path: &Path) -> Res {
        let last = *path
            .segments
            .last()
            .expect("a path always has at least one segment");

        if path.segments.len() == 1 && last.text == Interner::intern("Self") {
            return match self.self_scopes.last() {
                Some(Some(def)) => Res::Type(Type::Def(*def)),
                Some(None) => Res::Err,
                None => {
                    report_self_unavailable(last.span);
                    Res::Err
                }
            };
        }

        match self.lookup_type_path(from, path) {
            Some(ty) => Res::Type(ty),
            None => {
                report_not_found(last);
                Res::Err
            }
        }
    }

    /// Resolves a `TyKind::Dyn`'s path, which **must** name a trait.
    ///
    /// Anything else -- a struct, an enum, a generic, a primitive, or an unresolved name -- is
    /// an error, recorded as [`Res::Err`] so the diagnostic fires exactly once here rather than
    /// cascading into typeck as a mismatched-type error with no clear origin.
    ///
    /// `dyn Self` falls into the "not a trait" arm via [`Self::lookup_type_path`]'s own `Self`
    /// handling: `Self` never names a trait in a position where `dyn` is legal, so this is the
    /// correct outcome rather than a case that needs its own arm.
    pub fn lookup_dyn_path(&self, from: NodeId, path: &Path) -> Res {
        let last = *path
            .segments
            .last()
            .expect("a path always has at least one segment");

        match self.lookup_type_path(from, path) {
            Some(Type::Def(TyDef::Trait(id))) => Res::Type(Type::Def(TyDef::Trait(id))),
            Some(_) => {
                report_dyn_not_trait(path.span);
                Res::Err
            }
            None => {
                report_not_found(last);
                Res::Err
            }
        }
    }

    /// Looks `name` up among `enum_`'s variants.
    ///
    /// No way exists to search for a variant by name alone. The enum comes from the expected
    /// type, and typeck calls this once it knows it. Scanning every enum in scope would
    /// duplicate the ambiguity that the leading `.` on a variant reference exists to avoid.
    ///
    /// No caller outside this module's own tests yet -- typeck doesn't consult the AST-level
    /// table for this today. Part of the public API the design spec lists, kept for a future
    /// consumer.
    #[allow(dead_code)]
    pub fn lookup_variant(&self, enum_: NodeId, name: Symbol) -> Option<NodeId> {
        let item = self.item(enum_)?;
        let ItemKind::Enum(e) = &item.kind else {
            return None;
        };
        e.variants
            .iter()
            .find(|v| v.name.text == name)
            .map(|v| v.id)
    }

    /// The `Item` declared with `id`, if any. Backs [`Self::lookup_variant`], and is what a
    /// later pass reaches an item's contents through once a path has resolved to its `NodeId`.
    /// Dead along with [`Self::lookup_variant`], its only caller.
    #[allow(dead_code)]
    fn item(&self, id: NodeId) -> Option<&'ast Item> {
        self.items.get(&id).copied()
    }

    /// Steps through each of `segments` as a submodule name, starting from `base`.
    fn walk_modules(&self, base: NodeId, segments: &[Ident]) -> Option<NodeId> {
        let mut current = base;
        for segment in segments {
            current = self.lookup_mod(current, segment.text)?;
        }
        Some(current)
    }

    /// Looks `name` up among `module`'s function items.
    pub fn lookup_function(&self, module: NodeId, name: Symbol) -> Option<NodeId> {
        self.modules.get(&module)?.functions.get(&name).copied()
    }

    /// Looks `name` up among `module`'s struct/enum/trait items. Submodules live in their own
    /// namespace -- see [`Self::lookup_mod`].
    pub fn lookup_type_name(&self, module: NodeId, name: Symbol) -> Option<TyDef> {
        self.modules.get(&module)?.types.get(&name).copied()
    }

    /// Looks `name` up among `module`'s submodules.
    pub fn lookup_mod(&self, module: NodeId, name: Symbol) -> Option<NodeId> {
        self.modules.get(&module)?.mods.get(&name).copied()
    }

    /// The module named by `segments`, a canonical, fully-qualified path (e.g.
    /// `["math", "vector"]`), if any module in the tree has that path.
    ///
    /// Widely exercised by this module's own tests to reach a module by name, but no
    /// non-test caller yet.
    #[allow(dead_code)]
    pub fn module_by_path(&self, segments: &[Symbol]) -> Option<NodeId> {
        self.by_path.get(segments).copied()
    }

    /// Opens a new local scope, e.g. on entering a block or a match arm.
    pub fn push_scope(&mut self) {
        self.local_scopes.push(HashMap::new());
    }

    /// Closes the innermost local scope, discarding every binding it holds.
    pub fn pop_scope(&mut self) {
        self.local_scopes.pop();
    }

    /// Binds `name` in the innermost scope.
    ///
    /// Takes an `Ident`, not a `Path`: a local is always one segment; accepting `Path` would
    /// imply `let a::b = ...` is possible. Returns `()`, not `Result`, because shadowing is
    /// legal. The innermost map is overwritten, with no failure to report.
    pub fn insert_local(&mut self, name: Ident, local: Local) {
        self.local_scopes
            .last_mut()
            .expect("insert_local requires an open scope")
            .insert(name.text, local);
    }

    /// Looks `name` up in every open local scope, innermost first.
    pub fn lookup_local(&self, name: Symbol) -> Option<Local> {
        self.local_scopes
            .iter()
            .rev()
            .find_map(|s| s.get(&name).copied())
    }

    /// Opens a new generic scope, e.g. on entering a definition declaring `<...>`.
    pub fn push_generics(&mut self, params: HashMap<Symbol, Type>) {
        self.generic_scopes.push(params);
    }

    /// Closes the innermost generic scope.
    pub fn pop_generics(&mut self) {
        self.generic_scopes.pop();
    }

    /// Looks `name` up in every open generic scope, innermost first. A method seeing its
    /// `extend` block's `<T>` is just the outer scope still being on the stack.
    pub fn lookup_generic(&self, name: Symbol) -> Option<Type> {
        self.generic_scopes
            .iter()
            .rev()
            .find_map(|s| s.get(&name).copied())
    }

    /// Opens a `Self` scope, e.g. on entering a struct, enum, trait, or `extend` block.
    pub fn push_self(&mut self, ty: TyDef) {
        self.self_scopes.push(Some(ty));
    }

    /// Opens a `Self` scope for a definition whose own target failed to resolve -- an `extend`
    /// block whose `adt_path` didn't find anything. Keeps the stack balanced with the matching
    /// `pop_self` the definition's `visit_*` still calls, while making a `Self` written inside
    /// report nothing: [`Self::resolve_type_path`] sees a non-empty stack (so it doesn't emit its
    /// own "not available" diagnostic) whose top is empty (so it can't resolve to a definition
    /// either), and returns `Res::Err` silently. Without this, `Self` inside the block would
    /// report a second, redundant diagnostic on top of whatever already explained why `adt_path`
    /// failed.
    pub fn push_self_unresolved(&mut self) {
        self.self_scopes.push(None);
    }

    /// Closes the innermost `Self` scope.
    pub fn pop_self(&mut self) {
        self.self_scopes.pop();
    }

    /// What `Self` stands for here: the innermost enclosing struct, enum, trait, or `extend`
    /// target. Neither a function nor a closure pushes a scope of its own, which is what lets a
    /// method body and a closure inside it both see the enclosing definition's `Self`. `None`
    /// when the innermost scope is open but unresolved ([`Self::push_self_unresolved`]) as well
    /// as when no scope is open at all -- callers that need to tell those two apart (only
    /// [`Self::resolve_type_path`] does) read `self_scopes` directly instead.
    pub fn current_self(&self) -> Option<TyDef> {
        self.self_scopes.last().copied().flatten()
    }
}
