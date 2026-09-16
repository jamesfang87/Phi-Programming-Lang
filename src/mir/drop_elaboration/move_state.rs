use std::collections::HashSet;

use crate::mir::checks::borrowck::{Register, register_of};
use crate::mir::{
    BasicBlock, Body, Operand, Place, Rvalue, Statement, StatementKind, Terminator, TerminatorKind,
};

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(super) enum Ownership {
    Owned,
    Moved,
    Maybe,
}

#[derive(Clone, PartialEq, Eq, Default, Debug)]
pub(super) struct MoveState {
    may: HashSet<Register>,
    must: HashSet<Register>,
}

impl MoveState {
    fn meet(predecessors: &[&MoveState]) -> MoveState {
        let mut may = HashSet::new();
        for state in predecessors {
            may.extend(state.may.iter().cloned());
        }
        let mut must: HashSet<Register> = match predecessors.first() {
            Some(first) => first.must.clone(),
            None => HashSet::new(),
        };
        for state in &predecessors[predecessors.len().min(1)..] {
            must.retain(|register| state.must.contains(register));
        }
        MoveState { may, must }
    }

    fn covers(set: &HashSet<Register>, register: &Register, from: usize) -> bool {
        (from..=register.subregister.len()).any(|len| {
            set.contains(&Register {
                owner: register.owner,
                subregister: register.subregister[..len].to_vec(),
            })
        })
    }

    pub(super) fn status(&self, place: &Place) -> Ownership {
        self.status_from(&register_of(place), 0)
    }

    pub(super) fn status_within(&self, place: &Place, covered: &Register) -> Ownership {
        let register = register_of(place);
        debug_assert!(
            register.owner == covered.owner
                && register.subregister.starts_with(&covered.subregister),
            "status_within: {register:?} does not sit inside {covered:?}"
        );
        self.status_from(&register, covered.subregister.len() + 1)
    }

    fn status_from(&self, register: &Register, from: usize) -> Ownership {
        if Self::covers(&self.must, register, from) {
            Ownership::Moved
        } else if Self::covers(&self.may, register, from) {
            Ownership::Maybe
        } else {
            Ownership::Owned
        }
    }

    pub(super) fn owns_every_part(&self, place: &Place) -> bool {
        let register = register_of(place);
        !self.may.iter().any(|moved| {
            moved.owner == register.owner
                && moved.subregister.len() > register.subregister.len()
                && moved.subregister.starts_with(&register.subregister)
        })
    }

    fn mark_moved(&mut self, droppable: &[bool], register: Register) {
        if !droppable[register.owner.index()] {
            return;
        }
        self.may.insert(register.clone());
        self.must.insert(register);
    }

    fn mark_initialized(&mut self, droppable: &[bool], register: &Register) {
        if !droppable[register.owner.index()] {
            return;
        }
        for set in [&mut self.may, &mut self.must] {
            set.retain(|held| {
                held.owner != register.owner || !held.subregister.starts_with(&register.subregister)
            });
        }
    }

    fn apply_operand(&mut self, droppable: &[bool], operand: &Operand) {
        if let Operand::Move(place) = operand {
            self.mark_moved(droppable, register_of(place));
        }
    }

    fn apply_rvalue(&mut self, droppable: &[bool], rvalue: &Rvalue) {
        for operand in rvalue.operands() {
            self.apply_operand(droppable, operand);
        }
    }

    pub(super) fn apply_statement(&mut self, droppable: &[bool], stmt: &Statement) {
        match &stmt.kind {
            StatementKind::StorageLive(local) | StatementKind::StorageDead(local) => {
                let whole = Register {
                    owner: *local,
                    subregister: Vec::new(),
                };
                self.mark_initialized(droppable, &whole);
                self.mark_moved(droppable, whole);
            }
            StatementKind::Assign(place, rvalue) => {
                self.apply_rvalue(droppable, rvalue);
                self.mark_initialized(droppable, &register_of(place));
            }
            StatementKind::PlaceMention(_)
            | StatementKind::SetDiscriminant { .. }
            | StatementKind::WithLend(_) => {}
        }
    }

    pub(super) fn apply_terminator(&mut self, droppable: &[bool], terminator: &Terminator) {
        for operand in terminator.kind.operands() {
            self.apply_operand(droppable, operand);
        }
        match &terminator.kind {
            TerminatorKind::Call { destination, .. } => {
                self.mark_initialized(droppable, &register_of(destination))
            }
            TerminatorKind::Drop { place, .. } | TerminatorKind::DropIso { place, .. } => {
                self.mark_moved(droppable, register_of(place))
            }
            _ => {}
        }
    }
}

pub(super) fn analyze(droppable: &[bool], body: &Body) -> Vec<MoveState> {
    let lattice = crate::mir::checks::lattice::solve(
        body,
        MoveState::default(),
        MoveState::default(),
        |_current, pred_states| MoveState::meet(pred_states),
        |entry, _id, block| {
            let mut state = entry.clone();
            for stmt in &block.statements {
                state.apply_statement(droppable, stmt);
            }
            state.apply_terminator(droppable, &block.terminator);
            state
        },
    );

    (0..body.basic_blocks.len())
        .map(BasicBlock::from_usize)
        .map(|id| {
            lattice
                .entry(id)
                .expect("every block's entry is seeded by the solver")
                .clone()
        })
        .collect()
}
