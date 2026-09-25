use std::collections::{HashMap, HashSet};

use crate::ast::Ident;
use crate::driver::cli::Mode;
use crate::driver::source::SrcSpan;
use crate::hir::{DefId, Hir, HirId};
use crate::mir::lower::Task;
use crate::mir::{
    AnyMode, BasicBlock, BasicBlockData, Body, BodyKind, Local, LocalDecl, Place, Statement,
    StatementId, StatementKind, Terminator, TerminatorKind,
};
use crate::session::Session;
use crate::typeck::results::TypeResolutions;
use crate::typeck::ty::Ty;
use crate::typeck::ty::ctx::TyCtx;

#[derive(Clone, Copy, Debug)]
pub(super) enum ExitObligation {
    StorageDead(Local),

    RunDeferred(HirId),
}

struct LoopCtx {
    break_target: BasicBlock,
    continue_target: BasicBlock,
    scope_depth: usize,
}

struct BlockBuilder {
    statements: Vec<Statement>,
    terminator: Option<Terminator>,
}

pub(crate) struct BodyLowerCtx<'a> {
    pub(crate) session: &'a Session,
    pub(crate) hir: &'a Hir,
    pub(crate) tcx: &'a mut TyCtx,
    pub(crate) types: &'a TypeResolutions,
    pub(crate) mode: Mode,
    pub(crate) def_id: DefId,
    pub(crate) any_mode: Option<AnyMode>,

    pub(crate) kind: BodyKind,
    pub(crate) generics: Vec<HirId>,

    local_decls: Vec<LocalDecl>,
    blocks: Vec<BlockBuilder>,
    current: BasicBlock,
    next_stmt_id: usize,

    hir_locals: HashMap<HirId, Place>,

    loop_stack: Vec<LoopCtx>,

    block_scopes: Vec<Vec<ExitObligation>>,

    pub(crate) discovered: Vec<Task>,

    pub(crate) coerced: HashSet<HirId>,
}

impl<'a> BodyLowerCtx<'a> {
    pub(crate) fn new(
        session: &'a Session,
        hir: &'a Hir,
        tcx: &'a mut TyCtx,
        types: &'a TypeResolutions,
        mode: Mode,
        def_id: DefId,
        any_mode: Option<AnyMode>,
    ) -> Self {
        let mut ctx = BodyLowerCtx {
            session,
            hir,
            tcx,
            types,
            mode,
            def_id,
            any_mode,
            kind: BodyKind::Function,
            generics: Vec::new(),
            local_decls: Vec::new(),
            blocks: Vec::new(),
            current: BasicBlock::from_usize(0),
            next_stmt_id: 0,
            hir_locals: HashMap::new(),
            loop_stack: Vec::new(),
            block_scopes: Vec::new(),
            discovered: Vec::new(),
            coerced: HashSet::new(),
        };
        let entry = ctx.new_block();
        ctx.current = entry;
        ctx
    }

    pub(crate) fn new_local(&mut self, ty: Ty, name: Option<Ident>, span: SrcSpan) -> Local {
        let local = Local::from_usize(self.local_decls.len());
        self.local_decls.push(LocalDecl { ty, name, span });
        local
    }

    pub(crate) fn new_temp(&mut self, ty: Ty, span: SrcSpan) -> Local {
        let local = self.new_local(ty, None, span);
        self.push_stmt(StatementKind::StorageLive(local), span);
        self.register_exit_obligation(ExitObligation::StorageDead(local));
        local
    }

    pub(crate) fn bind_local(&mut self, id: impl Into<HirId>, local: Local) {
        let id = id.into();
        self.hir_locals.insert(id, Place::from_local(local));
    }

    pub(crate) fn bind_place(&mut self, id: impl Into<HirId>, place: Place) {
        let id = id.into();
        self.hir_locals.insert(id, place);
    }

    pub(crate) fn place_for(&self, id: impl Into<HirId>) -> Place {
        let id = id.into();
        self.hir_locals
            .get(&id)
            .unwrap_or_else(|| panic!("mir::lower: no place bound for {id:?}"))
            .clone()
    }

    pub(crate) fn local_decl_span(&self, local: Local) -> SrcSpan {
        self.local_decls[local.index()].span
    }

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

    pub(crate) fn switch_to(&mut self, block: BasicBlock) {
        self.current = block;
    }

    pub(crate) fn push_stmt(&mut self, kind: StatementKind, span: SrcSpan) {
        let id = StatementId::from_usize(self.next_stmt_id);
        self.next_stmt_id += 1;
        self.blocks[self.current.index()]
            .statements
            .push(Statement { id, kind, span });
    }

    pub(crate) fn set_terminator(&mut self, kind: TerminatorKind, span: SrcSpan) {
        let block = &mut self.blocks[self.current.index()];
        assert!(
            block.terminator.is_none(),
            "mir::lower: block {:?} was given a terminator twice",
            self.current
        );
        block.terminator = Some(Terminator { kind, span });
    }

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

    pub(crate) fn break_target(&self) -> Option<(BasicBlock, Vec<ExitObligation>)> {
        let loop_ctx = self.loop_stack.last()?;
        Some((
            loop_ctx.break_target,
            self.obligations_since(loop_ctx.scope_depth),
        ))
    }

    pub(crate) fn continue_target(&self) -> Option<(BasicBlock, Vec<ExitObligation>)> {
        let loop_ctx = self.loop_stack.last()?;
        Some((
            loop_ctx.continue_target,
            self.obligations_since(loop_ctx.scope_depth),
        ))
    }

    pub(crate) fn push_block_scope(&mut self) {
        self.block_scopes.push(Vec::new());
    }

    pub(crate) fn register_exit_obligation(&mut self, obligation: ExitObligation) {
        self.block_scopes
            .last_mut()
            .expect("mir::lower: register_exit_obligation with no open block scope")
            .push(obligation);
    }

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

    fn obligations_since(&self, since_depth: usize) -> Vec<ExitObligation> {
        let mut out = Vec::new();
        for scope in self.block_scopes[since_depth..].iter().rev() {
            out.extend(scope.iter().rev().copied());
        }
        out
    }

    pub(crate) fn obligations_for_return(&self) -> Vec<ExitObligation> {
        self.obligations_since(0)
    }

    pub(crate) fn discover(&mut self, task: Task) {
        self.discovered.push(task);
    }

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
            kind: self.kind,
            basic_blocks,
            local_decls: std::mem::take(&mut self.local_decls),
            param_count: arg_count,
            generics: std::mem::take(&mut self.generics),
            span,
        }
    }
}
