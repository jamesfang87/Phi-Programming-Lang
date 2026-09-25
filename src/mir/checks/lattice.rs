use std::collections::{HashMap, VecDeque};
use std::hash::Hash;

use crate::mir::{BasicBlock, BasicBlockData, Body};

pub struct Lattice<Key: Hash + Eq, State> {
    pub entry: HashMap<Key, State>,
    pub exit: HashMap<Key, State>,
}

impl<Key: Hash + Eq, State> Lattice<Key, State> {
    pub fn set_entry(&mut self, key: Key, state: State) -> Option<State> {
        self.entry.insert(key, state)
    }

    pub fn set_exit(&mut self, key: Key, state: State) -> Option<State> {
        self.exit.insert(key, state)
    }

    pub fn entry(&self, key: Key) -> Option<&State> {
        self.entry.get(&key)
    }

    pub fn exit(&self, key: Key) -> Option<&State> {
        self.exit.get(&key)
    }
}

impl<Key: Hash + Eq, State> Default for Lattice<Key, State> {
    fn default() -> Self {
        Self {
            entry: HashMap::new(),
            exit: HashMap::new(),
        }
    }
}

pub fn solve<State: Clone + PartialEq>(
    body: &Body,
    init_entry: State,
    init_exit: State,
    meet: impl Fn(&State, &[&State]) -> State,
    transfer: impl Fn(&State, BasicBlock, &BasicBlockData) -> State,
) -> Lattice<BasicBlock, State> {
    let n = body.basic_blocks.len();
    let mut lattice: Lattice<BasicBlock, State> = Default::default();
    let preds = body.predecessors();

    for index in 0..n {
        let id = BasicBlock::from_usize(index);
        lattice.set_entry(id, init_entry.clone());
        lattice.set_exit(id, init_exit.clone());
    }

    let mut queued = vec![true; n];
    let mut queue: VecDeque<BasicBlock> = reverse_postorder(body).into();

    while let Some(id) = queue.pop_front() {
        queued[id.index()] = false;
        let block = &body.basic_blocks[id.index()];

        let new_entry = {
            let pred_states: Vec<&State> = preds
                .of(id)
                .iter()
                .filter_map(|&pred| lattice.exit(pred))
                .collect();
            let current = lattice
                .entry(id)
                .expect("every block's entry is seeded above");
            meet(current, &pred_states)
        };

        let new_exit = transfer(&new_entry, id, block);
        lattice.set_entry(id, new_entry);

        if lattice.exit(id) != Some(&new_exit) {
            lattice.set_exit(id, new_exit);
            for succ in block.terminator.successors() {
                if !queued[succ.index()] {
                    queued[succ.index()] = true;
                    queue.push_back(succ);
                }
            }
        }
    }

    lattice
}

pub fn solve_backward<State: Clone + PartialEq>(
    body: &Body,
    init_entry: State,
    init_exit: State,
    meet: impl Fn(&State, &[&State]) -> State,
    transfer: impl Fn(&State, BasicBlock, &BasicBlockData) -> State,
) -> Lattice<BasicBlock, State> {
    let n = body.basic_blocks.len();
    let mut lattice: Lattice<BasicBlock, State> = Default::default();
    let preds = body.predecessors();

    for index in 0..n {
        let id = BasicBlock::from_usize(index);
        lattice.set_entry(id, init_entry.clone());
        lattice.set_exit(id, init_exit.clone());
    }

    let mut queued = vec![true; n];
    let mut queue: VecDeque<BasicBlock> = {
        let mut order = reverse_postorder(body);
        order.reverse();
        order.into()
    };

    while let Some(id) = queue.pop_front() {
        queued[id.index()] = false;
        let block = &body.basic_blocks[id.index()];

        let new_exit = {
            let succ_states: Vec<&State> = body
                .successors(id)
                .filter_map(|succ| lattice.entry(succ))
                .collect();
            let current = lattice
                .exit(id)
                .expect("every block's exit is seeded above");
            meet(current, &succ_states)
        };

        let new_entry = transfer(&new_exit, id, block);
        lattice.set_exit(id, new_exit);

        if lattice.entry(id) != Some(&new_entry) {
            lattice.set_entry(id, new_entry);
            for &pred in preds.of(id) {
                if !queued[pred.index()] {
                    queued[pred.index()] = true;
                    queue.push_back(pred);
                }
            }
        }
    }

    lattice
}

fn reverse_postorder(body: &Body) -> Vec<BasicBlock> {
    let n = body.basic_blocks.len();
    let mut visited = vec![false; n];
    let mut order = Vec::with_capacity(n);

    let roots = std::iter::once(BasicBlock::START_BLOCK).chain((0..n).map(BasicBlock::from_usize));
    for root in roots {
        if visited[root.index()] {
            continue;
        }
        let mut post = Vec::new();
        let mut stack = vec![(root, false)];
        while let Some((block, expanded)) = stack.pop() {
            if expanded {
                post.push(block);
                continue;
            }
            if visited[block.index()] {
                continue;
            }
            visited[block.index()] = true;
            stack.push((block, true));
            for succ in body.basic_blocks[block.index()].terminator.successors() {
                if !visited[succ.index()] {
                    stack.push((succ, false));
                }
            }
        }
        post.reverse();
        order.extend(post);
    }

    order
}
