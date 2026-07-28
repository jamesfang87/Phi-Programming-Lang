//! [`OwnerLowerer`]: lowers one owner's AST subtree into its arena, plus the generic
//! reserve/build/fill helpers every node kind is lowered through.

use crate::hir::builder::ArenaBuilder;
use crate::hir::ids::{DefId, LocalId};
use crate::hir::lower::ctx::LoweringCtx;
use crate::hir::{Arm, Block, Expr, HirId, Node, Pat, PatKind, Stmt, StmtKind, Ty, TyKind};
use crate::lexer::src_span::SrcSpan;

/// Lowers one owner's AST subtree into that owner's arena.
///
/// `cx` stays reachable throughout so that a nested owner discovered mid-lowering (a closure)
/// can allocate its own `DefId` and get its own `OwnerLowerer` without unwinding back to
/// [`LoweringCtx`] first.
pub(super) struct OwnerLowerer<'a> {
    pub(super) cx: &'a mut LoweringCtx,
    builder: ArenaBuilder,
}

impl<'a> OwnerLowerer<'a> {
    pub(super) fn new(cx: &'a mut LoweringCtx, item_id: DefId) -> Self {
        OwnerLowerer {
            cx,
            builder: ArenaBuilder::new(item_id),
        }
    }

    pub(super) fn hir_id(&self, id: LocalId) -> HirId {
        self.builder.hir_id(id)
    }

    /// The owner whose arena is being built -- the parent of any nested owner (a closure)
    /// discovered while lowering it.
    pub(super) fn def_id(&self) -> DefId {
        self.builder.def_id()
    }

    pub(super) fn reserve(&mut self) -> LocalId {
        self.builder.reserve()
    }

    /// Reserves the owner's own root -- always the first reservation, which is what guarantees
    /// it lands at `LocalId::OWNER`.
    pub(super) fn reserve_root(&mut self) -> LocalId {
        self.builder.reserve()
    }

    pub(super) fn fill(&mut self, id: LocalId, node: impl Into<Node>) {
        self.builder.fill(id, node);
    }

    /// Finishes the owner's arena, registers it in [`LoweringCtx::owners`], and returns the
    /// owner's `DefId` so the caller can record it wherever it's declared (a module's item list,
    /// a trait's function list, and so on).
    pub(super) fn finish(self) -> DefId {
        let item_id = self.builder.def_id();
        let arena = self.builder.finish();
        self.cx.owners.insert(item_id, arena);
        item_id
    }

    // The helpers below all follow the same shape: reserve an id, let the passed-in closure
    // lower any children against that id, then fill in the finished node.

    pub(super) fn synth_expr(
        &mut self,
        span: SrcSpan,
        build: impl FnOnce(&mut Self, LocalId) -> crate::hir::ExprKind,
    ) -> LocalId {
        let id = self.reserve();
        let kind = build(self, id);
        let hir_id = self.hir_id(id);
        self.fill(id, Node::Expr(Expr { hir_id, kind, span }));
        id
    }

    pub(super) fn synth_stmt(
        &mut self,
        span: SrcSpan,
        build: impl FnOnce(&mut Self, LocalId) -> StmtKind,
    ) -> LocalId {
        let id = self.reserve();
        let kind = build(self, id);
        let hir_id = self.hir_id(id);
        self.fill(id, Node::Stmt(Stmt { hir_id, kind, span }));
        id
    }

    pub(super) fn synth_pat(
        &mut self,
        span: SrcSpan,
        build: impl FnOnce(&mut Self, LocalId) -> PatKind,
    ) -> LocalId {
        let id = self.reserve();
        let kind = build(self, id);
        let hir_id = self.hir_id(id);
        self.fill(id, Node::Pat(Pat { hir_id, kind, span }));
        id
    }

    pub(super) fn synth_block(
        &mut self,
        span: SrcSpan,
        build: impl FnOnce(&mut Self, LocalId) -> (Vec<LocalId>, Option<LocalId>),
    ) -> LocalId {
        let id = self.reserve();
        let (stmts, expr) = build(self, id);
        let hir_id = self.hir_id(id);
        self.fill(
            id,
            Node::Block(Block {
                hir_id,
                stmts,
                expr,
                span,
            }),
        );
        id
    }

    pub(super) fn synth_arm(
        &mut self,
        span: SrcSpan,
        build: impl FnOnce(&mut Self, LocalId) -> (LocalId, LocalId),
    ) -> LocalId {
        let id = self.reserve();
        let (pat, body) = build(self, id);
        let hir_id = self.hir_id(id);
        self.fill(
            id,
            Node::Arm(Arm {
                hir_id,
                pat,
                body,
                span,
            }),
        );
        id
    }

    pub(super) fn synth_ty(
        &mut self,
        span: SrcSpan,
        build: impl FnOnce(&mut Self, LocalId) -> TyKind,
    ) -> LocalId {
        let id = self.reserve();
        let kind = build(self, id);
        let hir_id = self.hir_id(id);
        self.fill(id, Node::Ty(Ty { hir_id, kind, span }));
        id
    }
}
