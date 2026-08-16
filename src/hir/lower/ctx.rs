//! Orchestrates lowering: allocates `DefId`s, lowers each module's items into their own
//! owners, and assembles the final `Hir` once every item has been lowered.

use std::collections::HashMap;

use crate::ast::{self, NodeId};
use crate::hir::builder::DefIdAllocator;
use crate::hir::ids::{DefId, HirId};
use crate::hir::lower::owner::OwnerLowerer;
use crate::hir::{
    Arena, Enum, Extend, Function, Hir, Local, Module, OwnerNode, Res, Struct, Trait, TyDef, Type,
};
use crate::nameres::NameResolutions;
use crate::nameres::{Local as SLocal, Res as SRes, TyDef as STyDef, Type as SType};

pub(super) struct LoweringCtx<'res> {
    pub(super) def_id_allocator: DefIdAllocator,
    pub(super) def_ids: HashMap<NodeId, DefId>,
    pub(super) hir_ids: HashMap<NodeId, HirId>,
    pub(super) method_defs: HashMap<NodeId, Vec<DefId>>,
    pub(super) arenas: HashMap<DefId, Arena>,
    nameres: &'res NameResolutions,
}

impl<'res> LoweringCtx<'res> {
    pub(super) fn new(nameres: &'res NameResolutions) -> Self {
        LoweringCtx {
            def_id_allocator: DefIdAllocator::new(),
            def_ids: HashMap::new(),
            method_defs: HashMap::new(),
            arenas: HashMap::new(),
            nameres,
            hir_ids: HashMap::new(),
        }
    }

    pub(super) fn record_hir_id(&mut self, node: NodeId, hir_id: HirId) {
        self.hir_ids.insert(node, hir_id);
    }

    fn def_id_of(&self, node: NodeId, what: &str) -> DefId {
        *self.def_ids.get(&node).unwrap_or_else(|| {
            panic!("lowering bug: {node:?}, expected to already have a DefId as {what}, has none")
        })
    }

    fn hir_id_of(&self, node: NodeId, what: &str) -> HirId {
        *self.hir_ids.get(&node).unwrap_or_else(|| {
            panic!("lowering bug: {node:?}, expected to already have a HirId as {what}, has none")
        })
    }

    fn translate_res(&self, res: SRes) -> Res {
        match res {
            SRes::Err => Res::Err,
            SRes::Function(node) => Res::Function(self.def_id_of(node, "a function item")),
            SRes::Module(node) => Res::Module(self.def_id_of(node, "a module")),
            SRes::Type(SType::Prim(prim)) => Res::Type(Type::Prim(prim)),
            SRes::Type(SType::Generic(node)) => {
                Res::Type(Type::Generic(self.hir_id_of(node, "a generic parameter")))
            }
            SRes::Type(SType::Def(STyDef::Struct(node))) => Res::Type(Type::Def(TyDef::Struct(
                self.def_id_of(node, "a struct item"),
            ))),
            SRes::Type(SType::Def(STyDef::Enum(node))) => {
                Res::Type(Type::Def(TyDef::Enum(self.def_id_of(node, "an enum item"))))
            }
            SRes::Type(SType::Def(STyDef::Trait(node))) => Res::Type(Type::Def(TyDef::Trait(
                self.def_id_of(node, "a trait item"),
            ))),
            SRes::Local(SLocal::Param(node)) => {
                Res::Local(Local::Param(self.hir_id_of(node, "a parameter")))
            }
            SRes::Local(SLocal::SelfParam(node)) => {
                Res::Local(Local::SelfParam(self.hir_id_of(node, "a self parameter")))
            }
            SRes::Local(SLocal::Variable(node)) => {
                Res::Local(Local::Variable(self.hir_id_of(node, "a binding pattern")))
            }
        }
    }

    pub(super) fn lower_path(&self, owner: NodeId, path: &ast::Path) -> crate::hir::Path {
        let res = self.nameres.get(owner, path).unwrap_or_else(|| {
            panic!(
                "lowering bug: {owner:?} owns no recorded resolution for the path `{}`.",
                path.segments
                    .iter()
                    .map(|s| crate::ast::interner::Interner::resolve(s.text))
                    .collect::<Vec<_>>()
                    .join("::")
            )
        });
        crate::hir::Path {
            segments: path.segments.clone(),
            span: path.span,
            res: self.as_self_ty(path, self.translate_res(res)),
        }
    }

    // TODO: Code smell... probably need refactoring
    fn as_self_ty(&self, path: &ast::Path, res: Res) -> Res {
        match res {
            Res::Type(Type::Def(tydef)) if is_self_path(path) => Res::SelfTy(tydef),
            other => other,
        }
    }

    /// Pre-allocates `item`'s `DefId` and records it in `def_ids`.
    /// For a trait or `extend` block, this also pre-allocates a `DefId`
    /// for every method it declares and records those in `method_defs`.
    ///
    /// This MUST allocate for exactly the `ItemKind`s [`LoweringCtx::lower_item`] returns
    /// `Some` for and nothing else. Otherwise, an id allocated here for a definition that never gets
    /// lowered or an item lowered without a pre-allocated id to find breaks
    /// the dense-arena invariant.
    pub(super) fn prealloc_item(&mut self, module: DefId, item: &ast::Item) {
        let def_id = match &item.kind {
            ast::ItemKind::Function(_)
            | ast::ItemKind::Struct(_)
            | ast::ItemKind::Enum(_)
            | ast::ItemKind::Trait(_)
            | ast::ItemKind::Extend(_) => self.def_id_allocator.alloc(Some(module)),
            // Mirrors `lower_item`: neither declares anything to give a `DefId` to.
            ast::ItemKind::ModuleDecl(_) | ast::ItemKind::Import(_) | ast::ItemKind::Error => {
                return;
            }
        };
        self.def_ids.insert(item.id, def_id);

        match &item.kind {
            ast::ItemKind::Trait(t) => {
                let methods = t
                    .functions
                    .iter()
                    .map(|_| self.def_id_allocator.alloc(Some(def_id)))
                    .collect();
                self.method_defs.insert(item.id, methods);
            }
            ast::ItemKind::Extend(e) => {
                let methods = e
                    .methods
                    .iter()
                    .map(|_| self.def_id_allocator.alloc(Some(def_id)))
                    .collect();
                self.method_defs.insert(item.id, methods);
            }
            _ => {}
        }
    }

    pub(super) fn lower_module(
        &mut self,
        def_id: DefId,
        module: &ast::Module,
        child_modules: Vec<DefId>,
    ) {
        let mut items = child_modules;
        for item in &module.items {
            if let Some(item_id) = self.lower_item(item) {
                items.push(item_id);
            }
        }

        let mut ow = OwnerLowerer::new(self, def_id);
        let root = ow.reserve_root();
        let imports = module
            .imports
            .iter()
            .map(|imp| ow.lower_import(imp))
            .collect();
        ow.fill(
            root,
            OwnerNode::Module(Module {
                hir_id: root,
                items,
                imports,
                span: module.path.span,
                path: module.path.clone(),
            }),
        );
        ow.finish();
    }

    fn lower_item(&mut self, item: &ast::Item) -> Option<DefId> {
        match &item.kind {
            ast::ItemKind::Function(f) => {
                let item_id = self.def_ids[&item.id];
                Some(self.lower_function(item_id, f))
            }
            ast::ItemKind::Struct(s) => {
                let item_id = self.def_ids[&item.id];
                Some(self.lower_struct(item_id, s))
            }
            ast::ItemKind::Enum(e) => {
                let item_id = self.def_ids[&item.id];
                Some(self.lower_enum(item_id, e))
            }
            ast::ItemKind::Trait(t) => {
                let item_id = self.def_ids[&item.id];
                let method_ids = self.method_defs.remove(&item.id).unwrap_or_default();
                Some(self.lower_trait(item_id, method_ids, t))
            }
            ast::ItemKind::Extend(e) => {
                let item_id = self.def_ids[&item.id];
                let method_ids = self.method_defs.remove(&item.id).unwrap_or_default();
                Some(self.lower_extend(item.id, item_id, method_ids, e))
            }
            // `Parser::assemble_file` sorts a file's `module` header and its imports out of its
            // items, so neither reaches lowering.
            ast::ItemKind::ModuleDecl(_) | ast::ItemKind::Import(_) | ast::ItemKind::Error => None,
        }
    }

    pub(super) fn lower_function(&mut self, item_id: DefId, f: &ast::Function) -> DefId {
        let mut ow = OwnerLowerer::new(self, item_id);
        let root = ow.reserve_root();
        let generics = ow.lower_generics(&f.generics);
        let self_param = f.self_param.as_ref().map(|sp| ow.lower_self_param(sp));
        let params = f.params.iter().map(|p| ow.lower_param(p)).collect();
        let ret = f.ret.as_ref().map(|t| ow.lower_ty(t));
        let block = f.block.as_ref().map(|b| ow.lower_block(b));
        ow.fill(
            root,
            OwnerNode::Function(Function {
                hir_id: root,
                visibility: f.visibility,
                name: f.name,
                generics,
                self_param,
                params,
                ret,
                block,
                span: f.span,
            }),
        );
        ow.finish()
    }

    fn lower_struct(&mut self, item_id: DefId, s: &ast::Struct) -> DefId {
        let mut ow = OwnerLowerer::new(self, item_id);
        let root = ow.reserve_root();
        let generics = ow.lower_generics(s.generics.as_deref().unwrap_or(&[]));
        let fields = s.fields.iter().map(|f| ow.lower_field(f)).collect();
        ow.fill(
            root,
            OwnerNode::Struct(Struct {
                hir_id: root,
                visibility: s.visibility,
                name: s.name,
                generics,
                fields,
                span: s.span,
            }),
        );
        ow.finish()
    }

    fn lower_enum(&mut self, item_id: DefId, e: &ast::Enum) -> DefId {
        let mut ow = OwnerLowerer::new(self, item_id);
        let root = ow.reserve_root();
        let generics = ow.lower_generics(e.generics.as_deref().unwrap_or(&[]));
        let variants = e.variants.iter().map(|v| ow.lower_variant(v)).collect();
        ow.fill(
            root,
            OwnerNode::Enum(Enum {
                hir_id: root,
                visibility: e.visibility,
                name: e.name,
                generics,
                variants,
                span: e.span,
            }),
        );
        ow.finish()
    }

    fn lower_trait(&mut self, item_id: DefId, method_ids: Vec<DefId>, t: &ast::Trait) -> DefId {
        // Each trait function is its own owner, parented to the trait rather than to the module
        // the trait sits in. Its `DefId` was pre-allocated too, positionally, by `prealloc_item`
        // -- which is also why `lower_function(id, f)` always hands `id` straight back (see
        // `OwnerLowerer::finish`): a function's `DefId` never depends on lowering its own body.
        // So the trait's `functions` field needs nothing from actually lowering a function --
        // it is just `method_ids`, already known here. That means the trait's own arena is
        // complete as soon as its generics are lowered: `ow` fills and finishes right there,
        // releasing `self` before any function is lowered.
        //
        // The generics still have to come first: a function's signature or body can name the
        // trait's own generics (`trait C<T> { fun get(self) -> T; }`), so those generics need
        // `HirId`s recorded (`ow.lower_generics` does this through `cx.record_hir_id`) before
        // `lower_function` can resolve a reference to one -- and lowering them before `ow`
        // finishes is what guarantees that.
        let mut ow = OwnerLowerer::new(self, item_id);
        let root = ow.reserve_root();
        let generics = ow.lower_generics(t.generics.as_deref().unwrap_or(&[]));
        ow.fill(
            root,
            OwnerNode::Trait(Trait {
                hir_id: root,
                visibility: t.visibility,
                name: t.name,
                generics,
                functions: method_ids.clone(),
                span: t.span,
            }),
        );
        let trait_id = ow.finish();

        debug_assert_eq!(method_ids.len(), t.functions.len());
        for (f, id) in t.functions.iter().zip(method_ids) {
            self.lower_function(id, f);
        }
        trait_id
    }

    /// `node_id` is the enclosing `ast::Item`'s own id -- `Extend` has none of its own
    /// (`src/ast.rs`) -- which is what `adt_path`/`trait_path` are recorded under in
    /// `NameResolutions`, per `Resolver::visit_extend`.
    fn lower_extend(
        &mut self,
        node_id: NodeId,
        item_id: DefId,
        method_ids: Vec<DefId>,
        e: &ast::Extend,
    ) -> DefId {
        // See `lower_trait`: same pre-allocated, positional pairing for a method's `DefId`, and
        // the same reason `methods` needs nothing from actually lowering a method -- it is just
        // `method_ids`. `extend_generics` still has to come first, before everything else built
        // here: `adt_generics`/`trait_generics`/`adt_path`/`trait_path` can name it, same as
        // `extend<T> Box<T> for Container<T>` names `T` in `Box<T>`, and so can a method. The
        // former only get a chance to because they are lowered here, through `ow`, before it
        // finishes; the latter, because `self` is free again by the time each method lowers.
        let mut ow = OwnerLowerer::new(self, item_id);
        let root = ow.reserve_root();
        let extend_generics = ow.lower_generics(e.extend_generics.as_deref().unwrap_or(&[]));
        let adt_generics = e
            .adt_generics
            .as_deref()
            .unwrap_or(&[])
            .iter()
            .map(|t| ow.lower_ty(t))
            .collect();
        let trait_generics = e
            .trait_generics
            .as_deref()
            .unwrap_or(&[])
            .iter()
            .map(|t| ow.lower_ty(t))
            .collect();
        let adt_path = ow.cx.lower_path(node_id, &e.adt_path);
        let trait_path = e.trait_path.as_ref().map(|p| ow.cx.lower_path(node_id, p));
        ow.fill(
            root,
            OwnerNode::Extend(Extend {
                hir_id: root,
                extend_generics,
                adt_generics,
                trait_generics,
                adt_path,
                trait_path,
                methods: method_ids.clone(),
                span: e.span,
            }),
        );
        let extend_id = ow.finish();

        debug_assert_eq!(method_ids.len(), e.methods.len());
        for (f, id) in e.methods.iter().zip(method_ids) {
            self.lower_function(id, f);
        }
        extend_id
    }

    pub(super) fn finish(self, root_module: DefId) -> Hir {
        let allocated = self.def_id_allocator.len();
        let mut owners: Vec<(DefId, Arena)> = self.arenas.into_iter().collect();
        owners.sort_by_key(|(item_id, _)| item_id.index());
        debug_assert!(
            owners.len() == allocated
                && owners
                    .iter()
                    .enumerate()
                    .all(|(index, (item_id, _))| item_id.index() == index),
            "every allocated DefId owns exactly one arena"
        );
        let arenas: Vec<Arena> = owners.into_iter().map(|(_, arena)| arena).collect();

        let lang_items =
            crate::langitems::hir::LangItems::from_ast(self.nameres.lang_items(), |node| {
                self.def_ids.get(&node).copied()
            });

        let parent_of = self.def_id_allocator.finish();

        Hir {
            arenas,
            parent_of,
            root_module,
            lang_items,
        }
    }
}

fn is_self_path(path: &ast::Path) -> bool {
    match path.segments.as_slice() {
        [segment] => crate::ast::interner::Interner::resolve(segment.text) == "Self",
        _ => false,
    }
}
