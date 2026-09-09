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

    fn mark_moved(&mut self, register: Register) {
        self.may.insert(register.clone());
        self.must.insert(register);
    }

    fn mark_initialized(&mut self, register: &Register) {
        for set in [&mut self.may, &mut self.must] {
            set.retain(|held| {
                held.owner != register.owner || !held.subregister.starts_with(&register.subregister)
            });
        }
    }

    fn apply_operand(&mut self, operand: &Operand) {
        if let Operand::Move(place) = operand {
            self.mark_moved(register_of(place));
        }
    }

    fn apply_rvalue(&mut self, rvalue: &Rvalue) {
        for operand in rvalue.operands() {
            self.apply_operand(operand);
        }
    }

    pub(super) fn apply_statement(&mut self, stmt: &Statement) {
        match &stmt.kind {
            StatementKind::StorageLive(local) | StatementKind::StorageDead(local) => {
                let whole = Register {
                    owner: *local,
                    subregister: Vec::new(),
                };
                self.mark_initialized(&whole);
                self.mark_moved(whole);
            }
            StatementKind::Assign(place, rvalue) => {
                self.apply_rvalue(rvalue);
                self.mark_initialized(&register_of(place));
            }
            StatementKind::PlaceMention(_)
            | StatementKind::SetDiscriminant { .. }
            | StatementKind::WithLend(_) => {}
        }
    }

    pub(super) fn apply_terminator(&mut self, terminator: &Terminator) {
        for operand in terminator.kind.operands() {
            self.apply_operand(operand);
        }
        match &terminator.kind {
            TerminatorKind::Call { destination, .. } => {
                self.mark_initialized(&register_of(destination))
            }
            TerminatorKind::Drop { place, .. } | TerminatorKind::DropIso { place, .. } => {
                self.mark_moved(register_of(place))
            }
            _ => {}
        }
    }
}

pub(super) fn analyze(body: &Body) -> Vec<MoveState> {
    let mut entries = vec![MoveState::default(); body.basic_blocks.len()];
    let mut exits = vec![MoveState::default(); body.basic_blocks.len()];
    let predecessors = body.predecessors();

    let mut changed = true;
    while changed {
        changed = false;
        for (index, block) in body.basic_blocks.iter().enumerate() {
            let id = BasicBlock::from_usize(index);
            let incoming: Vec<&MoveState> = predecessors
                .of(id)
                .iter()
                .map(|&pred| &exits[pred.index()])
                .collect();
            let entry = MoveState::meet(&incoming);

            let mut exit = entry.clone();
            for stmt in &block.statements {
                exit.apply_statement(stmt);
            }
            exit.apply_terminator(&block.terminator);

            if entries[index] != entry {
                entries[index] = entry;
                changed = true;
            }
            if exits[index] != exit {
                exits[index] = exit;
                changed = true;
            }
        }
    }

    entries
}
