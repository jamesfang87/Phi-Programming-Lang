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

impl Node {
    /// The [`HirId`] this node stores for itself.
    ///
    /// Every node knows its own address, which is what lets [`Hir::node`](crate::hir::Hir::node)
    /// check that an id addresses the node it names without a per-node-kind walk.
    pub fn hir_id(&self) -> HirId {
        match self {
            Node::Owner(owner) => owner.hir_id(),
            Node::Import(n) => n.hir_id,
            Node::Param(n) => n.hir_id,
            Node::ClosureParam(n) => n.hir_id,
            Node::SelfParam(n) => n.hir_id,
            Node::Field(n) => n.hir_id,
            Node::Variant(n) => n.hir_id,
            Node::Generic(n) => n.hir_id,
            Node::Arm(n) => n.hir_id,
            Node::Block(n) => n.hir_id,
            Node::Stmt(n) => n.hir_id,
            Node::Expr(n) => n.hir_id,
            Node::Pat(n) => n.hir_id,
            Node::Ty(n) => n.hir_id,
        }
    }
}

impl OwnerNode {
    /// The [`HirId`] this owner stores for itself, which always addresses slot zero of its own
    /// arena.
    pub fn hir_id(&self) -> HirId {
        match self {
            OwnerNode::Module(n) => n.hir_id,
            OwnerNode::Function(n) => n.hir_id,
            OwnerNode::Struct(n) => n.hir_id,
            OwnerNode::Enum(n) => n.hir_id,
            OwnerNode::Trait(n) => n.hir_id,
            OwnerNode::Extend(n) => n.hir_id,
            OwnerNode::Closure(n) => n.hir_id,
        }
    }
}

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
