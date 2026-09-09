//! Drop elaboration: the MIR-to-MIR pass that turns each owned local's implicit end-of-scope
//! drop into explicit `TerminatorKind::Drop`s.
//!
//! Runs after [`crate::mir::monomorphize`], on the concrete, monomorphized `Body` of every
//! `Instance` -- not on the generic bodies `mir::lower` produces -- since deciding whether a
//! local needs dropping requires a concrete `Ty` (see `codegen::drop::drop_glue`'s own match on
//! `TyKind::Iso`, which is what a `Drop` terminator ultimately compiles down to).
//!
//! For every local whose `StorageLive`/`StorageDead` pair `mir::lower` already emits, this pass
//! decides, at the point of that `StorageDead`, whether the local still owns a value that needs
//! dropping. That decision has two parts, each of which can make an unconditional
//! "one `Drop` right before the `StorageDead`" insertion wrong:
//!
//! - **Ownership can be conditional.** A local moved out (via `Operand::Move`) on one branch but
//!   not another has no single static answer at the point the branches rejoin -- MIR has no phi
//!   nodes to carry "was it moved" through a join. Where the set of predecessors feeding that join
//!   is finite and statically known (a plain `if`), this pass may resolve it by open-coding the
//!   drop separately into each predecessor instead of any runtime check at all. Where it isn't --
//!   chiefly a value conditionally moved inside a loop, whose ownership state must survive across
//!   the loop's own back edge -- this pass threads a synthesized `bool` drop flag through the
//!   local's lifetime instead: set `true` where it starts being owned, set `false` at each point
//!   it is moved out, and read at the point of doubt via a `SwitchInt` that replaces the
//!   unconditional `Drop` with one arm that drops and one that skips.
//! - **A composite type's need to drop can be conditional on its own runtime shape.** A struct
//!   that is not itself `iso`/`Drop` can still own fields that need dropping, and each such field
//!   is dropped individually (a separate `Drop` per field, `Place`-projected via
//!   `Projection::Field`) rather than the struct being handed to `drop_glue` as one unit. An enum
//!   is the same idea taken one step further: which fields need dropping depends on which variant
//!   is active at runtime, so this pass reads the value's own discriminant (`Rvalue::Discriminant`)
//!   through a `SwitchInt`, and only the active variant's arm drops its fields (`Place`-projected
//!   via `Projection::Downcast` then `Projection::Field`).
//!
//! In every case, the block containing the local's `StorageDead` is split as needed so each
//! `Drop` (or the `SwitchInt` guarding one) can end its own block as a terminator, resuming with
//! the original `StorageDead` and everything after it.
//!
//! `StorageDead` is not the only point a local's old value can need dropping, though: a genuine
//! reassignment (`p = new_value;` to an already, possibly conditionally, initialized `p` -- as
//! opposed to `p`'s own first, initializing assignment, which has no old value to drop) overwrites
//! whatever the place held with no `StorageDead` involved at all. Left alone, that old value's
//! resources become unreachable -- a leak, the mirror image of the double-free a missing
//! moved-out check at `StorageDead` would cause. So every genuine reassignment gets the same
//! treatment: a `Drop` of the place's old value spliced in immediately before it, decided by the
//! same "was it moved by now" analysis (unconditional, skipped, or flag-gated) as the
//! `StorageDead` case, just asked at the reassignment's own program point instead.

mod move_state;
#[cfg(test)]
mod tests;
mod tree;

use std::collections::HashMap;

use crate::driver::source::SrcSpan;
use crate::mir::checks::borrowck::{Register, register_of};
use crate::mir::{
    BasicBlock, BasicBlockData, Body, ConstKind, Constant, Instance, Local, LocalDecl, Operand,
    Place, Rvalue, Statement, StatementId, StatementKind, SwitchTargets, Terminator,
    TerminatorKind, place_ty,
};
use crate::nameres::PrimTy;
use crate::typeck::ty::Ty;
use crate::typeck::tyctx::TyCtx;

use move_state::MoveState;
use tree::{DropNode, FlagLocals, plan};

pub fn elaborate_drops(
    tcx: &mut TyCtx,
    mut instances: HashMap<Instance, Body>,
) -> HashMap<Instance, Body> {
    for body in instances.values_mut() {
        elaborate_body(tcx, body);
    }
    instances
}

struct Insertion {
    before: usize,
    tree: DropNode,
}

fn elaborate_body(tcx: &mut TyCtx, body: &mut Body) {
    let entry_states = move_state::analyze(body);
    let bool_ty = tcx.mk_prim(PrimTy::Bool);
    let mut flags = FlagLocals::new(bool_ty, body);
    let insertions = plan_insertions(tcx, body, &entry_states, &mut flags);

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
    flags: &mut FlagLocals,
) -> Vec<Vec<Insertion>> {
    let reachable = reachable_blocks(body);
    let mut insertions: Vec<Vec<Insertion>> = Vec::new();

    for (index, block) in body.basic_blocks.iter().enumerate() {
        if !reachable[index] {
            insertions.push(Vec::new());
            continue;
        }
        let mut state = entry_states[index].clone();
        let mut block_insertions = Vec::new();

        for (before, statement) in block.statements.iter().enumerate() {
            if let Some(tree) = plan_at_statement(tcx, body, &state, flags, statement) {
                block_insertions.push(Insertion { before, tree });
            }
            state.apply_statement(statement);
        }

        if matches!(block.terminator.kind, TerminatorKind::Return)
            && let Some(tree) = plan_parameter_drops(tcx, body, &state, flags)
        {
            block_insertions.push(Insertion {
                before: block.statements.len(),
                tree,
            });
        }

        insertions.push(block_insertions);
    }

    insertions
}

fn reachable_blocks(body: &Body) -> Vec<bool> {
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

    fn statement(&mut self, kind: StatementKind, span: SrcSpan) -> Statement {
        let id = StatementId::from_usize(self.next_statement_id);
        self.next_statement_id += 1;
        Statement { id, kind, span }
    }

    fn set_flag(&mut self, flag: Local, value: bool, span: SrcSpan) -> Statement {
        let constant = Constant {
            ty: self.bool_ty,
            kind: ConstKind::Bool(value),
        };
        self.statement(
            StatementKind::Assign(
                Place::from_local(flag),
                Rvalue::Use(Operand::Constant(constant)),
            ),
            span,
        )
    }

    fn block(
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

    fn temp(&mut self, ty: Ty, span: SrcSpan) -> Local {
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
            DropNode::Glue(place) => self.block(
                Vec::new(),
                TerminatorKind::Drop {
                    place,
                    target: continues_at,
                },
                span,
            ),
            DropNode::FreeAllocation(place) => self.block(
                Vec::new(),
                TerminatorKind::DropIso {
                    place,
                    target: continues_at,
                },
                span,
            ),
            DropNode::Seq(nodes) => nodes
                .into_iter()
                .rev()
                .fold(continues_at, |next, node| self.emit(node, next, span)),
            DropNode::Variants { place, arms } => {
                let discriminant = self.temp(self.discriminant_ty, span);
                let values = arms
                    .into_iter()
                    .map(|(variant, arm)| {
                        (variant.index() as u128, self.emit(arm, continues_at, span))
                    })
                    .collect();
                let read = self.statement(
                    StatementKind::Assign(
                        Place::from_local(discriminant),
                        Rvalue::Discriminant(place),
                    ),
                    span,
                );
                self.block(
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
            DropNode::Flagged { flag, inner } => {
                let inner = self.emit(*inner, continues_at, span);
                let cleared = self.set_flag(flag, false, span);
                let arm = self.block(vec![cleared], TerminatorKind::Goto { target: inner }, span);
                self.block(
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
        }
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
            let updates = self.flag_updates_for_statement(&statement, flags, span);
            statements.push(statement);
            statements.extend(updates);
        }

        let span = data.terminator.span;
        statements.extend(self.flag_updates_for_terminator(&data.terminator, flags, span));
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
        let continues_at = self.block(tail, terminator.kind, span);
        let mut entry = self.emit(tree, continues_at, span);
        while let Some((position, tree)) = cuts.pop() {
            let run = statements.split_off(position);
            let continues_at = self.block(run, TerminatorKind::Goto { target: entry }, span);
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

    fn flag_updates_for_statement(
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
                    updates.extend(moved_flags(operand, flags));
                }
                updates.extend(initialized_flags(place, flags));
            }
            _ => {}
        }
        updates
            .into_iter()
            .map(|(flag, value)| self.set_flag(flag, value, span))
            .collect()
    }

    fn flag_updates_for_terminator(
        &mut self,
        terminator: &Terminator,
        flags: &[(Register, Local)],
        span: SrcSpan,
    ) -> Vec<Statement> {
        let mut updates = Vec::new();
        for operand in terminator.kind.operands() {
            updates.extend(moved_flags(operand, flags));
        }
        if let TerminatorKind::Call { destination, .. } = &terminator.kind {
            updates.extend(initialized_flags(destination, flags));
        }
        updates
            .into_iter()
            .map(|(flag, value)| self.set_flag(flag, value, span))
            .collect()
    }
}

fn moved_flags(operand: &Operand, flags: &[(Register, Local)]) -> Vec<(Local, bool)> {
    let Operand::Move(place) = operand else {
        return Vec::new();
    };
    let moved = register_of(place);
    flags
        .iter()
        .filter(|(register, _)| overlaps(register, &moved))
        .map(|(_, flag)| (*flag, false))
        .collect()
}

fn initialized_flags(place: &Place, flags: &[(Register, Local)]) -> Vec<(Local, bool)> {
    let initialized = register_of(place);
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
