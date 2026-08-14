//! [`OwnerLowerer`]: lowers one owner's AST subtree into its arena, plus the generic
//! reserve/build/fill helpers every node kind is lowered through.

use crate::ast;
use crate::driver::source::SrcSpan;
use crate::hir::builder::ArenaBuilder;
use crate::hir::ids::DefId;
use crate::hir::lower::ctx::LoweringCtx;
use crate::hir::{
    Arm, Block, Expr, Generic, HirId, Node, Pat, PatKind, Stmt, StmtKind, Ty, TyKind,
};

/// Lowers one owner's AST subtree into that owner's arena.
///
/// `cx` stays reachable throughout so that a nested owner discovered mid-lowering (a closure)
/// can allocate its own `DefId` and get its own `OwnerLowerer` without unwinding back to
/// [`LoweringCtx`] first. `'res` is [`LoweringCtx`]'s own lifetime, the borrowed
/// [`NameResolutions`](crate::nameres::NameResolutions) every `Path` is built against.
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

    /// Starts building `item_id`'s arena just far enough to lower its own generics: reserves the
    /// root (so it still lands at `LocalId::OWNER`, exactly as [`OwnerLowerer::new`] plus
    /// `reserve_root` would), then lowers `generics` into the same builder.
    ///
    /// This is what lets a trait or `extend` block lower its own generics *before* its nested
    /// functions/methods, which need those generics to already have `HirId`s so a reference to
    /// one resolves (see [`LoweringCtx::node_to_hir`]), while its functions/methods still need
    /// `&mut LoweringCtx` (via [`LoweringCtx::lower_function`]) to lower in between. This function
    /// takes `cx` too, to record each generic's `HirId` and resolve its bounds, but -- unlike
    /// [`OwnerLowerer::new`] -- doesn't hold onto it past its own return, so the two needs don't
    /// conflict; see the note on that parameter.
    ///
    /// Pass the returned `ArenaBuilder` to [`OwnerLowerer::resume`] once the nested owners in
    /// between have been lowered, to fill in the root node and finish the arena.
    ///
    /// Takes `cx` only for the duration of this call -- unlike [`OwnerLowerer::new`], nothing
    /// here holds onto it afterward -- to record each generic's `NodeId -> HirId` mapping (so a
    /// reference to it from a function/method lowered in between can be translated, see
    /// [`LoweringCtx::node_to_hir`]) and to resolve each bound's path. That borrow ends when this
    /// function returns, well before the caller's own `&mut self` is needed again to lower those
    /// functions/methods, so it doesn't reintroduce the conflict this two-phase split exists to
    /// avoid.
    pub(super) fn begin_with_generics(
        cx: &mut LoweringCtx<'res>,
        item_id: DefId,
        generics: &[ast::Generic],
    ) -> (ArenaBuilder, HirId, Vec<HirId>) {
        let mut builder = ArenaBuilder::new(item_id);
        let root = builder.reserve();
        let generic_ids = generics
            .iter()
            .map(|g| {
                let hir_id = builder.reserve();
                cx.record_hir_id(g.id, hir_id);
                let bounds = g
                    .bounds
                    .as_deref()
                    .unwrap_or(&[])
                    .iter()
                    .map(|bound| cx.lower_path(g.id, bound))
                    .collect();
                builder.fill(
                    hir_id,
                    Node::Generic(Generic {
                        hir_id,
                        name: g.name,
                        bounds,
                        span: g.span,
                    }),
                );
                hir_id
            })
            .collect();
        (builder, root, generic_ids)
    }

    /// Reattaches a builder set down by [`OwnerLowerer::begin_with_generics`] to `cx`, once
    /// whatever needed lowering in between (a trait's functions, an `extend` block's methods) is
    /// done, so the rest of the owner's node can be filled in and the arena finished.
    pub(super) fn resume(cx: &'a mut LoweringCtx<'res>, builder: ArenaBuilder) -> Self {
        OwnerLowerer { cx, builder }
    }

    /// The owner whose arena is being built -- the parent of any nested owner (a closure)
    /// discovered while lowering it.
    pub(super) fn def_id(&self) -> DefId {
        self.builder.def_id()
    }

    pub(super) fn reserve(&mut self) -> HirId {
        self.builder.reserve()
    }

    /// Reserves the owner's own root -- always the first reservation, which is what guarantees
    /// it lands at `LocalId::OWNER`.
    pub(super) fn reserve_root(&mut self) -> HirId {
        self.builder.reserve()
    }

    pub(super) fn fill(&mut self, id: HirId, node: impl Into<Node>) {
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
