mod move_state;
#[cfg(test)]
mod tests;
mod tree;

use std::collections::HashMap;

use crate::driver::source::SrcSpan;
use crate::mir::checks::borrowck::{Register, to_register};
use crate::mir::{
    BasicBlock, BasicBlockData, Body, ConstKind, Constant, Instance, Local, LocalDecl, Operand,
    Place, Rvalue, Statement, StatementId, StatementKind, SwitchTargets, Terminator,
    TerminatorKind, place_ty,
};
use crate::nameres::PrimTy;
use crate::typeck::ty::Ty;
use crate::typeck::ty::ctx::TyCtx;

use move_state::MoveState;
use tree::{DropNode, FlagLocals, plan};

/// Returns `instances` with drop glue, allocation frees, and drop flags spliced into every body.
pub fn elaborate_drops(
    tcx: &mut TyCtx,
    mut instances: HashMap<Instance, Body>,
) -> HashMap<Instance, Body> {
    for body in instances.values_mut() {
        elaborate_body(tcx, body);
    }
    instances
}

/// A drop plan to splice into a block immediately before the statement at `before`.
struct Insertion {
    before: usize,
    tree: DropNode,
}

fn elaborate_body(tcx: &mut TyCtx, body: &mut Body) {
    let droppable: Vec<bool> = body
        .local_decls
        .iter()
        .map(|decl| tcx.needs_drop(decl.ty))
        .collect();

    let entry_states = move_state::analyze(&droppable, body);
    let bool_ty = tcx.mk_prim(PrimTy::Bool);
    let mut flags = FlagLocals::new(bool_ty, body);
    let insertions = plan_insertions(tcx, body, &entry_states, &droppable, &mut flags);

    if insertions.iter().all(|block| block.is_empty()) {
        return;
    }

    flags.declare(body);
    let mut editor = Editor::new(tcx, body);
    for (index, insertions) in insertions.into_iter().enumerate() {
        editor.rewrite_block(BasicBlock::from_usize(index), insertions, flags.flags());
    }
}

fn plan_insertions(
    tcx: &mut TyCtx,
    body: &Body,
    entry_states: &[MoveState],
    droppable: &[bool],
    flags: &mut FlagLocals,
) -> Vec<Vec<Insertion>> {
    let reachable = compute_reachable_blocks(body);
    body.basic_blocks
        .iter()
        .enumerate()
        .map(|(index, block)| {
            if !reachable[index] {
                return Vec::new();
            }
            plan_block_insertions(tcx, body, &entry_states[index], droppable, flags, block)
        })
        .collect()
}

/// Returns the insertions for one block, replaying the move state from its entry to each
/// statement that drops something.
fn plan_block_insertions(
    tcx: &mut TyCtx,
    body: &Body,
    entry_state: &MoveState,
    droppable: &[bool],
    flags: &mut FlagLocals,
    block: &BasicBlockData,
) -> Vec<Insertion> {
    let mut state = entry_state.clone();
    let mut insertions = Vec::new();

    for (before, statement) in block.statements.iter().enumerate() {
        if let Some(tree) = plan_at_statement(tcx, body, &state, flags, statement) {
            insertions.push(Insertion { before, tree });
        }
        state.apply_statement(droppable, statement);
    }

    if matches!(block.terminator.kind, TerminatorKind::Return)
        && let Some(tree) = plan_parameter_drops(tcx, body, &state, flags)
    {
        insertions.push(Insertion {
            before: block.statements.len(),
            tree,
        });
    }

    insertions
}

fn compute_reachable_blocks(body: &Body) -> Vec<bool> {
    let mut reachable = vec![false; body.basic_blocks.len()];
    let mut worklist = vec![BasicBlock::START_BLOCK];
    while let Some(block) = worklist.pop() {
        if std::mem::replace(&mut reachable[block.index()], true) {
            continue;
        }
        worklist.extend(body.basic_blocks[block.index()].terminator.successors());
    }
    reachable
}

fn plan_at_statement(
    tcx: &mut TyCtx,
    body: &Body,
    state: &MoveState,
    flags: &mut FlagLocals,
    statement: &Statement,
) -> Option<DropNode> {
    let place = match &statement.kind {
        StatementKind::StorageDead(local) => Place::from_local(*local),
        StatementKind::Assign(place, _) => place.clone(),
        _ => return None,
    };
    if place.local == Local::RETURN_PLACE {
        return None;
    }
    let ty = place_ty(tcx, &body.local_decls, &place);
    plan(tcx, state, flags, place, ty, None)
}

fn plan_parameter_drops(
    tcx: &mut TyCtx,
    body: &Body,
    state: &MoveState,
    flags: &mut FlagLocals,
) -> Option<DropNode> {
    let mut nodes = Vec::new();
    for index in (1..=body.param_count).rev() {
        let place = Place::from_local(Local::from_usize(index));
        let ty = body.local_decls[index].ty;
        if let Some(node) = plan(tcx, state, flags, place, ty, None) {
            nodes.push(node);
        }
    }
    (!nodes.is_empty()).then_some(DropNode::Seq(nodes))
}

struct Editor<'a> {
    body: &'a mut Body,
    next_statement_id: usize,
    bool_ty: Ty,
    discriminant_ty: Ty,
}

impl<'a> Editor<'a> {
    fn new(tcx: &mut TyCtx, body: &'a mut Body) -> Editor<'a> {
        let next_statement_id = body
            .basic_blocks
            .iter()
            .flat_map(|block| &block.statements)
            .map(|statement| statement.id.index() + 1)
            .max()
            .unwrap_or(0);
        Editor {
            body,
            next_statement_id,
            bool_ty: tcx.mk_prim(PrimTy::Bool),
            discriminant_ty: tcx.mk_prim(PrimTy::I32),
        }
    }

    fn new_statement(&mut self, kind: StatementKind, span: SrcSpan) -> Statement {
        let id = StatementId::from_usize(self.next_statement_id);
        self.next_statement_id += 1;
        Statement { id, kind, span }
    }

    fn set_flag(&mut self, flag: Local, value: bool, span: SrcSpan) -> Statement {
        let constant = Constant {
            ty: self.bool_ty,
            kind: ConstKind::Bool(value),
        };
        self.new_statement(
            StatementKind::Assign(
                Place::from_local(flag),
                Rvalue::Use(Operand::Constant(constant)),
            ),
            span,
        )
    }

    fn new_block(
        &mut self,
        statements: Vec<Statement>,
        terminator: TerminatorKind,
        span: SrcSpan,
    ) -> BasicBlock {
        let block = BasicBlock::from_usize(self.body.basic_blocks.len());
        self.body.basic_blocks.push(BasicBlockData {
            statements,
            terminator: Terminator {
                kind: terminator,
                span,
            },
        });
        block
    }

    fn new_temp(&mut self, ty: Ty, span: SrcSpan) -> Local {
        let local = Local::from_usize(self.body.local_decls.len());
        self.body.local_decls.push(LocalDecl {
            ty,
            name: None,
            span,
        });
        local
    }

    fn emit(&mut self, node: DropNode, continues_at: BasicBlock, span: SrcSpan) -> BasicBlock {
        match node {
            DropNode::Glue(place) => self.emit_drop(place, continues_at, span),
            DropNode::FreeAllocation(place) => self.emit_free(place, continues_at, span),
            DropNode::Seq(nodes) => nodes
                .into_iter()
                .rev()
                .fold(continues_at, |next, node| self.emit(node, next, span)),
            DropNode::Variants { place, arms } => {
                self.emit_variants(place, arms, continues_at, span)
            }
            DropNode::Flagged { flag, inner } => {
                self.emit_flagged(flag, *inner, continues_at, span)
            }
        }
    }

    fn emit_drop(&mut self, place: Place, target: BasicBlock, span: SrcSpan) -> BasicBlock {
        self.new_block(Vec::new(), TerminatorKind::Drop { place, target }, span)
    }

    fn emit_free(&mut self, place: Place, target: BasicBlock, span: SrcSpan) -> BasicBlock {
        self.new_block(Vec::new(), TerminatorKind::DropIso { place, target }, span)
    }

    fn emit_variants(
        &mut self,
        place: Place,
        arms: Vec<(crate::mir::VariantIdx, DropNode)>,
        continues_at: BasicBlock,
        span: SrcSpan,
    ) -> BasicBlock {
        let discriminant = self.new_temp(self.discriminant_ty, span);
        let values = arms
            .into_iter()
            .map(|(variant, arm)| (variant.index() as u128, self.emit(arm, continues_at, span)))
            .collect();
        let read = self.new_statement(
            StatementKind::Assign(Place::from_local(discriminant), Rvalue::Discriminant(place)),
            span,
        );
        self.new_block(
            vec![read],
            TerminatorKind::SwitchInt {
                discr: Operand::Copy(Place::from_local(discriminant)),
                targets: SwitchTargets {
                    values,
                    otherwise: continues_at,
                },
            },
            span,
        )
    }

    fn emit_flagged(
        &mut self,
        flag: Local,
        inner: DropNode,
        continues_at: BasicBlock,
        span: SrcSpan,
    ) -> BasicBlock {
        let inner = self.emit(inner, continues_at, span);
        let cleared = self.set_flag(flag, false, span);
        let arm = self.new_block(vec![cleared], TerminatorKind::Goto { target: inner }, span);
        self.new_block(
            Vec::new(),
            TerminatorKind::SwitchInt {
                discr: Operand::Copy(Place::from_local(flag)),
                targets: SwitchTargets {
                    values: vec![(1, arm)],
                    otherwise: continues_at,
                },
            },
            span,
        )
    }

    fn rewrite_block(
        &mut self,
        block: BasicBlock,
        insertions: Vec<Insertion>,
        flags: &[(Register, Local)],
    ) {
        let data = std::mem::replace(
            &mut self.body.basic_blocks[block.index()],
            BasicBlockData {
                statements: Vec::new(),
                terminator: Terminator {
                    kind: TerminatorKind::Unreachable,
                    span: self.body.span,
                },
            },
        );

        let mut statements = Vec::with_capacity(data.statements.len());
        let mut cuts: Vec<(usize, DropNode)> = Vec::new();
        let mut insertions = insertions.into_iter().peekable();

        for (index, statement) in data.statements.into_iter().enumerate() {
            if insertions.peek().is_some_and(|next| next.before == index) {
                let tree = insertions.next().expect("just peeked").tree;
                cuts.push((statements.len(), tree));
            }
            let span = statement.span;
            let updates = self.compute_flag_updates_for_statement(&statement, flags, span);
            statements.push(statement);
            statements.extend(updates);
        }

        let span = data.terminator.span;
        statements.extend(self.compute_flag_updates_for_terminator(&data.terminator, flags, span));
        if let Some(insertion) = insertions.next() {
            cuts.push((statements.len(), insertion.tree));
        }

        self.splice(block, statements, data.terminator, cuts);
    }

    fn splice(
        &mut self,
        block: BasicBlock,
        mut statements: Vec<Statement>,
        terminator: Terminator,
        mut cuts: Vec<(usize, DropNode)>,
    ) {
        let span = terminator.span;
        let Some((position, tree)) = cuts.pop() else {
            self.body.basic_blocks[block.index()] = BasicBlockData {
                statements,
                terminator,
            };
            return;
        };

        let tail = statements.split_off(position);
        let continues_at = self.new_block(tail, terminator.kind, span);
        let mut entry = self.emit(tree, continues_at, span);
        while let Some((position, tree)) = cuts.pop() {
            let run = statements.split_off(position);
            let continues_at = self.new_block(run, TerminatorKind::Goto { target: entry }, span);
            entry = self.emit(tree, continues_at, span);
        }

        self.body.basic_blocks[block.index()] = BasicBlockData {
            statements,
            terminator: Terminator {
                kind: TerminatorKind::Goto { target: entry },
                span,
            },
        };
    }

    fn compute_flag_updates_for_statement(
        &mut self,
        statement: &Statement,
        flags: &[(Register, Local)],
        span: SrcSpan,
    ) -> Vec<Statement> {
        let mut updates = Vec::new();
        match &statement.kind {
            StatementKind::StorageLive(local) => {
                for (register, flag) in flags {
                    if register.owner == *local {
                        updates.push((*flag, false));
                    }
                }
            }
            StatementKind::Assign(place, rvalue) => {
                for operand in rvalue.operands() {
                    updates.extend(compute_moved_flags(operand, flags));
                }
                updates.extend(compute_initialized_flags(place, flags));
            }
            _ => {}
        }
        updates
            .into_iter()
            .map(|(flag, value)| self.set_flag(flag, value, span))
            .collect()
    }

    fn compute_flag_updates_for_terminator(
        &mut self,
        terminator: &Terminator,
        flags: &[(Register, Local)],
        span: SrcSpan,
    ) -> Vec<Statement> {
        let mut updates = Vec::new();
        for operand in terminator.kind.operands() {
            updates.extend(compute_moved_flags(operand, flags));
        }
        if let TerminatorKind::Call { destination, .. } = &terminator.kind {
            updates.extend(compute_initialized_flags(destination, flags));
        }
        updates
            .into_iter()
            .map(|(flag, value)| self.set_flag(flag, value, span))
            .collect()
    }
}

fn compute_moved_flags(operand: &Operand, flags: &[(Register, Local)]) -> Vec<(Local, bool)> {
    let Operand::Move(place) = operand else {
        return Vec::new();
    };
    let moved = to_register(place);
    flags
        .iter()
        .filter(|(register, _)| overlaps(register, &moved))
        .map(|(_, flag)| (*flag, false))
        .collect()
}

fn compute_initialized_flags(place: &Place, flags: &[(Register, Local)]) -> Vec<(Local, bool)> {
    let initialized = to_register(place);
    flags
        .iter()
        .filter(|(register, _)| covers(&initialized, register))
        .map(|(_, flag)| (*flag, true))
        .collect()
}

fn covers(outer: &Register, inner: &Register) -> bool {
    outer.owner == inner.owner && inner.subregister.starts_with(&outer.subregister)
}

fn overlaps(left: &Register, right: &Register) -> bool {
    covers(left, right) || covers(right, left)
}
