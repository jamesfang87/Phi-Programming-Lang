//! The High-level Intermediate Representation (HIR) is the tree that lowering produces from the
//! AST. Later passes, such as name resolution and type checking, operate on the HIR rather than
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
mod path;
mod types;
pub mod visit;

pub use arena::{Arena, Node, OwnerNode};
pub use block::{Block, Stmt, StmtKind, WithLend};
pub use expr::{AccessArgs, Expr, ExprKind, LoopSource, Payload, PayloadField};
pub use ids::{DefId, HirId};
pub use items::{
    Closure, ClosureParam, Enum, Extend, Field, Function, Generic, Import, Module, Param,
    SelfParam, Struct, Trait, Variant, VariantPayload,
};
pub use pat::{Arm, BindingMode, Pat, PatKind};
pub use path::{Local, Path, Res, TyDef, Type};
pub use types::{Ty, TyKind};

#[derive(Debug)]
pub struct Hir {
    /// Maps each definition, indexed by its (global) [`DefId`], to the [`Arena`] holding its
    /// nodes.
    ///
    /// Every slot is filled once lowering has finished: a field, a variant, or a generic type
    /// parameter is addressed by a [`HirId`] within its owner's arena and never gets a `DefId`
    /// at all (see [`DefId`]), so no unused slots exist. This allows a plain `Vec<Arena>` rather
    /// than a `Vec<Option<Arena>>` -- lookups need no unwrap, and there is no unreachable
    /// "missing arena" case to handle.
    ///
    /// `LoweringCtx::finish` is what upholds it, ordering the arenas it collected by `DefId`
    /// rather than scattering them into placeholders.
    arenas: Vec<Arena>,

    /// Maps each definition, indexed by its (global) [`DefId`], to the definition it is
    /// lexically declared inside. That enclosing definition is an item's module, a method's
    /// `extend` block or trait, or a closure's enclosing owner.
    ///
    /// The root module is its own parent. This allows a plain `Vec<DefId>` rather than a
    /// `Vec<Option<DefId>>` and makes "has no parent" representable, since the root is the only
    /// definition that can reference itself. [`Hir::parent`] converts this to an `Option`, so a
    /// caller walking up the chain gets a `None` termination condition instead of looping.
    parents: Vec<DefId>,

    /// The root module, which transitively contains every other definition in the program.
    root_module: DefId,

    /// The core-library definitions the compiler itself knows by name -- the enums `?` and `for`
    /// desugar through, and the traits the operators dispatch to.
    ///
    /// Resolved at the AST level, before any `DefId` exists (see [`crate::langitems::collect_ast`]),
    /// and translated into this `DefId`-keyed form as the last step of lowering (see
    /// [`crate::langitems::translate`]) -- lowering has both a lang item's `NodeId` and
    /// the `DefId` it became, so this is where the mapping occurs. Stored on `Hir` itself
    /// rather than returned from `lower_unit` alongside it, because every later pass already
    /// has a `Hir` in hand and needs nothing else to resolve a lang item.
    lang_items: crate::langitems::LangItems,
}

impl Hir {
    /// Returns the [`Arena`] belonging to `def_id`.
    ///
    /// Panics if `def_id` has no arena, i.e. it does not name an owner or lowering has not
    /// finished yet.
    pub fn arena(&self, def_id: DefId) -> &Arena {
        &self.arenas[def_id.index()]
    }

    /// Looks up the [`Node`] a [`HirId`] addresses.
    ///
    /// The assertion verifies arena consistency: the node stored at a slot has the [`HirId`]
    /// that addresses that slot. This catches a node filled with an incorrect id.
    ///
    /// This does not catch child references into other arenas. Following such an id lands
    /// on a real node in that arena, which has the id that was followed, so the assertion
    /// would pass. That constraint is enforced instead during child storage via
    /// `ArenaBuilder::fill`.
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

    /// The core-library definitions the compiler knows by name.
    pub fn lang_items(&self) -> &crate::langitems::LangItems {
        &self.lang_items
    }

    /// Returns the definition that `def_id` is lexically declared inside, or `None` if `def_id`
    /// is the root module.
    ///
    /// Together with [`Hir::module_of`], this lets a pass retrieve its enclosing context from
    /// an id alone, without maintaining context state during traversal.
    pub fn parent(&self, def_id: DefId) -> Option<DefId> {
        let parent = self.parents[def_id.index()];
        // The root is stored as its own parent, which is how the table stays dense. Reporting
        // that as `None` is what gives a caller walking upwards a termination condition.
        (parent != def_id).then_some(parent)
    }

    /// Iterates every `DefId` that owns an arena, in the order they were allocated. Used by the
    /// `--debug` dump to traverse the program without needing its own traversal; see
    /// [`crate::driver::emit_debug`].
    pub fn def_ids(&self) -> impl Iterator<Item = DefId> + '_ {
        self.arenas
            .iter()
            .enumerate()
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

/// Generates typed node accessors on [`Hir`], one per child-bearing [`Node`] variant.
///
/// Every child reference in the HIR is a bare [`HirId`]. The type system provides no enforcement
/// that a particular id addresses the expected [`Node`] variant — only a comment like `// -> Node::Block`
/// beside the field indicates what kind is expected. Code that follows such an id must unwrap the
/// variant; untyped unwraps written by hand are both verbose and error-prone. For example, passing
/// a block id to a function expecting an expression id is a one-word mistake that compiles cleanly
/// but panics at runtime (this happened with `if`/`else`'s `else_block`, where the wrong traversal
/// was used for the block).
///
/// Generated typed accessors (`hir.block(id)` vs. `hir.expr(id)`) make the type expectation
/// explicit at the call site and report uniform diagnostic messages that name both the expected
/// variant and what was actually found. The bodies are generated once rather than written fourteen
/// times over, eliminating duplication and transposition errors.
///
/// [`Hir::node`] remains as an untyped escape hatch for code that legitimately needs to dispatch
/// on the variant dynamically, such as the `--debug` diagnostic dump.
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
