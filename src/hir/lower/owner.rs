use crate::driver::source::SrcSpan;
use crate::hir::builder::ArenaBuilder;
use crate::hir::ids::DefId;
use crate::hir::lower::ctx::LoweringCtx;
use crate::hir::{Arm, Block, Expr, HirId, Node, Pat, PatKind, Stmt, StmtKind, Ty, TyKind};

/// Lowers one owner's AST subtree into that owner's arena.
pub(super) struct OwnerLowerer<'a, 'res> {
    pub(super) cx: &'a mut LoweringCtx<'res>,
    builder: ArenaBuilder,
}

impl<'a, 'res> OwnerLowerer<'a, 'res> {
    pub(super) fn new(cx: &'a mut LoweringCtx<'res>, item_id: DefId) -> Self {
        OwnerLowerer {
            cx,
            builder: ArenaBuilder::new(item_id),
        }
    }

    pub(super) fn def_id(&self) -> DefId {
        self.builder.def_id()
    }

    pub(super) fn reserve(&mut self) -> HirId {
        self.builder.reserve()
    }

    /// Reserves the owner's own root. This is must always be the first
    /// reservation to guarantee that the owner of the arena is located at
    /// `LocalId::OWNER`.
    pub(super) fn reserve_root(&mut self) -> HirId {
        self.builder.reserve()
    }

    pub(super) fn fill(&mut self, id: HirId, node: impl Into<Node>) {
        self.builder.fill(id, node);
    }

    pub(super) fn finish(self) -> DefId {
        let item_id = self.builder.def_id();
        let arena = self.builder.finish();
        self.cx.arenas.insert(item_id, arena);
        item_id
    }

    pub(super) fn synth_expr(
        &mut self,
        span: SrcSpan,
        build: impl FnOnce(&mut Self, HirId) -> crate::hir::ExprKind,
    ) -> HirId {
        let hir_id = self.reserve();
        let kind = build(self, hir_id);
        self.fill(hir_id, Node::Expr(Expr { hir_id, kind, span }));
        hir_id
    }

    pub(super) fn synth_stmt(
        &mut self,
        span: SrcSpan,
        build: impl FnOnce(&mut Self, HirId) -> StmtKind,
    ) -> HirId {
        let hir_id = self.reserve();
        let kind = build(self, hir_id);
        self.fill(hir_id, Node::Stmt(Stmt { hir_id, kind, span }));
        hir_id
    }

    pub(super) fn synth_pat(
        &mut self,
        span: SrcSpan,
        build: impl FnOnce(&mut Self, HirId) -> PatKind,
    ) -> HirId {
        let hir_id = self.reserve();
        let kind = build(self, hir_id);
        self.fill(hir_id, Node::Pat(Pat { hir_id, kind, span }));
        hir_id
    }

    pub(super) fn synth_block(
        &mut self,
        span: SrcSpan,
        build: impl FnOnce(&mut Self, HirId) -> (Vec<HirId>, Option<HirId>),
    ) -> HirId {
        let hir_id = self.reserve();
        let (stmts, expr) = build(self, hir_id);
        self.fill(
            hir_id,
            Node::Block(Block {
                hir_id,
                stmts,
                expr,
                span,
            }),
        );
        hir_id
    }

    pub(super) fn synth_arm(
        &mut self,
        span: SrcSpan,
        build: impl FnOnce(&mut Self, HirId) -> (HirId, Option<HirId>, HirId),
    ) -> HirId {
        let hir_id = self.reserve();
        let (pat, guard, block) = build(self, hir_id);
        self.fill(
            hir_id,
            Node::Arm(Arm {
                hir_id,
                pat,
                guard,
                block,
                span,
            }),
        );
        hir_id
    }

    pub(super) fn synth_ty(
        &mut self,
        span: SrcSpan,
        build: impl FnOnce(&mut Self, HirId) -> TyKind,
    ) -> HirId {
        let hir_id = self.reserve();
        let kind = build(self, hir_id);
        self.fill(hir_id, Node::Ty(Ty { hir_id, kind, span }));
        hir_id
    }
}
