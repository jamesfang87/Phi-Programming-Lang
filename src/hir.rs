//! The High-level Intermediate Representation (HIR) is the tree that lowering produces from the
//! AST. Future passes, such as name resolution and type checking, operate on the HIR rather than
//! the AST.
//!
//! The HIR differs from the AST in two ways. First, it addresses nodes differently. Instead of
//! the AST's `Box`-linked tree, every definition (a function, struct, enum, trait, `extend`
//! block, closure, or module) gets a global [`DefId`] and owns an [`Arena`] of its own nodes,
//! each addressed by a [`LocalId`] local to that arena. A [`HirId`] is the `(DefId, LocalId)`
//! pair that names one node anywhere in the program. Second, lowering desugars surface forms
//! that the AST still keeps separate: `while`, `for`, and `while let` all become
//! [`ExprKind::Loop`], and `if let` becomes an [`ExprKind::Match`]. Later passes therefore only
//! need to handle one canonical form instead of several equivalent ones.
//!
//! [`Hir`] is the owner of every arena in the program: it maps each [`DefId`] to the arena
//! holding that definition's nodes, and records each definition's lexical parent so a pass can
//! recover surrounding context (its enclosing module, for instance) from an id alone.

mod arena;
mod block;
mod builder;
mod expr;
mod ids;
mod items;
pub mod lower;
mod pat;
mod types;

pub use crate::nameres::results::NameResolutions;
pub use arena::{Arena, Node, OwnerNode};
pub use block::{Block, Stmt, StmtKind, WithLend};
pub use expr::{AccessArgs, Expr, ExprKind, LoopSource, Payload, PayloadField};
pub use ids::{DefId, HirId, LocalId};
pub use items::{
    Closure, ClosureParam, Enum, Extend, Field, Function, Generic, Import, Module, Param,
    SelfParam, Struct, Trait, Variant, VariantPayload,
};
pub use pat::{Arm, BindingMode, Pat, PatKind};
pub use types::{Ty, TyKind};

#[derive(Debug)]
pub struct Hir {
    /// Maps each definition, indexed by its (global) [`DefId`], to the [`Arena`] holding its
    /// nodes.
    ///
    /// Every slot is filled once lowering has finished: a field, a variant, or a generic type
    /// parameter is addressed by a [`HirId`] within its owner's arena and never gets a `DefId`
    /// at all (see [`DefId`]), so it never claims a slot here to leave empty. The `Option` is
    /// scaffolding from construction only -- `LoweringCtx::finish` collects the arenas out of a
    /// `HashMap` into this dense `Vec`, and needs a placeholder to scatter them into.
    ///
    /// It should therefore be a plain `Vec<Arena>`. Making it one means changing how
    /// `LoweringCtx::finish` builds it, in `crate::hir::lower::ctx`.
    arenas: Vec<Option<Arena>>,

    /// Maps each definition, indexed by its (global) [`DefId`], to the definition it is
    /// lexically declared inside. That enclosing definition is an item's module, a method's
    /// `extend` block or trait, or a closure's enclosing owner.
    ///
    /// Only the root module has no parent, so `parents[root_module]` is the sole `None`.
    parents: Vec<Option<DefId>>,

    /// The root module, which transitively contains every other definition in the program.
    root_module: DefId,
}

impl Hir {
    /// Returns the [`Arena`] belonging to `def_id`.
    ///
    /// Panics if `def_id` has no arena, i.e. it does not name an owner or lowering has not
    /// finished yet.
    pub fn arena(&self, def_id: DefId) -> &Arena {
        self.arenas[def_id.index()]
            .as_ref()
            .expect("def has no owner arena (not an owner DefKind, or not yet lowered)")
    }

    /// Looks up the [`Node`] a [`HirId`] addresses.
    ///
    /// The assertion checks that an arena agrees with itself: the node stored at a slot is the
    /// one whose own [`HirId`] names that slot. It catches a node filled in under an id other
    /// than the one it was built with.
    ///
    /// It does *not* catch a child reference into a foreign arena. Following such an id lands
    /// on a real node in that other arena, which stores exactly the id that was followed, so
    /// nothing here disagrees. That invariant is enforced where a child id is stored instead,
    /// by `ArenaBuilder::fill`.
    pub fn node(&self, id: HirId) -> &Node {
        let node = self.arena(id.owner).get(id);
        debug_assert_eq!(
            node.hir_id(),
            id,
            "HirId does not address the node it names"
        );
        node
    }

    /// Returns the [`OwnerNode`] a [`DefId`] names. Every `DefId` names an owner, so this never
    /// fails.
    pub fn def(&self, id: DefId) -> &OwnerNode {
        self.arena(id).owner()
    }

    /// Returns the root module of the program.
    pub fn root(&self) -> &Module {
        let OwnerNode::Module(module) = self.def(self.root_module) else {
            unreachable!("root of a Module owner is always OwnerNode::Module");
        };

        module
    }

    /// Returns the [`DefId`] of the root module.
    pub fn root_id(&self) -> DefId {
        self.root_module
    }

    /// Returns the definition that `def_id` is lexically declared inside, or `None` if `def_id`
    /// names the root module.
    ///
    /// Together with [`Hir::module_of`], this lets a pass recover its surrounding context from
    /// an id alone, instead of threading that context through its own traversal.
    pub fn parent(&self, def_id: DefId) -> Option<DefId> {
        self.parents[def_id.index()]
    }

    /// Iterates every `DefId` that owns an arena, in the order they were allocated. Used by the
    /// `--debug` dump to walk the whole program without needing its own traversal; see
    /// [`crate::driver::emit_debug`].
    pub fn def_ids(&self) -> impl Iterator<Item = DefId> + '_ {
        self.arenas
            .iter()
            .enumerate()
            .filter(|(_, arena)| arena.is_some())
            .map(|(index, _)| DefId::from_usize(index))
    }

    /// Walks up from `def_id` until it reaches a module, and returns that module. The result is
    /// either the module `def_id` is declared in, or `def_id` itself if it already names a
    /// module.
    pub fn module_of(&self, def_id: DefId) -> DefId {
        let mut current = def_id;
        loop {
            if matches!(self.def(current), OwnerNode::Module(_)) {
                return current;
            }
            current = self
                .parent(current)
                .expect("every def is nested in the root module, which is a module");
        }
    }
}

/// Generates the typed node accessors on [`Hir`], one per child-bearing [`Node`] variant.
///
/// Every child reference in the HIR is a bare [`HirId`], so nothing in the type system says which
/// [`Node`] variant one addresses -- only the `// -> Node::Block` comment beside the field does.
/// A pass that follows such an id therefore has to unwrap the variant itself, and the natural way
/// to write that is a `let ... else { unreachable!(..) }` naming the kind the author *believed*
/// was there. Written out by hand at every child reference, that is both noise and a place to be
/// wrong: passing a block id to a function expecting an expression is a one-word mistake that
/// compiles cleanly and panics at run time (it did, for `if`/`else`'s `else_block`).
///
/// Routing every such unwrap through one generated accessor per kind makes the mistake obvious at
/// the call site -- `hir.block(id)` versus `hir.expr(id)` reads as a claim about the id -- and
/// gives every failure the same message, naming both the kind expected and the kind found. The
/// bodies are identical apart from two names, so they are generated rather than repeated
/// fourteen times over.
///
/// [`Hir::node`] stays the untyped escape hatch for code that genuinely has to dispatch on the
/// variant, such as the `--debug` dump.
macro_rules! node_accessors {
    ($($method:ident => $variant:ident -> $node_ty:ty),* $(,)?) => {
        impl Hir {
            $(
                #[doc = concat!(
                    "Looks up the [`", stringify!($node_ty), "`] that `id` addresses.\n\n",
                    "Panics if `id` addresses any node other than a `Node::",
                    stringify!($variant), "`, naming what it found instead."
                )]
                pub fn $method(&self, id: HirId) -> &$node_ty {
                    match self.node(id) {
                        Node::$variant(node) => node,
                        other => panic!(
                            "expected {id:?} to name a Node::{}, found a Node::{}",
                            stringify!($variant),
                            other.kind_name(),
                        ),
                    }
                }
            )*
        }
    };
}

node_accessors! {
    import => Import -> Import,
    param => Param -> Param,
    closure_param => ClosureParam -> ClosureParam,
    self_param => SelfParam -> SelfParam,
    field => Field -> Field,
    variant => Variant -> Variant,
    generic => Generic -> Generic,
    arm => Arm -> Arm,
    block => Block -> Block,
    stmt => Stmt -> Stmt,
    expr => Expr -> Expr,
    pat => Pat -> Pat,
    ty => Ty -> Ty,
}
