use std::collections::HashSet;

use crate::mir::checks::borrowck::{Register, to_register};
use crate::mir::{
    BasicBlock, Body, Operand, Place, Rvalue, Statement, StatementKind, Terminator, TerminatorKind,
};

/// Whether a place still owns its value, has been moved, or may have been moved on some path.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(super) enum Ownership {
    Owned,
    Moved,
    MaybeMoved,
}

/// The registers that may have been moved and the subset that definitely has been.
#[derive(Clone, PartialEq, Eq, Default, Debug)]
pub(super) struct MoveState {
    maybe_moved: HashSet<Register>,
    definitely_moved: HashSet<Register>,
}

impl MoveState {
    fn meet(predecessors: &[&MoveState]) -> MoveState {
        let mut maybe_moved = HashSet::new();
        for state in predecessors {
            maybe_moved.extend(state.maybe_moved.iter().cloned());
        }
        let mut definitely_moved: HashSet<Register> = match predecessors.first() {
            Some(first) => first.definitely_moved.clone(),
            None => HashSet::new(),
        };
        for state in &predecessors[predecessors.len().min(1)..] {
            definitely_moved.retain(|register| state.definitely_moved.contains(register));
        }
        MoveState {
            maybe_moved,
            definitely_moved,
        }
    }

    fn covers(set: &HashSet<Register>, register: &Register, from: usize) -> bool {
        (from..=register.subregister.len()).any(|len| {
            set.contains(&Register {
                owner: register.owner,
                subregister: register.subregister[..len].to_vec(),
            })
        })
    }

    pub(super) fn ownership_of(&self, place: &Place) -> Ownership {
        self.ownership_from(&to_register(place), 0)
    }

    pub(super) fn ownership_within(&self, place: &Place, covered: &Register) -> Ownership {
        let register = to_register(place);
        debug_assert!(
            register.owner == covered.owner
                && register.subregister.starts_with(&covered.subregister),
            "ownership_within: {register:?} does not sit inside {covered:?}"
        );
        self.ownership_from(&register, covered.subregister.len() + 1)
    }

    fn ownership_from(&self, register: &Register, from: usize) -> Ownership {
        if Self::covers(&self.definitely_moved, register, from) {
            Ownership::Moved
        } else if Self::covers(&self.maybe_moved, register, from) {
            Ownership::MaybeMoved
        } else {
            Ownership::Owned
        }
    }

    pub(super) fn owns_every_part(&self, place: &Place) -> bool {
        let register = to_register(place);
        !self.maybe_moved.iter().any(|moved| {
            moved.owner == register.owner
                && moved.subregister.len() > register.subregister.len()
                && moved.subregister.starts_with(&register.subregister)
        })
    }

    fn mark_moved(&mut self, droppable: &[bool], register: Register) {
        if !droppable[register.owner.index()] {
            return;
        }
        self.maybe_moved.insert(register.clone());
        self.definitely_moved.insert(register);
    }

    fn mark_initialized(&mut self, droppable: &[bool], register: &Register) {
        if !droppable[register.owner.index()] {
            return;
        }
        for set in [&mut self.maybe_moved, &mut self.definitely_moved] {
            set.retain(|held| {
                held.owner != register.owner || !held.subregister.starts_with(&register.subregister)
            });
        }
    }

    fn apply_operand(&mut self, droppable: &[bool], operand: &Operand) {
        if let Operand::Move(place) = operand {
            self.mark_moved(droppable, to_register(place));
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
                self.mark_initialized(droppable, &to_register(place));
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
                self.mark_initialized(droppable, &to_register(destination))
            }
            TerminatorKind::Drop { place, .. } | TerminatorKind::DropIso { place, .. } => {
                self.mark_moved(droppable, to_register(place))
            }
            _ => {}
        }
    }
}

/// Returns the move state on entry to every block of `body`.
pub(super) fn analyze(droppable: &[bool], body: &Body) -> Vec<MoveState> {
    let solved = crate::mir::checks::lattice::solve(
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
            solved
                .entry(id)
                .expect("every block's entry is seeded by the solver")
                .clone()
        })
        .collect()
}
