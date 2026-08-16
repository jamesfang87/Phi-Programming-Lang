use std::collections::hash_map::Entry;
use std::collections::HashMap;

use crate::ast::interner::Interner;
use crate::ast::{Ast, Ident, Import, Item, ItemKind, NodeId, Path, Symbol, Visibility};
use crate::diagnostics::nameres::{
    report_ambiguous_import, report_conflict, report_dyn_not_trait, report_not_found,
    report_private_item,
};
use crate::nameres::res::PrimTy;
use crate::nameres::res::{Local, Res, TyDef, Type};

const PRELUDE_PATH: [&str; 2] = ["core", "prelude"];

pub struct SymbolTable<'ast> {
    local_scopes: Vec<HashMap<Symbol, Local>>,
    generic_scopes: Vec<HashMap<Symbol, Type>>,
    self_scopes: Vec<Option<TyDef>>,

    modules: HashMap<NodeId, ModuleScope>,
    items: HashMap<NodeId, &'ast Item>,

    prelude: Option<NodeId>,
    ast: &'ast Ast,
}

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
    pub fn new(ast: &'ast Ast) -> Self {
        let mut table = Self::collect(ast);
        table.resolve_imports();
        table.prelude = table.find_prelude();
        table
    }

    //-------------------------------------------------------------------------

    #[allow(dead_code)]
    pub fn prelude(&self) -> Option<NodeId> {
        self.prelude
    }

    fn find_prelude(&self) -> Option<NodeId> {
        let mut current = self.ast.root_id();
        for segment in PRELUDE_PATH {
            current = self.lookup_mod(current, Interner::intern(segment))?;
        }
        Some(current)
    }

    //-------------------------------------------------------------------------

    pub fn collect(ast: &'ast Ast) -> Self {
        let mut table = Self {
            local_scopes: Vec::new(),
            generic_scopes: Vec::new(),
            self_scopes: Vec::new(),
            modules: HashMap::new(),
            items: HashMap::new(),
            prelude: None,
            ast,
        };
        table.collect_module(ast.root_id());
        table
    }

    fn collect_module(&mut self, module_id: NodeId) {
        let module = self.ast.module(module_id);

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

    //-------------------------------------------------------------------------

    fn resolve_imports(&mut self) {
        for module_id in self.ast.mod_ids() {
            let module = self.ast.module(module_id);
            for import in &module.imports {
                self.resolve_import(module_id, import);
            }
        }
    }

    fn resolve_import(&mut self, importing_module: NodeId, import: &Import) {
        // note that ALL imports start from the root, not the importing module
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

        let name = import.alias.unwrap_or(
            *import
                .path
                .segments
                .last()
                .expect("a path always has at least one segment"),
        );

        let mut private_hit = false;
        let type_res =
            self.resolve_import_type_path(root, &import.path)
                .and_then(|(module, def)| {
                    if self.is_visible(importing_module, module, self.visibility(def.node_id())) {
                        Some(def)
                    } else {
                        private_hit = true;
                        None
                    }
                });
        let val_res =
            self.resolve_import_value_path(root, &import.path)
                .and_then(|(module, id)| {
                    if self.is_visible(importing_module, module, self.visibility(id)) {
                        Some(id)
                    } else {
                        private_hit = true;
                        None
                    }
                });
        let mod_res = self.resolve_import_mod_path(root, &import.path);

        if type_res.is_none() && val_res.is_none() && mod_res.is_none() && private_hit {
            report_private_item(name);
            return;
        }

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

        // TODO: Modify AST to allow to get from NodeId
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

    /// Resolves an import's value-namespace target, alongside the module its scope was found
    /// in -- `resolve_import` needs that module to decide whether the importing module is
    /// allowed to see it at all.
    fn resolve_import_value_path(&self, base: NodeId, path: &Path) -> Option<(NodeId, NodeId)> {
        let (name, modules) = path.segments.split_last()?;
        let module = self.walk_modules(base, modules)?;
        self.lookup_function(module, name.text)
            .map(|id| (module, id))
    }

    /// [`Self::resolve_import_value_path`], for the type namespace.
    fn resolve_import_type_path(&self, base: NodeId, path: &Path) -> Option<(NodeId, TyDef)> {
        let (name, modules) = path.segments.split_last()?;
        let module = self.walk_modules(base, modules)?;
        self.lookup_type(module, name.text).map(|def| (module, def))
    }

    fn resolve_import_mod_path(&self, base: NodeId, path: &Path) -> Option<NodeId> {
        let (name, modules) = path.segments.split_last()?;
        let module = self.walk_modules(base, modules)?;
        self.lookup_mod(module, name.text)
    }

    //-------------------------------------------------------------------------

    /// Returns the module chain startin from `from` up to the root module
    fn module_chain(&self, from: NodeId) -> Vec<NodeId> {
        let mut chain = Vec::new();
        let mut current = Some(from);
        while let Some(module) = current {
            chain.push(module);
            current = self.ast.parent(module);
        }
        chain
    }

    /// Searches through the module chain starting from `from` up to the root
    /// module using the lookup predicate `lookup`
    fn in_module_chain<T>(&self, from: NodeId, lookup: impl Fn(NodeId) -> Option<T>) -> Option<T> {
        self.module_chain(from)
            .into_iter()
            .chain(self.prelude)
            .find_map(lookup)
    }

    //-------------------------------------------------------------------------

    pub fn lookup_value_path(&self, from: NodeId, path: &Path) -> Option<Res> {
        let (last, prefix) = path.segments.split_last()?;

        if prefix.is_empty()
            && let Some(local) = self.lookup_local(last.text)
        {
            return Some(Res::Local(local));
        }

        self.in_module_chain(from, |base| {
            let module = self.walk_modules(base, prefix)?;
            let id = self.lookup_function(module, last.text)?;
            self.is_visible(from, module, self.visibility(id))
                .then_some(Res::Function(id))
        })
    }

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
            let def = self.lookup_type(module, last.text)?;
            self.is_visible(from, module, self.visibility(def.node_id()))
                .then_some(Type::Def(def))
        })
    }

    /// We special case this since the type for a `dyn T` must be a Trait
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

    pub fn lookup_mod_path(&self, from: NodeId, path: &Path) -> Option<NodeId> {
        let (last, prefix) = path.segments.split_last()?;
        self.in_module_chain(from, |base| {
            let module = self.walk_modules(base, prefix)?;
            self.lookup_mod(module, last.text)
        })
    }

    //-------------------------------------------------------------------------

    pub fn lookup_function(&self, module: NodeId, name: Symbol) -> Option<NodeId> {
        self.modules.get(&module)?.functions.get(&name).copied()
    }

    pub fn lookup_type(&self, module: NodeId, name: Symbol) -> Option<TyDef> {
        self.modules.get(&module)?.types.get(&name).copied()
    }

    pub fn lookup_mod(&self, module: NodeId, name: Symbol) -> Option<NodeId> {
        self.modules.get(&module)?.mods.get(&name).copied()
    }

    //-------------------------------------------------------------------------

    fn item(&self, id: NodeId) -> Option<&'ast Item> {
        self.items.get(&id).copied()
    }

    /// The `public`/`private` declared on `id`'s own item -- the flag every item-carrying
    /// `ItemKind` already stores, not something re-derived from where it lives in the tree.
    fn visibility(&self, id: NodeId) -> Visibility {
        match self.item(id).map(|item| &item.kind) {
            Some(ItemKind::Function(f)) => f.visibility,
            Some(ItemKind::Struct(s)) => s.visibility,
            Some(ItemKind::Enum(e)) => e.visibility,
            Some(ItemKind::Trait(t)) => t.visibility,
            // `extend` blocks are unnamed and modules carry no visibility of their own; neither
            // is ever looked up through this path.
            _ => Visibility::Public,
        }
    }

    /// Whether an item declared `visibility` in `owner` -- the module whose scope it was just
    /// found in -- is reachable from `from`. `public` is visible everywhere a path can name it;
    /// `private` (the default) only reaches the declaring module and its own descendants, so
    /// `owner` must appear in `from`'s chain of ancestors (or be `from` itself).
    fn is_visible(&self, from: NodeId, owner: NodeId, visibility: Visibility) -> bool {
        match visibility {
            Visibility::Public => true,
            Visibility::Private => self.module_chain(from).contains(&owner),
        }
    }

    fn walk_modules(&self, base: NodeId, segments: &[Ident]) -> Option<NodeId> {
        let mut current = base;
        for segment in segments {
            current = self.lookup_mod(current, segment.text)?;
        }
        Some(current)
    }

    //-------------------------------------------------------------------------

    pub fn push_scope(&mut self) {
        self.local_scopes.push(HashMap::new());
    }

    pub fn pop_scope(&mut self) {
        self.local_scopes.pop();
    }

    pub fn insert_local(&mut self, name: Ident, local: Local) {
        self.local_scopes
            .last_mut()
            .expect("insert_local requires an open scope")
            .insert(name.text, local);
    }

    pub fn lookup_local(&self, name: Symbol) -> Option<Local> {
        self.local_scopes
            .iter()
            .rev()
            .find_map(|s| s.get(&name).copied())
    }

    //-------------------------------------------------------------------------

    pub fn push_generics(&mut self, params: HashMap<Symbol, Type>) {
        self.generic_scopes.push(params);
    }

    pub fn pop_generics(&mut self) {
        self.generic_scopes.pop();
    }

    pub fn lookup_generic(&self, name: Symbol) -> Option<Type> {
        self.generic_scopes
            .iter()
            .rev()
            .find_map(|s| s.get(&name).copied())
    }

    //-------------------------------------------------------------------------

    pub fn push_self(&mut self, ty: TyDef) {
        self.self_scopes.push(Some(ty));
    }

    /// This is used for cases where due to a program error, a Self does not exist.
    /// For example, an `extend`  block whose `adt_path` didn't find anything.
    pub fn push_self_unresolved(&mut self) {
        self.self_scopes.push(None);
    }

    pub fn pop_self(&mut self) {
        self.self_scopes.pop();
    }

    /// Collapses "no enclosing `Self`" and "the enclosing definition's target already failed
    /// to resolve" into one `None`, since most callers just want "is there a resolved `Self`
    /// or not." [`Self::current_self_entry`] keeps the two apart, for the one caller that needs
    /// to.
    pub fn current_self(&self) -> Option<TyDef> {
        self.current_self_entry().flatten()
    }

    /// The raw top of the `Self` scope stack, without [`Self::current_self`]'s collapsing.
    /// Tells an empty stack (`None`, `Self` is not available at all) apart from a stack topped
    /// by [`Self::push_self_unresolved`] (`Some(None)`, `Self` sits inside a definition whose
    /// target already failed) from one resolved to a type (`Some(Some(def))`).
    pub fn current_self_entry(&self) -> Option<Option<TyDef>> {
        self.self_scopes.last().copied()
    }
}
