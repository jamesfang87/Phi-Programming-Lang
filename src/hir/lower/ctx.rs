//! Drives the whole lowering pass: allocates `DefId`s, lowers each module's items into their own
//! owners, and assembles the final `Hir` once every item has been lowered.

use std::collections::HashMap;

use crate::ast;
use crate::hir::builder::DefIdAllocator;
use crate::hir::ids::DefId;
use crate::hir::lower::owner::OwnerLowerer;
use crate::hir::{Arena, Enum, Extend, Function, Hir, Module, OwnerNode, Struct, Trait};

/// `LoweringCtx` tracks the state threaded through the whole lowering pass. It holds:
///
/// - The `DefId` allocator, which assigns every definition its global id.
/// - Each owner's finished arena, once that owner has been lowered.
///
/// The module tree itself is not state here: [`ast::Ast`] has already grouped every file's
/// contents into modules, so [`lower_unit`](super::lower_unit) walks a finished tree and this
/// only has to give each module a `DefId` and lower what is in it.
///
/// [`LoweringCtx::finish`] consumes this state after every item has been lowered and packs it
/// into the final `Hir`.
pub(super) struct LoweringCtx {
    pub(super) items: DefIdAllocator,
    pub(super) owners: HashMap<DefId, Arena>,
}

impl LoweringCtx {
    pub(super) fn new() -> Self {
        LoweringCtx {
            items: DefIdAllocator::new(),
            owners: HashMap::new(),
        }
    }

    /// Lowers one module: every item declared in it becomes an owner of its own, and the module
    /// itself becomes a `Module` owner listing them.
    ///
    /// `def_id` is the module's own id, allocated by the caller, and `child_modules` holds the
    /// ids of the modules nested inside it. A submodule is one of its parent's items, like any
    /// other definition -- that's what keeps the module tree walkable downwards from the root,
    /// for passes that traverse it (name resolution) rather than address a def by id.
    pub(super) fn lower_module(
        &mut self,
        def_id: DefId,
        module: &ast::AstModule,
        child_modules: Vec<DefId>,
    ) {
        let mut items = child_modules;
        for item in &module.items {
            if let Some(item_id) = self.lower_item(def_id, item) {
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

    /// Lowers one item into its own owner, returning the `DefId` its module should list it
    /// under, or `None` for an item that declares nothing.
    fn lower_item(&mut self, module: DefId, item: &ast::Item) -> Option<DefId> {
        match &item.kind {
            ast::ItemKind::Function(f) => Some(self.lower_function(module, f)),
            ast::ItemKind::Struct(s) => Some(self.lower_struct(module, s)),
            ast::ItemKind::Enum(e) => Some(self.lower_enum(module, e)),
            ast::ItemKind::Trait(t) => Some(self.lower_trait(module, t)),
            ast::ItemKind::Extend(e) => Some(self.lower_extend(module, e)),
            // `Parser::assemble_file` sorts a file's `module` header and its imports out of its
            // items before [`ast::Ast`] groups them into modules, so neither reaches lowering.
            // `Error` stands in for an item the parser recovered from and declares nothing.
            ast::ItemKind::Module(_) | ast::ItemKind::Import(_) | ast::ItemKind::Error => None,
        }
    }

    /// Lowers a function into its own owner. `parent` is whatever declares it: a module for a
    /// free function, or the trait / `extend` block it is a method of.
    pub(super) fn lower_function(&mut self, parent: DefId, f: &ast::Function) -> DefId {
        let item_id = self.items.alloc(Some(parent));
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

    fn lower_struct(&mut self, module: DefId, s: &ast::Struct) -> DefId {
        let item_id = self.items.alloc(Some(module));
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

    fn lower_enum(&mut self, module: DefId, e: &ast::Enum) -> DefId {
        let item_id = self.items.alloc(Some(module));
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

    fn lower_trait(&mut self, module: DefId, t: &ast::Trait) -> DefId {
        let item_id = self.items.alloc(Some(module));
        // Each trait function is its own owner, lowered before the trait's own arena -- see the
        // module doc -- but parented to the trait, not to the module the trait sits in.
        let functions: Vec<DefId> = t
            .functions
            .iter()
            .map(|f| self.lower_function(item_id, f))
            .collect();
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
                functions,
                span: t.span,
            }),
        );
        ow.finish()
    }

    fn lower_extend(&mut self, module: DefId, e: &ast::Extend) -> DefId {
        let item_id = self.items.alloc(Some(module));
        let methods: Vec<DefId> = e
            .methods
            .iter()
            .map(|f| self.lower_function(item_id, f))
            .collect();
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
        ow.fill(
            root,
            OwnerNode::Extend(Extend {
                hir_id: root,
                extend_generics,
                adt_generics,
                trait_generics,
                adt_path: e.adt_path.clone(),
                trait_path: e.trait_path.clone(),
                methods,
                span: e.span,
            }),
        );
        ow.finish()
    }

    /// Packs the whole pass's bookkeeping into a dense `Hir`, once every module and item has been
    /// lowered. `root_module` is the `DefId` given to [`ast::Ast`]'s root.
    pub(super) fn finish(self, root_module: DefId) -> Hir {
        // Every allocated `DefId` owns exactly one arena, so the finished table is dense and
        // needs no placeholder to scatter into: ordering the collected arenas by id is enough
        // to index them by `DefId`. The assertion is what keeps `Hir::arenas` safe to be a
        // plain `Vec<Arena>` -- a def that never registered an arena would otherwise shift
        // every later id silently.
        let allocated = self.items.len();
        let mut owners: Vec<(DefId, Arena)> = self.owners.into_iter().collect();
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
        let parents = self.items.finish();

        // `Hir`'s fields are private to `crate::hir`, but `crate::hir::lower` and its
        // submodules are descendants of it, so the struct literal is accessible here even
        // though it isn't public.
        Hir {
            arenas,
            parents,
            root_module,
        }
    }
}
