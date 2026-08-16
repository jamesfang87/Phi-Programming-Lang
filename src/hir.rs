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
    arenas: Vec<Arena>,
    parent_of: Vec<DefId>,
    root_module: DefId,
    lang_items: crate::langitems::hir::LangItems,
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
    pub fn lang_items(&self) -> &crate::langitems::hir::LangItems {
        &self.lang_items
    }

    /// Returns the definition that `def_id` is lexically declared inside, or `None` if `def_id`
    /// is the root module.
    pub fn parent(&self, def_id: DefId) -> Option<DefId> {
        let parent = self.parent_of[def_id.index()];
        // The root is stored as its own parent, which is how the table stays dense. Reporting
        // that as `None` is what gives a caller walking upwards a termination condition.
        (parent != def_id).then_some(parent)
    }

    /// Iterates every `DefId` that owns an arena in the order they were allocated.
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

/// Generates typed lookup methods on [`Hir`] that retrieve a [`Node`] by [`HirId`] and downcast
/// it to one expected variant.
///
/// Generated lookup methods (`hir.block(id)` vs. `hir.expr(id)`) make the type expectation
/// explicit at the call site and report uniform diagnostic messages that name both the expected
/// variant and what was actually found.
macro_rules! typed_node_lookup {
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

typed_node_lookup! {
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

/// Generates typed lookup methods on [`Hir`] that retrieve an [`OwnerNode`] by [`DefId`] and
/// downcast it to one expected variant.
macro_rules! typed_owner_lookup {
    ($($method:ident => $variant:ident -> $owner_ty:ty),* $(,)?) => {
        impl Hir {
            $(
                pub fn $method(&self, id: DefId) -> &$owner_ty {
                    match self.def(id) {
                        OwnerNode::$variant(owner) => owner,
                        other => panic!(
                            "expected {id:?} to name an OwnerNode::{}, found an OwnerNode::{}",
                            stringify!($variant),
                            other.kind_name(),
                        ),
                    }
                }
            )*
        }
    };
}

typed_owner_lookup! {
    module => Module -> Module,
    function => Function -> Function,
    struct_ => Struct -> Struct,
    enum_ => Enum -> Enum,
    trait_ => Trait -> Trait,
    extend => Extend -> Extend,
    closure => Closure -> Closure,
}
