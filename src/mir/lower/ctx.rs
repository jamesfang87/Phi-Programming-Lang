//! [`BodyLowerCtx`], the per-`Body` builder every corner of `mir::lower` lowers into. It plays
//! the role [`OwnerLowerer`](crate::hir::lower) plays for Lowering #1: one context owns exactly
//! one `Body`-in-progress, and the rest of `mir::lower`'s submodules are `impl` blocks on it.
//!
//! Unlike an HIR arena, built by `reserve`-then-`fill` in tree order, a `Body`'s basic blocks are
//! built in control-flow order: [`BodyLowerCtx::new_block`] reserves an empty block with no
//! terminator yet, [`BodyLowerCtx::switch_to`] moves the "current block" cursor onto one, and
//! [`BodyLowerCtx::push_stmt`]/[`BodyLowerCtx::set_terminator`] append to whichever block the
//! cursor currently names. [`BodyLowerCtx::finish`] panics if any reserved block was never given
//! a terminator, the same "this is a lowering-pass bug, not a user error" discipline
//! `hir::lower::ctx`'s `def_id_of`/`hir_id_of` already use.

use std::collections::HashMap;

use crate::ast::{Ident, Mutability};
use crate::driver::cli::Mode;
use crate::driver::source::SrcSpan;
use crate::hir::{DefId, Hir, HirId};
use crate::mir::lower::Task;
use crate::mir::{
    AnyMode, BasicBlock, BasicBlockData, Body, Local, LocalDecl, Place, Statement, StatementKind,
    Terminator, TerminatorKind,
};
use crate::typeck::results::TypeResolutions;
use crate::typeck::ty::Ty;
use crate::typeck::tyctx::TyCtx;

/// One entry of a block's exit obligations: something that has to run at every point control
/// leaves that block, regardless of which path got it there. See the spec's `with`-lend
/// `StorageDead` note and this pass's block-scoped `defer`.
#[derive(Clone, Copy, Debug)]
pub(super) enum ExitObligation {
    StorageDead(Local),
    /// Runs the deferred expression `HirId` for its side effects, discarding its value.
    RunDeferred(HirId),
}

/// `break_target`/`continue_target` are which block `break`/`continue` jump to; `scope_depth` is
/// how many block scopes (see [`BodyLowerCtx::block_scopes`]) were open when the loop was
/// entered, which is where a `break`/`continue` reached from inside it stops replaying exit
/// obligations -- it leaves every block the loop's own body opened, and no block outside the loop.
struct LoopCtx {
    break_target: BasicBlock,
    continue_target: BasicBlock,
    scope_depth: usize,
}

/// One block under construction: statements accumulate directly, but the terminator stays
/// unset until the code lowering into this block reaches its own end, which is why it is an
/// `Option` here and a plain field on the finished [`BasicBlockData`].
struct BlockBuilder {
    statements: Vec<Statement>,
    terminator: Option<Terminator>,
}

pub(crate) struct BodyLowerCtx<'a> {
    pub(crate) hir: &'a Hir,
    pub(crate) tcx: &'a mut TyCtx,
    pub(crate) types: &'a TypeResolutions,
    pub(crate) mode: Mode,
    pub(crate) def_id: DefId,
    /// The `any`-mode this task is specialized to, `None` for an `Ordinary` one. Every type
    /// this context reads through [`BodyLowerCtx::expr_ty`]/[`BodyLowerCtx::pat_ty`] is resolved
    /// through this uniformly, so a read of an `any`-typed parameter inside a specialized body
    /// sees the same concrete type the parameter's own `LocalDecl` was declared with, not the
    /// unspecialized `Any(T)` typeck recorded for it.
    pub(crate) any_mode: Option<AnyMode>,

    local_decls: Vec<LocalDecl>,
    blocks: Vec<BlockBuilder>,
    current: BasicBlock,

    /// Maps a HIR node that names one value slot -- a parameter, a `let`/`with` binding's
    /// pattern, a closure's implicit environment -- to the `Place` lowering allocated for it.
    /// Almost always a bare local with no projection; a captured variable inside a closure's own
    /// body is the one exception, addressed through the closure's environment local instead
    /// (`mir::lower::closure`). `ExprKind::Path`'s `Res::Local` lowering reads this to turn a
    /// HIR-level name back into a `Place`.
    hir_locals: HashMap<HirId, Place>,

    loop_stack: Vec<LoopCtx>,

    /// A stack of currently-open blocks' own exit obligations, pushed by
    /// [`BodyLowerCtx::push_block_scope`] on entering a HIR block and popped by
    /// [`BodyLowerCtx::pop_block_scope`] on leaving it. An early exit (`break`, `continue`,
    /// `return`) reached from inside one or more of these does not pop them -- it only replays
    /// a copy of what is currently on the stack, since the blocks are still lexically open for
    /// whatever in the same block follows the early exit, or for the next loop iteration.
    block_scopes: Vec<Vec<ExitObligation>>,

    /// Closures and `any`-mode-specialized callees discovered while lowering this body, merged
    /// into the driver's worklist once this body finishes. See `mir::lower`'s module docs.
    pub(crate) discovered: Vec<Task>,
}

impl<'a> BodyLowerCtx<'a> {
    pub(crate) fn new(
        hir: &'a Hir,
        tcx: &'a mut TyCtx,
        types: &'a TypeResolutions,
        mode: Mode,
        def_id: DefId,
        any_mode: Option<AnyMode>,
    ) -> Self {
        let mut ctx = BodyLowerCtx {
            hir,
            tcx,
            types,
            mode,
            def_id,
            any_mode,
            local_decls: Vec::new(),
            blocks: Vec::new(),
            current: BasicBlock::from_usize(0),
            hir_locals: HashMap::new(),
            loop_stack: Vec::new(),
            block_scopes: Vec::new(),
            discovered: Vec::new(),
        };
        let entry = ctx.new_block();
        ctx.current = entry;
        ctx
    }

    // -----------------------------------------------------------------
    // Locals
    // -----------------------------------------------------------------

    pub(crate) fn new_local(
        &mut self,
        ty: Ty,
        mutability: Mutability,
        name: Option<Ident>,
        span: SrcSpan,
    ) -> Local {
        let local = Local::from_usize(self.local_decls.len());
        self.local_decls.push(LocalDecl {
            ty,
            mutability,
            name,
            span,
        });
        local
    }

    /// Allocates a new local with no source name, for a value lowering itself introduces (a
    /// flattened sub-expression, a bounds check's length, and so on). Always immutable: nothing
    /// after lowering ever assigns into a temporary a second time in a way mutability would
    /// guard against.
    pub(crate) fn new_temp(&mut self, ty: Ty, span: SrcSpan) -> Local {
        self.new_local(ty, Mutability::Immutable, None, span)
    }

    /// Records that HIR node `id` (a parameter, a binding pattern) is addressed by `local`,
    /// with no projection, from here on.
    pub(crate) fn bind_local(&mut self, id: HirId, local: Local) {
        self.hir_locals.insert(id, Place::from_local(local));
    }

    /// Records that HIR node `id` is addressed by `place` from here on -- the general form
    /// [`BodyLowerCtx::bind_local`] is sugar for, used directly for a closure's captured
    /// variable, which projects into the environment local instead of naming a local of its own.
    pub(crate) fn bind_place(&mut self, id: HirId, place: Place) {
        self.hir_locals.insert(id, place);
    }

    /// The `Place` bound to HIR node `id` by an earlier [`BodyLowerCtx::bind_local`]/
    /// [`BodyLowerCtx::bind_place`].
    pub(crate) fn place_for(&self, id: HirId) -> Place {
        self.hir_locals
            .get(&id)
            .unwrap_or_else(|| panic!("mir::lower: no place bound for {id:?}"))
            .clone()
    }

    /// `local`'s own declaration span, for a synthesized statement (a `StorageDead`, chiefly)
    /// that addresses no source text of its own.
    pub(crate) fn local_decl_span(&self, local: Local) -> SrcSpan {
        self.local_decls[local.index()].span
    }

    // -----------------------------------------------------------------
    // Blocks
    // -----------------------------------------------------------------

    /// Reserves a new, empty block with no terminator yet, without moving the "current block"
    /// cursor onto it. The caller switches to it explicitly with [`BodyLowerCtx::switch_to`]
    /// once it is ready to lower code into it.
    pub(crate) fn new_block(&mut self) -> BasicBlock {
        let block = BasicBlock::from_usize(self.blocks.len());
        self.blocks.push(BlockBuilder {
            statements: Vec::new(),
            terminator: None,
        });
        block
    }

    pub(crate) fn current_block(&self) -> BasicBlock {
        self.current
    }

    /// Moves the "current block" cursor: every later [`BodyLowerCtx::push_stmt`]/
    /// [`BodyLowerCtx::set_terminator`] call targets `block` until this is called again.
    pub(crate) fn switch_to(&mut self, block: BasicBlock) {
        self.current = block;
    }

    pub(crate) fn push_stmt(&mut self, kind: StatementKind, span: SrcSpan) {
        self.blocks[self.current.index()]
            .statements
            .push(Statement { kind, span });
    }

    /// Sets the current block's terminator. Panics if it already has one -- a block gets exactly
    /// one transfer of control, and a second call here means two branches of lowering both tried
    /// to close the same block, a lowering-pass bug rather than anything a user program can cause.
    pub(crate) fn set_terminator(&mut self, kind: TerminatorKind, span: SrcSpan) {
        let block = &mut self.blocks[self.current.index()];
        assert!(
            block.terminator.is_none(),
            "mir::lower: block {:?} was given a terminator twice",
            self.current
        );
        block.terminator = Some(Terminator { kind, span });
    }

    // -----------------------------------------------------------------
    // Loops
    // -----------------------------------------------------------------

    pub(crate) fn push_loop(&mut self, break_target: BasicBlock, continue_target: BasicBlock) {
        self.loop_stack.push(LoopCtx {
            break_target,
            continue_target,
            scope_depth: self.block_scopes.len(),
        });
    }

    pub(crate) fn pop_loop(&mut self) {
        self.loop_stack
            .pop()
            .expect("mir::lower: pop_loop with no loop on the stack");
    }

    /// The innermost enclosing loop's break target and the exit obligations a `break` reached
    /// from here needs to replay first, innermost block first. `None` if `break` was reached
    /// outside any loop, which typeck already rules out for an accepted program.
    pub(crate) fn break_target(&self) -> Option<(BasicBlock, Vec<ExitObligation>)> {
        let loop_ctx = self.loop_stack.last()?;
        Some((
            loop_ctx.break_target,
            self.obligations_since(loop_ctx.scope_depth),
        ))
    }

    /// The counterpart of [`BodyLowerCtx::break_target`] for `continue`.
    pub(crate) fn continue_target(&self) -> Option<(BasicBlock, Vec<ExitObligation>)> {
        let loop_ctx = self.loop_stack.last()?;
        Some((
            loop_ctx.continue_target,
            self.obligations_since(loop_ctx.scope_depth),
        ))
    }

    // -----------------------------------------------------------------
    // Block-scoped exit obligations (`with`-lend `StorageDead`, `defer`)
    // -----------------------------------------------------------------

    pub(crate) fn push_block_scope(&mut self) {
        self.block_scopes.push(Vec::new());
    }

    /// Registers `obligation` against the innermost currently-open block, to be replayed at
    /// every point control leaves it.
    pub(crate) fn register_exit_obligation(&mut self, obligation: ExitObligation) {
        self.block_scopes
            .last_mut()
            .expect("mir::lower: register_exit_obligation with no open block scope")
            .push(obligation);
    }

    /// Pops the innermost block scope and returns its own obligations, oldest-registration-last
    /// (the order they should be replayed in: last-registered-runs-first, the same order ordinary
    /// drop/defer stacking already uses). This is the natural-fallthrough exit from a block: the
    /// scope is genuinely finished, so it comes off the stack.
    /// The innermost currently-open block scope's own obligations, in replay order, without
    /// removing it from the stack. Used by a `match` guard's failure path: the arm's bindings
    /// need cleaning up before falling to the next candidate, but the scope itself is still open
    /// for the arm's success path, lowered afterward in the same sequential pass.
    pub(crate) fn peek_block_scope(&self) -> Vec<ExitObligation> {
        self.block_scopes
            .last()
            .map(|scope| scope.iter().rev().copied().collect())
            .unwrap_or_default()
    }

    pub(crate) fn pop_block_scope(&mut self) -> Vec<ExitObligation> {
        let scope = self
            .block_scopes
            .pop()
            .expect("mir::lower: pop_block_scope with no open block scope");
        scope.into_iter().rev().collect()
    }

    /// Every obligation belonging to a block scope opened at or after `since_depth`, innermost
    /// block first and, within a block, last-registered first. Used for an early exit (`break`,
    /// `continue`, `return`) that leaves one or more still-open blocks without popping them --
    /// whatever is lexically after the early exit inside the same block, or a later loop
    /// iteration, still needs those scopes open.
    fn obligations_since(&self, since_depth: usize) -> Vec<ExitObligation> {
        let mut out = Vec::new();
        for scope in self.block_scopes[since_depth..].iter().rev() {
            out.extend(scope.iter().rev().copied());
        }
        out
    }

    /// Every currently-open block's exit obligations, for a `return` reached from anywhere in
    /// the body: it leaves every block between here and the function's own outermost one.
    pub(crate) fn obligations_for_return(&self) -> Vec<ExitObligation> {
        self.obligations_since(0)
    }

    // -----------------------------------------------------------------
    // Finishing
    // -----------------------------------------------------------------

    pub(crate) fn discover(&mut self, task: Task) {
        self.discovered.push(task);
    }

    /// Assembles the finished [`Body`], leaving `self` emptied out (but otherwise usable) behind
    /// -- `&mut self` rather than `self` by value, so a driver holding a `BodyLowerCtx` behind a
    /// `&mut` (as the worklist loop in `mir::lower` does, to read `discovered` afterward) does
    /// not have to give it up just to finish the body. Panics if any block reserved by
    /// [`BodyLowerCtx::new_block`] was never given a terminator by
    /// [`BodyLowerCtx::set_terminator`] -- every block this pass reserves, it also means to
    /// finish, so one left open is a lowering bug, not a user error.
    pub(crate) fn finish(&mut self, arg_count: usize, span: SrcSpan) -> Body {
        assert!(
            self.block_scopes.is_empty(),
            "mir::lower: {:?} finished with {} block scope(s) still open",
            self.def_id,
            self.block_scopes.len()
        );
        let def_id = self.def_id;
        let basic_blocks = std::mem::take(&mut self.blocks)
            .into_iter()
            .enumerate()
            .map(|(index, block)| BasicBlockData {
                statements: block.statements,
                terminator: block.terminator.unwrap_or_else(|| {
                    panic!("mir::lower: block {index} in {def_id:?} was never given a terminator")
                }),
            })
            .collect();
        Body {
            def_id,
            basic_blocks,
            local_decls: std::mem::take(&mut self.local_decls),
            arg_count,
            span,
        }
    }
}
