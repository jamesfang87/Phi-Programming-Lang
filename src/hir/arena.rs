use crate::driver::source::SrcSpan;
use crate::hir::HirId;
use crate::hir::block::{Block, Stmt};
use crate::hir::expr::Expr;
use crate::hir::items::{
    Closure, ClosureParam, Enum, Extend, Field, Function, Generic, Import, Module, Param,
    SelfParam, Struct, Trait, Variant,
};
use crate::hir::pat::{Arm, Pat};
use crate::hir::types::Ty;

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

/// Generates the shared metadata methods every node enum supports, dispatching each one across
/// the enum's own variants.
macro_rules! node_dispatch {
    (
        $Enum:ident {
            $( delegate $Delegating:ident, )*
            $( field $Direct:ident, )*
        }
    ) => {
        impl $Enum {
            /// The name of this node's variant, such as `"Block"` or `"Expr"`.
            pub fn kind_name(&self) -> &'static str {
                match self {
                    $( $Enum::$Delegating(n) => n.kind_name(), )*
                    $( $Enum::$Direct(_) => stringify!($Direct), )*
                }
            }

            /// The [`HirId`] this node stores for itself.
            pub fn hir_id(&self) -> HirId {
                match self {
                    $( $Enum::$Delegating(n) => n.hir_id(), )*
                    $( $Enum::$Direct(n) => n.hir_id.into(), )*
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

node_dispatch!(Node {
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

node_dispatch!(OwnerNode {
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

#[cfg(test)]
mod tests {
    use crate::testing::lower_to_hir;

    #[test]
    fn a_node_reports_its_own_kind() {
        let hir = lower_to_hir(
            "struct Foo {}
             fun f(x: i32) {}",
        );
        let owner = hir.root().items[0];
        let function = hir.root().items[1];
        let param = hir.function(function).params[0];

        assert_eq!(hir.def(owner).kind_name(), "Struct");
        assert_eq!(hir.node(owner.owner_id()).kind_name(), "Struct");
        assert_eq!(hir.node(param).kind_name(), "Param");
    }
}
