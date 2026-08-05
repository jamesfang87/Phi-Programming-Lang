//! This file defines [`Node`] and [`Arena`]. A [`Node`] is one node of the HIR tree. An [`Arena`]
//! is the storage for all the nodes owned by a single definition (a function, struct, closure,
//! and so on).
//!
//! Every owning definition gets its own [`Arena`]. Lowering places each of that definition's
//! child nodes into the arena and addresses it by [`LocalId`].

use crate::hir::HirId;
use crate::hir::block::{Block, Stmt};
use crate::hir::expr::Expr;
use crate::hir::items::{
    Closure, ClosureParam, Enum, Extend, Field, Function, Generic, Import, Module, Param,
    SelfParam, Struct, Trait, Variant,
};
use crate::hir::pat::{Arm, Pat};
use crate::hir::types::Ty;
use crate::lexer::src_span::SrcSpan;

/// One node of the HIR.
#[derive(Debug)]
pub enum Node {
    Owner(OwnerNode),
    Import(Import),
    Param(Param),
    ClosureParam(ClosureParam),
    SelfParam(SelfParam),
    Field(Field),
    Variant(Variant),
    Generic(Generic),
    Arm(Arm),
    Block(Block),
    Stmt(Stmt),
    Expr(Expr),
    Pat(Pat),
    Ty(Ty),
}

/// The subset of [`Node`] that owns an [`Arena`] of its own. A module, function, struct, enum,
/// trait, `extend` block, or closure can be an [`OwnerNode`]. Every arena's slot zero holds one
/// of these.
#[derive(Debug)]
pub enum OwnerNode {
    Module(Module),
    Function(Function),
    Struct(Struct),
    Enum(Enum),
    Trait(Trait),
    Extend(Extend),
    Closure(Closure),
}

/// Generates the accessors every node kind supports, from one list of variants.
///
/// Each of `kind_name`, `hir_id`, and `span` is otherwise a match arm per variant, and there
/// were four such matches spread over this file and the debug dump -- so adding a node kind
/// meant four edits, three of which the compiler could not remind you about until the fourth was
/// wrong. Listing the variants once here is what collapses that.
///
/// Variants come in two groups because [`Node::Owner`] wraps another enum rather than a node
/// struct: a `delegate` variant forwards to the same accessor on its payload, while a `field`
/// variant reads the `hir_id` and `span` that every node struct carries directly.
macro_rules! node_accessors {
    (
        $Enum:ident {
            $( delegate $Delegating:ident, )*
            $( field $Direct:ident, )*
        }
    ) => {
        impl $Enum {
            /// The name of this node's variant, such as `"Block"` or `"Expr"`.
            ///
            /// This exists for the panic messages of the typed accessors on
            /// [`Hir`](crate::hir::Hir), which report what they actually found when a child id
            /// turns out to address the wrong kind of node. A `Debug` of the node would say the
            /// same thing, but it would also print the node's entire subtree -- for a function
            /// body, the whole function -- which buries the one word that makes the mismatch
            /// diagnosable.
            pub fn kind_name(&self) -> &'static str {
                match self {
                    $( $Enum::$Delegating(n) => n.kind_name(), )*
                    $( $Enum::$Direct(_) => stringify!($Direct), )*
                }
            }

            /// The [`HirId`] this node stores for itself.
            ///
            /// Every node knows its own address, which is what lets
            /// [`Hir::node`](crate::hir::Hir::node) check that an id addresses the node it names
            /// without a per-node-kind walk.
            pub fn hir_id(&self) -> HirId {
                match self {
                    $( $Enum::$Delegating(n) => n.hir_id(), )*
                    $( $Enum::$Direct(n) => n.hir_id, )*
                }
            }

            /// The source text this node was built from.
            pub fn span(&self) -> SrcSpan {
                match self {
                    $( $Enum::$Delegating(n) => n.span(), )*
                    $( $Enum::$Direct(n) => n.span, )*
                }
            }
        }
    };
}

node_accessors!(Node {
    delegate Owner,
    field Import,
    field Param,
    field ClosureParam,
    field SelfParam,
    field Field,
    field Variant,
    field Generic,
    field Arm,
    field Block,
    field Stmt,
    field Expr,
    field Pat,
    field Ty,
});

node_accessors!(OwnerNode {
    field Module,
    field Function,
    field Struct,
    field Enum,
    field Trait,
    field Extend,
    field Closure,
});

impl From<OwnerNode> for Node {
    fn from(owner: OwnerNode) -> Self {
        Node::Owner(owner)
    }
}

/// Stores every node belonging to one owner as a single, densely packed `Vec<Node>`.
///
/// Looking up a node by its [`LocalId`] is a direct index into [`Arena::nodes`]. Index zero
/// always holds the owner itself.
#[derive(Debug)]
pub struct Arena {
    pub(crate) nodes: Vec<Node>,
}

impl Arena {
    /// Returns the node stored at `local_id`.
    pub fn get(&self, id: HirId) -> &Node {
        &self.nodes[id.local_id.index()]
    }

    /// Returns the [`OwnerNode`] every arena's slot zero holds.
    pub fn owner(&self) -> &OwnerNode {
        let Node::Owner(owner) = &self.nodes[0] else {
            unreachable!("slot 0 of an arena is always Node::Owner");
        };
        owner
    }
}
