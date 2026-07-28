//! Drives the whole lowering pass: allocates `DefId`s, accumulates each module's contents as
//! files are lowered into it, and assembles the final `Hir` once every item has been lowered.

use std::collections::HashMap;

use crate::ast::{self, Ident, Path, Symbol};
use crate::hir::builder::DefIdAllocator;
use crate::hir::ids::DefId;
use crate::hir::lower::owner::OwnerLowerer;
use crate::hir::{Arena, Enum, Extend, Function, Hir, Module, OwnerNode, Struct, Trait};
use crate::lexer::src_span::SrcSpan;

/// A module's contents, accumulated across every file that declares into it, before its `Module`
/// node is built in [`LoweringCtx::finish`].
pub(super) struct ModuleScratch {
    path: Path,
    items: Vec<DefId>,
    imports: Vec<ast::Import>,
}

/// `LoweringCtx` tracks the state threaded through the whole lowering pass. It holds:
///
/// - The `DefId` allocator, which assigns every definition its global id.
/// - Each owner's finished arena, once that owner has been lowered.
/// - The modules being assembled, since a module accumulates contributions from every file
///   that declares into it before it becomes a real `Module` owner.
///
/// [`LoweringCtx::finish`] consumes this state after every item has been lowered and packs it
/// into the final `Hir`.
pub(super) struct LoweringCtx {
    pub(super) items: DefIdAllocator,
    pub(super) owners: HashMap<DefId, Arena>,
    /// Module path (dotted segments) -> its `DefId`, populated as modules are discovered --
    /// either from an explicit `module a::b;` declaration or synthesized as an ancestor of one.
    modules_by_path: HashMap<Vec<Symbol>, DefId>,
    module_scratch: HashMap<DefId, ModuleScratch>,
    root_module: DefId,
}

impl LoweringCtx {
    pub(super) fn new() -> Self {
        let mut items = DefIdAllocator::new();
        let root = items.alloc(None);

        let mut modules_by_path = HashMap::new();
        modules_by_path.insert(Vec::new(), root);

        let mut module_scratch = HashMap::new();
        module_scratch.insert(
            root,
            ModuleScratch {
                path: Path {
                    segments: Vec::new(),
                    span: SrcSpan::new(0, 0),
                },
                items: Vec::new(),
                imports: Vec::new(),
            },
        );

        LoweringCtx {
            items,
            owners: HashMap::new(),
            modules_by_path,
            module_scratch,
            root_module: root,
        }
    }

    pub(super) fn root_module(&self) -> DefId {
        self.root_module
    }

    /// Finds or creates the `DefId` for the module named by `segments`, synthesizing any
    /// ancestor module a file never declares on its own.
    pub(super) fn module_for_path(&mut self, segments: &[Ident]) -> DefId {
        let mut current = self.root_module;
        let mut prefix: Vec<Symbol> = Vec::new();
        for (i, seg) in segments.iter().enumerate() {
            prefix.push(seg.text);
            if let Some(&existing) = self.modules_by_path.get(&prefix) {
                current = existing;
                continue;
            }
            // `current` is still the module one level up: the parent of the one being created.
            let item_id = self.items.alloc(Some(current));
            let path_segments = segments[..=i].to_vec();
            let span = path_segments[0]
                .span
                .merge(path_segments[path_segments.len() - 1].span);
            self.module_scratch.insert(
                item_id,
                ModuleScratch {
                    path: Path {
                        segments: path_segments,
                        span,
                    },
                    items: Vec::new(),
                    imports: Vec::new(),
                },
            );
            self.modules_by_path.insert(prefix.clone(), item_id);
            // A submodule is one of its parent's items, like any other definition -- that's what
            // keeps the module tree walkable downwards from the root, for passes that traverse
            // it (name resolution) rather than address a def by id.
            self.declare(current, item_id);
            current = item_id;
        }
        current
    }

    pub(super) fn lower_file(&mut self, module: DefId, unit: &ast::ParsedSrcFile) {
        for import in &unit.imports {
            self.module_scratch
                .get_mut(&module)
                .unwrap()
                .imports
                .push(import.clone());
        }
        for item in &unit.items {
            self.lower_item(module, item);
        }
    }

    fn lower_item(&mut self, module: DefId, item: &ast::Item) {
        match &item.kind {
            ast::ItemKind::Function(f) => {
                let item_id = self.lower_function(module, f);
                self.declare(module, item_id);
            }
            ast::ItemKind::Struct(s) => {
                let item_id = self.lower_struct(module, s);
                self.declare(module, item_id);
            }
            ast::ItemKind::Enum(e) => {
                let item_id = self.lower_enum(module, e);
                self.declare(module, item_id);
            }
            ast::ItemKind::Trait(t) => {
                let item_id = self.lower_trait(module, t);
                self.declare(module, item_id);
            }
            ast::ItemKind::Extend(e) => {
                let item_id = self.lower_extend(module, e);
                self.declare(module, item_id);
            }
            ast::ItemKind::Import(import) => {
                self.module_scratch
                    .get_mut(&module)
                    .unwrap()
                    .imports
                    .push(import.clone());
            }
            // A file's own `module` header is handled by `lower_unit` (top-level) before any
            // item is visited; a bare `module` item mid-file has nothing further to do here.
            ast::ItemKind::Module(_) | ast::ItemKind::Error => {}
        }
    }

    fn declare(&mut self, module: DefId, item_id: DefId) {
        self.module_scratch
            .get_mut(&module)
            .unwrap()
            .items
            .push(item_id);
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
        let body = f.body.as_ref().map(|b| ow.lower_block(b));
        let hir_id = ow.hir_id(root);
        ow.fill(
            root,
            OwnerNode::Function(Function {
                hir_id,
                visibility: f.visibility,
                name: f.name,
                generics,
                self_param,
                params,
                ret,
                body,
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
        let hir_id = ow.hir_id(root);
        ow.fill(
            root,
            OwnerNode::Struct(Struct {
                hir_id,
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
        let hir_id = ow.hir_id(root);
        ow.fill(
            root,
            OwnerNode::Enum(Enum {
                hir_id,
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
        let hir_id = ow.hir_id(root);
        ow.fill(
            root,
            OwnerNode::Trait(Trait {
                hir_id,
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
        let extend_generics = e
            .extend_generics
            .as_deref()
            .unwrap_or(&[])
            .iter()
            .map(|t| ow.lower_ty(t))
            .collect();
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
        let hir_id = ow.hir_id(root);
        ow.fill(
            root,
            OwnerNode::Extend(Extend {
                hir_id,
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

    /// Turns every accumulated `ModuleScratch` into a real `Module` owner, then packs the whole
    /// pass's bookkeeping into a dense `Hir`.
    pub(super) fn finish(mut self) -> Hir {
        let module_defs: Vec<DefId> = self.module_scratch.keys().copied().collect();
        for item_id in module_defs {
            let scratch = self.module_scratch.remove(&item_id).unwrap();
            let span = scratch.path.span;
            let mut ow = OwnerLowerer::new(&mut self, item_id);
            let root = ow.reserve_root();
            let imports = scratch
                .imports
                .iter()
                .map(|imp| ow.lower_import(imp))
                .collect();
            let hir_id = ow.hir_id(root);
            ow.fill(
                root,
                OwnerNode::Module(Module {
                    hir_id,
                    path: scratch.path,
                    items: scratch.items,
                    imports,
                    span,
                }),
            );
            ow.finish();
        }

        let n = self.items.len();
        let mut owners: Vec<Option<Arena>> = (0..n).map(|_| None).collect();
        for (item_id, arena) in self.owners {
            owners[item_id.index()] = Some(arena);
        }
        let parents = self.items.finish();

        // `Hir`'s fields are private to `crate::hir`, but `crate::hir::lower` and its
        // submodules are descendants of it, so the struct literal is accessible here even
        // though it isn't public.
        Hir {
            arenas: owners,
            parents,
            root_module: self.root_module,
        }
    }
}
