//! This file defines [`Node`] and [`Arena`]. A [`Node`] is one node of the HIR tree. An [`Arena`]
//! is the storage for all the nodes owned by a single definition (a function, struct, closure,
//! and so on).
//!
//! Every owning definition gets its own [`Arena`]. Lowering places each of that definition's
//! child nodes into the arena and addresses it by [`LocalId`].

use crate::hir::block::{Block, Stmt};
use crate::hir::expr::Expr;
use crate::hir::ids::LocalId;
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
    pub fn get(&self, local_id: LocalId) -> &Node {
        &self.nodes[local_id.index()]
    }

    /// Returns the owner node at slot zero, still wrapped as a [`Node`]. Use [`Arena::owner`]
    /// instead if the unwrapped [`OwnerNode`] is what's needed.
    pub fn get_owner(&self) -> &Node {
        &self.nodes[0]
    }

    /// Returns the [`OwnerNode`] every arena's slot zero holds.
    pub fn owner(&self) -> &OwnerNode {
        let Node::Owner(owner) = &self.nodes[0] else {
            unreachable!("slot 0 of an arena is always Node::Owner");
        };
        owner
    }
}
