use std::collections::hash_map::Entry;
use std::collections::{HashMap, HashSet};

use crate::ast::{Ast, Ident, Import, Item, ItemKind, NodeId, Path, Symbol, Visibility};
use crate::diagnostics::nameres::{
    report_ambiguous_import, report_conflict, report_dyn_not_trait, report_not_found,
    report_private_item, report_self_unavailable,
};
use crate::driver::source::SrcSpan;
use crate::nameres::res::PrimTy;
use crate::nameres::res::{Local, Res, TyDef, Type};
use crate::session::Session;
use crate::spelling;

const PRELUDE_PATH: [&str; 2] = ["core", "prelude"];

/// At most this many nearby names are offered when a lookup fails.
const MAX_SUGGESTIONS: usize = 3;

/// Every primitive type, paired with its spelling.
const PRIMITIVE_TYPES: [(&str, PrimTy); 14] = [
    ("i8", PrimTy::I8),
    ("i16", PrimTy::I16),
    ("i32", PrimTy::I32),
    ("i64", PrimTy::I64),
    ("u8", PrimTy::U8),
    ("u16", PrimTy::U16),
    ("u32", PrimTy::U32),
    ("u64", PrimTy::U64),
    ("usize", PrimTy::Usize),
    ("f32", PrimTy::F32),
    ("f64", PrimTy::F64),
    ("bool", PrimTy::Bool),
    ("char", PrimTy::Char),
    ("str", PrimTy::Str),
];

enum SelfScope {
    Defined(Type),
    Unresolved,
}

#[derive(Clone, Copy, PartialEq, Eq, Hash)]
enum Namespace {
    Value,
    Type,
    Module,
}

pub struct SymbolTable<'ast> {
    session: &'ast Session,
    local_scopes: Vec<HashMap<Symbol, Local>>,
    generic_scopes: Vec<HashMap<Symbol, Type>>,
    self_scopes: Vec<SelfScope>,
    module_scopes: HashMap<NodeId, ModuleScope>,

    item_map: HashMap<NodeId, &'ast Item>,
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

    fn insert_function(&mut self, session: &Session, name: Ident, id: NodeId) {
        match self.functions.entry(name.text) {
            Entry::Occupied(_) => report_conflict(session, name),
            Entry::Vacant(e) => {
                e.insert(id);
            }
        }
    }

    fn insert_type(&mut self, session: &Session, name: Ident, def: TyDef) {
        match self.types.entry(name.text) {
            Entry::Occupied(_) => report_conflict(session, name),
            Entry::Vacant(e) => {
                e.insert(def);
            }
        }
    }

    fn insert_mod(&mut self, session: &Session, name: Ident, id: NodeId) {
        match self.mods.entry(name.text) {
            Entry::Occupied(_) => report_conflict(session, name),
            Entry::Vacant(e) => {
                e.insert(id);
            }
        }
    }
}

pub fn is_prim_ty(session: &Session, name: Symbol) -> Option<PrimTy> {
    PRIMITIVE_TYPES
        .iter()
        .find(|(spelling, _)| *spelling == session.resolve(name))
        .map(|(_, ty)| *ty)
}

impl<'ast> SymbolTable<'ast> {
    pub fn new(session: &'ast Session, ast: &'ast Ast) -> Self {
        let mut table = Self::collect(session, ast);
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
            current = self.lookup_mod(current, self.session.intern(segment))?;
        }
        Some(current)
    }

    //-------------------------------------------------------------------------

    pub fn collect(session: &'ast Session, ast: &'ast Ast) -> Self {
        let mut table = Self {
            session,
            local_scopes: Vec::new(),
            generic_scopes: Vec::new(),
            self_scopes: Vec::new(),
            module_scopes: HashMap::new(),
            item_map: HashMap::new(),
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
            self.item_map.insert(item.id, item);
            match &item.kind {
                ItemKind::Function(f) => scope.insert_function(self.session, f.name, item.id),
                ItemKind::Struct(s) => {
                    scope.insert_type(self.session, s.name, TyDef::Struct(item.id))
                }
                ItemKind::Enum(e) => scope.insert_type(self.session, e.name, TyDef::Enum(item.id)),
                ItemKind::Trait(t) => {
                    scope.insert_type(self.session, t.name, TyDef::Trait(item.id))
                }
                // `extend` blocks are unnamed, so neither namespace can hold them.
                ItemKind::Extend(_) => {}
                ItemKind::Error => {}
            }
        }

        // Recall that submodules come from `Module::children`, not `items`
        let children = module.children.clone();
        for &child_id in &children {
            let child = self.ast.module(child_id);
            let name = *child
                .path
                .segments
                .last()
                .expect("a module's path always has at least one segment");
            scope.insert_mod(self.session, name, child_id);
        }

        self.module_scopes.insert(module_id, scope);

        for &child_id in &children {
            self.collect_module(child_id);
        }
    }

    //-------------------------------------------------------------------------

    fn resolve_imports(&mut self) {
        let mut named = Vec::new();
        let mut globs = Vec::new();
        for module_id in self.ast.mod_ids() {
            for import in &self.ast.module(module_id).imports {
                if import.glob {
                    globs.push((module_id, import.clone()));
                } else {
                    named.push((module_id, import.clone()));
                }
            }
        }

        for (module_id, import) in &named {
            self.resolve_import(*module_id, import);
        }

        let root = self.ast.root_id();
        let globs: Vec<(NodeId, Import, Option<NodeId>)> = globs
            .into_iter()
            .map(|(module_id, import)| {
                let source = self.resolve_import_mod_path(root, &import.path);
                if source.is_none() {
                    let suggestions = self.suggest_import_names(&import.path);
                    report_not_found(
                        self.session,
                        *import
                            .path
                            .segments
                            .last()
                            .expect("a path always has at least one segment"),
                        &suggestions,
                    );
                }
                (module_id, import, source)
            })
            .collect();

        let mut reported = HashSet::new();
        loop {
            let mut changed = false;
            for (into, import, source) in &globs {
                if let Some(source) = source {
                    changed |= self.import_glob(*into, *source, import, &mut reported);
                }
            }
            if !changed {
                break;
            }
        }
    }

    fn resolve_import(&mut self, importing_module: NodeId, import: &Import) {
        // TODO: imports are always module-private (`Import` carries no `Visibility`, and the
        // parser accepts no `public import`), so re-exports are impossible. Real crates need
        // `pub import` to re-export items and build a public API facade.
        let root = self.ast.root_id();

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
                    if self.is_visible_from(
                        importing_module,
                        module,
                        self.visibility(def.node_id()),
                    ) {
                        Some(def)
                    } else {
                        private_hit = true;
                        None
                    }
                });
        let val_res =
            self.resolve_import_value_path(root, &import.path)
                .and_then(|(module, id)| {
                    if self.is_visible_from(importing_module, module, self.visibility(id)) {
                        Some(id)
                    } else {
                        private_hit = true;
                        None
                    }
                });
        let mod_res = self.resolve_import_mod_path(root, &import.path);

        if type_res.is_none() && val_res.is_none() && mod_res.is_none() && private_hit {
            report_private_item(self.session, name);
            return;
        }

        match (type_res, val_res, mod_res) {
            (Some(def), None, None) => self
                .module_scopes
                .get_mut(&importing_module)
                .unwrap()
                .insert_type(self.session, name, def),
            (None, Some(id), None) => self
                .module_scopes
                .get_mut(&importing_module)
                .unwrap()
                .insert_function(self.session, name, id),
            (None, None, Some(id)) => self
                .module_scopes
                .get_mut(&importing_module)
                .unwrap()
                .insert_mod(self.session, name, id),
            (None, None, None) => {
                let suggestions = self.suggest_import_names(&import.path);
                report_not_found(self.session, name, &suggestions);
            }
            _ => report_ambiguous_import(self.session, name),
        }
    }

    fn import_glob(
        &mut self,
        into: NodeId,
        source: NodeId,
        import: &Import,
        reported: &mut HashSet<(NodeId, Symbol, Namespace)>,
    ) -> bool {
        let (functions, types, mods) = {
            let source_scope = self
                .module_scopes
                .get(&source)
                .expect("every module in the tree has a scope by the time imports resolve");
            (
                source_scope.functions.clone(),
                source_scope.types.clone(),
                source_scope.mods.clone(),
            )
        };

        let functions: Vec<_> = functions
            .into_iter()
            .filter(|(_, id)| self.is_visible_from(into, source, self.visibility(*id)))
            .collect();
        let types: Vec<_> = types
            .into_iter()
            .filter(|(_, def)| self.is_visible_from(into, source, self.visibility(def.node_id())))
            .collect();

        let mut changed = false;
        let ident = |text| Ident {
            text,
            span: import.span,
        };

        for (text, id) in functions {
            match self.lookup_function(into, text) {
                Some(existing) if existing == id => {}
                Some(_) => {
                    if reported.insert((into, text, Namespace::Value)) {
                        report_conflict(self.session, ident(text));
                    }
                }
                None => {
                    self.module_scopes.get_mut(&into).unwrap().insert_function(
                        self.session,
                        ident(text),
                        id,
                    );
                    changed = true;
                }
            }
        }

        for (text, def) in types {
            match self.lookup_type(into, text) {
                Some(existing) if existing == def => {}
                Some(_) => {
                    if reported.insert((into, text, Namespace::Type)) {
                        report_conflict(self.session, ident(text));
                    }
                }
                None => {
                    self.module_scopes.get_mut(&into).unwrap().insert_type(
                        self.session,
                        ident(text),
                        def,
                    );
                    changed = true;
                }
            }
        }

        for (text, id) in mods {
            match self.lookup_mod(into, text) {
                Some(existing) if existing == id => {}
                Some(_) => {
                    if reported.insert((into, text, Namespace::Module)) {
                        report_conflict(self.session, ident(text));
                    }
                }
                None => {
                    self.module_scopes.get_mut(&into).unwrap().insert_mod(
                        self.session,
                        ident(text),
                        id,
                    );
                    changed = true;
                }
            }
        }

        changed
    }

    fn resolve_import_value_path(&self, base: NodeId, path: &Path) -> Option<(NodeId, NodeId)> {
        let (name, modules) = path.segments.split_last()?;
        let module = self.walk_modules(base, modules)?;
        self.lookup_function(module, name.text)
            .map(|id| (module, id))
    }

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

    // TODO: the behavior of these are slightly diff
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
            self.is_visible_from(from, module, self.visibility(id))
                .then_some(Res::Function(id))
        })
    }

    /// Looks up [`path`] in the type namespace
    /// Returns Res::Type if it is found, None otherwise
    pub fn probe_type_path(&self, from: NodeId, path: &Path) -> Option<Type> {
        let (last, prefix) = path.segments.split_last()?;

        if prefix.is_empty() {
            if let Some(prim) = is_prim_ty(self.session, last.text) {
                return Some(Type::Prim(prim));
            }
            if let Some(generic) = self.lookup_generic(last.text) {
                return Some(generic);
            }
        }

        self.in_module_chain(from, |base| {
            let module = self.walk_modules(base, prefix)?;
            let def = self.lookup_type(module, last.text)?;
            self.is_visible_from(from, module, self.visibility(def.node_id()))
                .then_some(Type::Def(def))
        })
    }

    /// Looks up [`path`] in the type namespace
    /// Returns Res::Type if it is found. Otherwise, a diagnostics is reported
    pub fn lookup_type_path(&self, from: NodeId, path: &Path) -> Res {
        let last = *path
            .segments
            .last()
            .expect("a path always has at least one segment");

        match self.probe_type_path(from, path) {
            Some(ty) => Res::Type(ty),
            None => {
                let suggestions = self.suggest_type_names(from, path);
                report_not_found(self.session, last, &suggestions);
                Res::Err
            }
        }
    }

    pub fn lookup_self_res(&self, span: SrcSpan) -> Res {
        match self.self_scopes.last() {
            Some(SelfScope::Defined(ty)) => Res::SelfTy(*ty),
            Some(SelfScope::Unresolved) => Res::Err,
            None => {
                report_self_unavailable(self.session, span);
                Res::Err
            }
        }
    }

    /// We special case this since the type for a `dyn T` must be a Trait
    pub fn lookup_dyn_path(&self, from: NodeId, path: &Path) -> Res {
        match self.lookup_type_path(from, path) {
            Res::Type(ty @ Type::Def(TyDef::Trait(_))) => Res::Type(ty),
            Res::Err => Res::Err,
            _ => {
                report_dyn_not_trait(self.session, path.span());
                Res::Err
            }
        }
    }

    //-------------------------------------------------------------------------

    /// Returns spellings close to the last segment of `path`, for a value lookup that failed from
    /// `from`.
    pub fn suggest_value_names(&self, from: NodeId, path: &Path) -> Vec<String> {
        let Some(written) = self.last_segment_text(path) else {
            return Vec::new();
        };
        let candidates = self.value_candidate_names(from, path);
        spelling::nearest_names(
            written,
            candidates.iter().map(String::as_str),
            MAX_SUGGESTIONS,
        )
    }

    /// Returns spellings close to the last segment of `path`, for a type lookup that failed from
    /// `from`.
    pub fn suggest_type_names(&self, from: NodeId, path: &Path) -> Vec<String> {
        let Some(written) = self.last_segment_text(path) else {
            return Vec::new();
        };
        let candidates = self.type_candidate_names(from, path);
        spelling::nearest_names(
            written,
            candidates.iter().map(String::as_str),
            MAX_SUGGESTIONS,
        )
    }

    /// Returns spellings close to the last segment of `path`, for an import that named nothing in
    /// any namespace. Imports resolve from the crate root.
    pub fn suggest_import_names(&self, path: &Path) -> Vec<String> {
        let Some(written) = self.last_segment_text(path) else {
            return Vec::new();
        };
        let candidates = self.import_candidate_names(path);
        spelling::nearest_names(
            written,
            candidates.iter().map(String::as_str),
            MAX_SUGGESTIONS,
        )
    }

    //-------------------------------------------------------------------------

    pub fn lookup_function(&self, module: NodeId, name: Symbol) -> Option<NodeId> {
        self.module_scopes
            .get(&module)?
            .functions
            .get(&name)
            .copied()
    }

    pub fn lookup_type(&self, module: NodeId, name: Symbol) -> Option<TyDef> {
        self.module_scopes.get(&module)?.types.get(&name).copied()
    }

    pub fn lookup_mod(&self, module: NodeId, name: Symbol) -> Option<NodeId> {
        self.module_scopes.get(&module)?.mods.get(&name).copied()
    }

    //-------------------------------------------------------------------------

    fn item(&self, id: NodeId) -> Option<&'ast Item> {
        self.item_map.get(&id).copied()
    }

    fn visibility(&self, id: NodeId) -> Visibility {
        match self.item(id).map(|item| &item.kind) {
            Some(ItemKind::Function(f)) => f.visibility,
            Some(ItemKind::Struct(s)) => s.visibility,
            Some(ItemKind::Enum(e)) => e.visibility,
            Some(ItemKind::Trait(t)) => t.visibility,
            _ => Visibility::Public,
        }
    }

    fn is_visible_from(&self, from: NodeId, owner: NodeId, visibility: Visibility) -> bool {
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

    /// Returns the names a failed value lookup of `path` from `from` could have found.
    fn value_candidate_names(&self, from: NodeId, path: &Path) -> Vec<String> {
        let Some((_, prefix)) = path.segments.split_last() else {
            return Vec::new();
        };
        if !prefix.is_empty() {
            return self.prefix_module_names(from, prefix, Namespace::Value);
        }

        let mut names: Vec<String> = self
            .local_scopes
            .iter()
            .flat_map(|scope| {
                scope
                    .keys()
                    .map(|name| self.session.resolve(*name).to_owned())
            })
            .collect();
        names.extend(self.module_chain_names(from, Namespace::Value));
        names
    }

    /// Returns the names a failed type lookup of `path` from `from` could have found.
    fn type_candidate_names(&self, from: NodeId, path: &Path) -> Vec<String> {
        let Some((_, prefix)) = path.segments.split_last() else {
            return Vec::new();
        };
        if !prefix.is_empty() {
            return self.prefix_module_names(from, prefix, Namespace::Type);
        }

        let mut names: Vec<String> = self
            .generic_scopes
            .iter()
            .flat_map(|scope| {
                scope
                    .keys()
                    .map(|name| self.session.resolve(*name).to_owned())
            })
            .collect();
        names.extend(
            PRIMITIVE_TYPES
                .iter()
                .map(|(spelling, _)| (*spelling).to_owned()),
        );
        names.extend(self.module_chain_names(from, Namespace::Type));
        names
    }

    /// Returns the names a failed import of `path` could have found, across every namespace.
    fn import_candidate_names(&self, path: &Path) -> Vec<String> {
        let Some((_, prefix)) = path.segments.split_last() else {
            return Vec::new();
        };
        let root = self.ast.root_id();
        let mut names = self.prefix_module_names(root, prefix, Namespace::Value);
        names.extend(self.prefix_module_names(root, prefix, Namespace::Type));
        names.extend(self.prefix_module_names(root, prefix, Namespace::Module));
        names
    }

    /// Returns the names `namespace` binds in the module `prefix` resolves to, searching the module
    /// chain from `from` as a lookup does.
    fn prefix_module_names(
        &self,
        from: NodeId,
        prefix: &[Ident],
        namespace: Namespace,
    ) -> Vec<String> {
        match self.in_module_chain(from, |base| self.walk_modules(base, prefix)) {
            Some(module) => self.module_names(from, module, namespace),
            None => Vec::new(),
        }
    }

    /// Returns the names `namespace` binds in every module visible from `from`, and in the prelude.
    fn module_chain_names(&self, from: NodeId, namespace: Namespace) -> Vec<String> {
        self.module_chain(from)
            .into_iter()
            .chain(self.prelude)
            .flat_map(|module| self.module_names(from, module, namespace))
            .collect()
    }

    /// Returns the names `namespace` binds in `module`, dropping those not visible from `from`.
    fn module_names(&self, from: NodeId, module: NodeId, namespace: Namespace) -> Vec<String> {
        let Some(scope) = self.module_scopes.get(&module) else {
            return Vec::new();
        };
        match namespace {
            Namespace::Value => scope
                .functions
                .iter()
                .filter(|(_, id)| self.is_visible_from(from, module, self.visibility(**id)))
                .map(|(name, _)| self.session.resolve(*name).to_owned())
                .collect(),
            Namespace::Type => scope
                .types
                .iter()
                .filter(|(_, def)| {
                    self.is_visible_from(from, module, self.visibility(def.node_id()))
                })
                .map(|(name, _)| self.session.resolve(*name).to_owned())
                .collect(),
            Namespace::Module => scope
                .mods
                .keys()
                .map(|name| self.session.resolve(*name).to_owned())
                .collect(),
        }
    }

    fn last_segment_text(&self, path: &Path) -> Option<&'static str> {
        path.segments
            .last()
            .map(|segment| self.session.resolve(segment.text))
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

    pub fn push_generics(&mut self) {
        self.generic_scopes.push(HashMap::new());
    }

    pub fn pop_generics(&mut self) {
        self.generic_scopes.pop();
    }

    pub fn insert_generic(&mut self, name: Symbol, ty: Type) {
        self.generic_scopes
            .last_mut()
            .expect("insert_generic requires an open generic scope")
            .insert(name, ty);
    }

    pub fn lookup_generic(&self, name: Symbol) -> Option<Type> {
        self.generic_scopes
            .iter()
            .rev()
            .find_map(|s| s.get(&name).copied())
    }

    pub fn lookup_generic_locally(&self, name: Symbol) -> Option<Type> {
        self.generic_scopes.last()?.get(&name).copied()
    }

    //-------------------------------------------------------------------------

    pub fn insert_self(&mut self, ty: Type) {
        self.self_scopes.push(SelfScope::Defined(ty));
    }

    /// This is used for cases where due to a program error, a Self does not exist.
    /// For example, an `extend` block whose `adt_path` is unresolved
    pub fn insert_self_unresolved(&mut self) {
        self.self_scopes.push(SelfScope::Unresolved);
    }

    pub fn pop_self(&mut self) {
        self.self_scopes.pop();
    }

    // TODO: is there a better way to do this?
    // I'm not sure if I like that there is a public function just for tests
    // Also, this should probably be the name of
    // pub fn lookup_self_res(&self, span: SrcSpan) -> Res;

    /// Returns the current self entry if present and None if not
    pub fn lookup_self(&self) -> Option<Type> {
        match self.self_scopes.last() {
            Some(SelfScope::Defined(ty)) => Some(*ty),
            _ => None,
        }
    }
}
