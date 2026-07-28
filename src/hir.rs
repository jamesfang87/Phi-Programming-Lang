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

pub use crate::name_res::resolve_results::NameResolverResults;
pub use arena::{Arena, Node, OwnerNode};
pub use block::{Block, LetStmt, Stmt, StmtKind, WithLend};
pub use builder::{ArenaBuilder, DefIdAllocator};
pub use expr::{AccessArgs, Expr, ExprKind, LoopSource, Payload};
pub use ids::{DefId, HirId, LocalId};
pub use items::{
    Closure, ClosureParam, Enum, Extend, Field, Function, Generic, Import, Module, Param,
    SelfParam, Struct, Trait, Variant, VariantPayload,
};
pub use pat::{Arm, BindingMode, Pat, PatKind};
pub use types::{Ty, TyKind};

pub struct Hir {
    /// Maps each definition, indexed by its (global) [`DefId`], to the [`Arena`] holding its
    /// nodes.
    ///
    /// Some `DefId`s never get an arena. Fields, variants, and generic type parameters are
    /// addressed by a [`LocalId`] within their owner's arena and never become owners
    /// themselves, so their slot here stays `None`.
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
    pub fn node(&self, id: HirId) -> &Node {
        self.arena(id.owner).get(id.local_id)
    }

    /// Returns the [`OwnerNode`] a [`DefId`] names. Every `DefId` names an owner, so this never
    /// fails.
    pub fn owner(&self, id: DefId) -> &OwnerNode {
        self.arena(id).owner()
    }

    /// Returns the root module of the program.
    pub fn root(&self) -> &Module {
        let OwnerNode::Module(module) = self.owner(self.root_module) else {
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

    /// Walks up from `def_id` until it reaches a module, and returns that module. The result is
    /// either the module `def_id` is declared in, or `def_id` itself if it already names a
    /// module.
    pub fn module_of(&self, def_id: DefId) -> DefId {
        let mut current = def_id;
        loop {
            if matches!(self.owner(current), OwnerNode::Module(_)) {
                return current;
            }
            current = self
                .parent(current)
                .expect("every def is nested in the root module, which is a module");
        }
    }
}
