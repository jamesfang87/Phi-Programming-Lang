//! Orchestrates lowering: allocates `DefId`s, lowers each module's items into their own
//! owners, and assembles the final `Hir` once every item has been lowered.

use std::collections::{HashMap, HashSet};

use crate::ast::{self, NodeId};
use crate::hir::builder::DefIdAllocator;
use crate::hir::ids::{DefId, HirId};
use crate::hir::lower::owner::OwnerLowerer;
use crate::hir::{
    Arena, Enum, Extend, Function, Hir, Local, Module, OwnerNode, Res, Struct, Trait, TyDef, Type,
};
use crate::nameres::NameResolutions;
use crate::nameres::{Local as SLocal, Res as SRes, TyDef as STyDef, Type as SType};

/// `LoweringCtx` tracks the state threaded through lowering. It holds:
///
/// - The `DefId` allocator, which assigns every definition its global id.
/// - `def_ids`, which is how a definition's pre-allocated id is found again once lowering
///   reaches it (see the module-level pre-allocation pass in
///   [`lower_unit`](super::lower_unit)).
/// - `method_defs`, the same thing for a trait's functions and an `extend` block's methods. They
///   are kept separately because a method is a bare `ast::Function` with no `NodeId` of its own
///   to key `def_ids` by -- only the `Item` wrapping a top-level definition has one -- so each
///   trait/`extend` item's `NodeId` instead maps to the ordered `Vec<DefId>` pre-allocated for
///   its methods, consumed positionally in the same order lowering visits them.
/// - Each owner's finished arena, once that owner has been lowered.
/// - `generics_ready`, a debug-only record of which owners (traits, `extend` blocks) have already
///   had their own generics lowered. [`LoweringCtx::lower_trait`] and
///   [`LoweringCtx::lower_extend`] assert against it right before lowering a nested
///   function/method, to catch a regression back to lowering one before its parent's generics
///   exist -- see the comment there for why that ordering matters.
///
/// The module tree itself is not state here: [`ast::Ast`] has already grouped every file's
/// contents into modules, so [`lower_unit`](super::lower_unit) walks a finished tree and this
/// only has to give each module a `DefId` and lower what is in it.
///
/// [`LoweringCtx::finish`] consumes this state after every item has been lowered and assembles it
/// into the final `Hir`.
///
/// `'res` is the lifetime of the [`NameResolutions`] this pass consumes -- AST-level name
/// resolution's output, borrowed for the whole pass rather than cloned, since lowering only ever
/// reads it.
pub(super) struct LoweringCtx<'res> {
    pub(super) items: DefIdAllocator,
    pub(super) def_ids: HashMap<NodeId, DefId>,
    pub(super) method_defs: HashMap<NodeId, Vec<DefId>>,
    pub(super) owners: HashMap<DefId, Arena>,
    generics_ready: HashSet<DefId>,
    /// AST-level name resolution's answer for every path in the program, keyed by the `NodeId`
    /// that owns each one. [`LoweringCtx::lower_path`] is what turns an entry here into a
    /// `hir::Path`.
    nameres: &'res NameResolutions,
    /// Maps a lowered node's `NodeId` in the AST to the `HirId` it became, for exactly the node
    /// kinds an AST-level `Res` can point at without a `DefId`: a generic parameter, a function
    /// parameter, a `self` parameter, and a binding pattern (`src/hir/ids.rs` -- these are arena
    /// nodes, not definitions). [`LoweringCtx::translate_res`] is the only reader.
    ///
    /// Every kind of lookup this map serves is available by the time it's needed: preallocation
    /// orders a trait/`extend` block's own generics before any function/method that could name
    /// them, and an ordinary binding always lowers before the expression that reads it, because
    /// lowering visits a block's statements (and a `let`'s own initializer before its pattern
    /// binds -- see `ast::visit::walk_stmt`) in source order. A miss here is a lowering-order
    /// bug, not a resolution failure -- see [`LoweringCtx::hir_id_of`].
    node_to_hir: HashMap<NodeId, HirId>,
}

impl<'res> LoweringCtx<'res> {
    pub(super) fn new(nameres: &'res NameResolutions) -> Self {
        LoweringCtx {
            items: DefIdAllocator::new(),
            def_ids: HashMap::new(),
            method_defs: HashMap::new(),
            owners: HashMap::new(),
            generics_ready: HashSet::new(),
            nameres,
            node_to_hir: HashMap::new(),
        }
    }

    /// Records that the AST node `node` became `hir_id` during lowering. Every site that lowers
    /// a generic parameter, a function/`self`/closure parameter, or a binding pattern calls this
    /// right after reserving that node's `HirId`, which is what lets a later reference to the
    /// same name -- resolved by the AST-level resolver against `node`'s `NodeId` -- be translated
    /// into the `HirId` [`LoweringCtx::translate_res`] needs.
    pub(super) fn record_hir_id(&mut self, node: NodeId, hir_id: HirId) {
        self.node_to_hir.insert(node, hir_id);
    }

    /// The `DefId` `node` was pre-allocated, expected because `node` is `what`.
    ///
    /// Every `NodeId` an AST-level `Res::Function`/`Res::Module`/`Res::Type(Type::Def(..))` carries names a
    /// module or an item `LoweringCtx::prealloc_item` (or the module pre-allocation pass in
    /// [`super::lower_unit`]) already gave a `DefId`, regardless of lowering order -- so a miss
    /// here means the id was recorded under the wrong `NodeId` somewhere, not that lowering ran
    /// out of order. Panicking is what keeps that bug visible as itself, rather than as a
    /// mysterious `Res::Err` surfacing as a type error far from its cause.
    fn def_id_of(&self, node: NodeId, what: &str) -> DefId {
        *self.def_ids.get(&node).unwrap_or_else(|| {
            panic!("lowering bug: {node:?}, expected to already have a DefId as {what}, has none")
        })
    }

    /// The `HirId` `node` lowered to, expected because `node` is `what`.
    ///
    /// See [`LoweringCtx::node_to_hir`] for why every lookup here is expected to already be
    /// populated. A miss is a lowering-order bug: panicking keeps the bug visible as itself,
    /// per the task's own instruction, rather than converting to `Res::Err` and reappearing later
    /// as an unexplained type error.
    fn hir_id_of(&self, node: NodeId, what: &str) -> HirId {
        *self.node_to_hir.get(&node).unwrap_or_else(|| {
            panic!("lowering bug: {node:?}, expected to already have a HirId as {what}, has none")
        })
    }

    /// Translates an AST-level `Res` -- addressed by `NodeId`, as AST-level resolution left it --
    /// into its `hir::Res` analogue, addressed by `DefId`/`HirId`. See the module docs on
    /// [`crate::hir::path`] for why nominal items and functions carry a `DefId` while locals and
    /// generics carry a `HirId`.
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

    /// Builds the `hir::Path` for `path`, as written on `owner` -- the `NodeId` `path` is
    /// recorded under in [`NameResolutions`] (see `NameResolutions::get`).
    ///
    /// Every path AST-level resolution visits records an entry, `Res::Err` included (see
    /// the AST-level `Res`'s own docs on why absence and failure are kept apart). A missing
    /// entry here indicates `owner` is wrong, not that resolution failed. This differs from a
    /// missing `node_to_hir`/`def_ids` entry inside [`Self::translate_res`], which is checked
    /// (and panics) separately once an entry is found.
    pub(super) fn lower_path(&self, owner: NodeId, path: &ast::Path) -> crate::hir::Path {
        let res = self.nameres.get(owner, path).unwrap_or_else(|| {
            panic!(
                "lowering bug: {owner:?} owns no recorded resolution for the path `{}` -- \
                 every path AST-level resolution visits is expected to have one, `Res::Err` \
                 included",
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

    /// Rewrites `res` into `Res::SelfTy` when `path` is exactly the written keyword `Self`,
    /// leaving every other path untouched.
    ///
    /// `Self` resolves through the ordinary type-namespace lookup, same as any other path (see
    /// `SymbolTable::lookup_type_path`), so `res` arrives as an ordinary `Res::Type(Type::Def(_))`
    /// -- there is no AST-level `Res::SelfTy` to translate. The distinguishing characteristic
    /// of `Self` is not its resolution target but its written form as the keyword `Self`, which
    /// [`is_self_path`] recognizes: the lexer tokenizes the text `Self` as the reserved
    /// `UpperSelfKw`, never as an `Identifier`, so no ordinary path segment can collide with it.
    /// Giving `Self` its own `Res` arm, here rather than at the resolver, lets `lower_ty`'s
    /// `Def` arm skip the two struct/enum-only checks that don't hold for `Self` -- see
    /// `hir::Res::SelfTy`'s own docs.
    fn as_self_ty(&self, path: &ast::Path, res: Res) -> Res {
        match res {
            Res::Type(Type::Def(tydef)) if is_self_path(path) => Res::SelfTy(tydef),
            other => other,
        }
    }

    /// Builds the `hir::Path` for a struct literal's own type name (`ExprKind::Ctor`'s `path`),
    /// falling back to `Res::Err` rather than panicking on a missing entry.
    ///
    /// `Resolver::visit_expr`'s `Ctor` arm (`src/nameres/resolver.rs`) does record an entry for
    /// this, keyed on the expression's own `NodeId` exactly like an ordinary `ExprKind::Path` --
    /// so this behaves the same as [`Self::lower_path`]. The fallback avoids panicking because
    /// this is a single, narrow call site (one `Option<Path>` field on one node kind) rather than
    /// the general path-lookup every other caller shares. Re-checking this by hand remains
    /// straightforward if some future edit reintroduces a gap here, unlike `Self::lower_path`'s
    /// panic, whose whole point is to surface a miss loudly rather than let it hide.
    pub(super) fn lower_ctor_path(&self, owner: NodeId, path: &ast::Path) -> crate::hir::Path {
        let res = self
            .nameres
            .get(owner, path)
            .map(|res| self.translate_res(res))
            .unwrap_or(Res::Err);
        crate::hir::Path {
            segments: path.segments.clone(),
            span: path.span,
            res,
        }
    }

    /// Pre-allocates `item`'s `DefId`, parented to `module`, and records it in `def_ids` keyed by
    /// the item's own `NodeId`. For a trait or `extend` block, this also pre-allocates a `DefId`
    /// for every method it declares, parented to the item itself rather than to `module`, and
    /// records those in `method_defs`.
    ///
    /// This must allocate for exactly the `ItemKind`s [`LoweringCtx::lower_item`] returns
    /// `Some` for, and nothing else: an id allocated here for a definition that never gets
    /// lowered -- or, worse, an item lowered without a pre-allocated id here to find -- breaks
    /// the dense-arena invariant [`LoweringCtx::finish`] relies on.
    pub(super) fn prealloc_item(&mut self, module: DefId, item: &ast::Item) {
        let def_id = match &item.kind {
            ast::ItemKind::Function(_)
            | ast::ItemKind::Struct(_)
            | ast::ItemKind::Enum(_)
            | ast::ItemKind::Trait(_)
            | ast::ItemKind::Extend(_) => self.items.alloc(Some(module)),
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
                    .map(|_| self.items.alloc(Some(def_id)))
                    .collect();
                self.method_defs.insert(item.id, methods);
            }
            ast::ItemKind::Extend(e) => {
                let methods = e
                    .methods
                    .iter()
                    .map(|_| self.items.alloc(Some(def_id)))
                    .collect();
                self.method_defs.insert(item.id, methods);
            }
            _ => {}
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

    /// Lowers one item into its own owner, returning the `DefId` its module should list it
    /// under, or `None` for an item that declares nothing.
    ///
    /// Every id here was already allocated by [`LoweringCtx::prealloc_item`] before lowering
    /// began, so this only ever looks one up -- it never allocates.
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
            // items before [`ast::Ast`] groups them into modules, so neither reaches lowering.
            // `Error` stands in for an item the parser recovered from and declares nothing.
            ast::ItemKind::ModuleDecl(_) | ast::ItemKind::Import(_) | ast::ItemKind::Error => None,
        }
    }

    /// Lowers a function into its own owner, under the `DefId` [`LoweringCtx::prealloc_item`]
    /// already allocated for it -- a free function's own, or the one pre-allocated for it as a
    /// trait/`extend` method.
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
        // the trait sits in. Its `DefId` was pre-allocated too, positionally, by
        // `prealloc_item`. A function's signature or body can name the trait's own generics
        // (`trait C<T> { fun get(self) -> T; }`), so those generics must exist -- have `HirId`s
        // allocated in the trait's arena -- before any function is lowered, for path lowering to
        // resolve such a reference into.
        //
        // That rules out lowering the functions after creating this owner's `OwnerLowerer` the
        // straightforward way: `OwnerLowerer::new` holds `&mut self` for as long as it's alive,
        // and lowering a function needs that same `&mut self` too, through
        // `LoweringCtx::lower_function`. So the trait's arena is built in two pieces instead:
        // `begin_with_generics` reserves the root -- still landing at `LocalId::OWNER` -- and
        // lowers the generics into a detached `ArenaBuilder` that borrows nothing from `self`
        // (sound because a `Generic`'s only children are its bound paths, plain cloned data, not
        // nodes lowered through `cx`). With that phase's borrow released, `self` is free to lower
        // every function; only then does `resume` reattach the builder to fill in the trait's own
        // node and finish the arena.
        let generics_ast = t.generics.as_deref().unwrap_or(&[]);
        let (builder, root, generics) =
            OwnerLowerer::begin_with_generics(self, item_id, generics_ast);
        self.generics_ready.insert(item_id);

        debug_assert_eq!(method_ids.len(), t.functions.len());
        let functions: Vec<DefId> = t
            .functions
            .iter()
            .zip(method_ids)
            .map(|(f, id)| {
                debug_assert!(
                    self.generics_ready.contains(&item_id),
                    "a trait function must not be lowered before the trait's own generics"
                );
                self.lower_function(id, f)
            })
            .collect();

        let mut ow = OwnerLowerer::resume(self, builder);
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
        // the same two-phase split -- the `extend` block's own (`extend<T>`) generics must be
        // lowered before any method that could name them, but lowering a method needs `&mut
        // self` while `OwnerLowerer::new` would already be holding it.
        let extend_generics_ast = e.extend_generics.as_deref().unwrap_or(&[]);
        let (builder, root, extend_generics) =
            OwnerLowerer::begin_with_generics(self, item_id, extend_generics_ast);
        self.generics_ready.insert(item_id);

        debug_assert_eq!(method_ids.len(), e.methods.len());
        let methods: Vec<DefId> = e
            .methods
            .iter()
            .zip(method_ids)
            .map(|(f, id)| {
                debug_assert!(
                    self.generics_ready.contains(&item_id),
                    "an extend block's method must not be lowered before its own generics"
                );
                self.lower_function(id, f)
            })
            .collect();

        let mut ow = OwnerLowerer::resume(self, builder);
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
        // Both `adt_path` and `trait_path` go through the strict `lower_path` below even when
        // `extend Foo with Foo` (or a duplicate bound) meant the resolver's own guard only
        // *recorded* the first writing and dropped the second -- see
        // `Resolver::visit_extend`/`resolve_bounds`. This works only because
        // `NameResolutions::get` matches by path *text* (`Path`'s `PartialEq` compares segment
        // symbols, not identity), so looking up the dropped second path finds the first entry
        // instead of missing and panicking. If a future guard ever dropped *both* records for a
        // duplicate path -- rather than keeping the first -- this would turn `extend Foo with
        // Foo` into a lowering panic instead of the diagnostic it reports today.
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
                methods,
                span: e.span,
            }),
        );
        ow.finish()
    }

    /// Assembles lowering's bookkeeping into a dense `Hir`, once every module and item has been
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

        // Every lang item that resolved at all names a struct, enum, or trait item, and every
        // one of those got its `DefId` in the pre-allocation passes `lower_unit` runs before
        // lowering proper starts -- so `def_ids` already has whatever `translate` needs, even
        // though nothing has been lowered into an arena yet at the point this runs.
        let lang_items = crate::langitems::translate(self.nameres.lang_items(), |node| {
            self.def_ids.get(&node).copied()
        });

        let parents = self.items.finish();

        // `Hir`'s fields are private to `crate::hir`, but `crate::hir::lower` and its
        // submodules are descendants of it, so the struct literal is accessible here even
        // though it isn't public.
        Hir {
            arenas,
            parents,
            root_module,
            lang_items,
        }
    }
}

/// Whether `path` is exactly the single-segment `Self` path the parser produces for the `Self`
/// keyword -- never true of a user-written path, since the lexer tokenizes the text `Self` as
/// the reserved `UpperSelfKw`, not as an `Identifier`, so no ordinary path segment can ever carry
/// that text.
fn is_self_path(path: &ast::Path) -> bool {
    match path.segments.as_slice() {
        [segment] => crate::ast::interner::Interner::resolve(segment.text) == "Self",
        _ => false,
    }
}
