use crate::driver::source::SrcSpan;
use crate::hir::builder::ArenaBuilder;
use crate::hir::ids::{ArmId, BlockId, DefId, ExprId, HirId, PatId, StmtId, TyId};
use crate::hir::lower::ctx::LoweringCtx;
use crate::hir::{Arm, Block, Expr, Node, Pat, PatKind, Stmt, StmtKind, Ty, TyKind};

/// Lowers one owner's AST subtree into that owner's arena.
pub(super) struct OwnerLowerer<'a, 'res> {
    pub(super) cx: &'a mut LoweringCtx<'res>,
    builder: ArenaBuilder,
    root: HirId,
}

impl<'a, 'res> OwnerLowerer<'a, 'res> {
    pub(super) fn new(cx: &'a mut LoweringCtx<'res>, item_id: DefId) -> Self {
        let mut builder = ArenaBuilder::new(item_id);
        let root = builder.reserve();
        OwnerLowerer { cx, builder, root }
    }

    pub(super) fn def_id(&self) -> DefId {
        self.builder.def_id()
    }

    pub(super) fn root(&self) -> HirId {
        self.root
    }

    pub(super) fn reserve(&mut self) -> HirId {
        self.builder.reserve()
    }

    pub(super) fn fill(&mut self, id: HirId, node: impl Into<Node>) {
        self.builder.fill(id, node);
    }

    pub(super) fn finish(self) -> DefId {
        let item_id = self.builder.def_id();
        let arena = self.builder.finish();
        let index = item_id.index();
        if self.cx.arenas.len() <= index {
            self.cx.arenas.resize_with(index + 1, || None);
        }
        self.cx.arenas[index] = Some(arena);
        item_id
    }

    pub(super) fn synth_expr(
        &mut self,
        span: SrcSpan,
        build: impl FnOnce(&mut Self, HirId) -> crate::hir::ExprKind,
    ) -> ExprId {
        let hir_id = self.reserve();
        let kind = build(self, hir_id);
        self.fill(
            hir_id,
            Node::Expr(Expr {
                hir_id: hir_id.into(),
                kind,
                span,
            }),
        );
        hir_id.into()
    }

    pub(super) fn synth_stmt(
        &mut self,
        span: SrcSpan,
        build: impl FnOnce(&mut Self, HirId) -> StmtKind,
    ) -> StmtId {
        let hir_id = self.reserve();
        let kind = build(self, hir_id);
        self.fill(
            hir_id,
            Node::Stmt(Stmt {
                hir_id: hir_id.into(),
                kind,
                span,
            }),
        );
        hir_id.into()
    }

    pub(super) fn synth_pat(
        &mut self,
        span: SrcSpan,
        build: impl FnOnce(&mut Self, HirId) -> PatKind,
    ) -> PatId {
        let hir_id = self.reserve();
        let kind = build(self, hir_id);
        self.fill(
            hir_id,
            Node::Pat(Pat {
                hir_id: hir_id.into(),
                kind,
                span,
            }),
        );
        hir_id.into()
    }

    pub(super) fn synth_block(
        &mut self,
        span: SrcSpan,
        build: impl FnOnce(&mut Self, HirId) -> (Vec<StmtId>, Option<ExprId>),
    ) -> BlockId {
        let hir_id = self.reserve();
        let (stmts, expr) = build(self, hir_id);
        self.fill(
            hir_id,
            Node::Block(Block {
                hir_id: hir_id.into(),
                stmts,
                expr,
                span,
            }),
        );
        hir_id.into()
    }

    pub(super) fn synth_arm(
        &mut self,
        span: SrcSpan,
        build: impl FnOnce(&mut Self, HirId) -> (PatId, Option<ExprId>, BlockId),
    ) -> ArmId {
        let hir_id = self.reserve();
        let (pat, guard, block) = build(self, hir_id);
        self.fill(
            hir_id,
            Node::Arm(Arm {
                hir_id: hir_id.into(),
                pat,
                guard,
                block,
                span,
            }),
        );
        hir_id.into()
    }

    pub(super) fn synth_ty(
        &mut self,
        span: SrcSpan,
        build: impl FnOnce(&mut Self, HirId) -> TyKind,
    ) -> TyId {
        let hir_id = self.reserve();
        let kind = build(self, hir_id);
        self.fill(
            hir_id,
            Node::Ty(Ty {
                hir_id: hir_id.into(),
                kind,
                span,
            }),
        );
        hir_id.into()
    }
}
