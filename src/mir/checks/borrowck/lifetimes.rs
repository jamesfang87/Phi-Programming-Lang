use std::collections::{HashMap, HashSet};
use std::ops::Range;

use crate::ast::Mutability;
use crate::hir::DefId;
use crate::mir::checks::borrowck::{Register, SubRegisters, register_of};
use crate::mir::checks::lattice;
use crate::mir::ids::StatementId;
use crate::mir::{
    AnyMode, BasicBlock, BasicBlockData, Body, Local, Operand, Place, Projection, Rvalue,
    Statement, StatementKind, Terminator, TerminatorKind, lower::Mir,
};

pub type AliasId = StatementId;

#[derive(Clone, Debug)]
pub struct Alias {
    /// The unique Id of the Alias.
    /// This is equivalent to the statement id which led to its creation
    pub id: AliasId,
    /// Set of registers s.t. each register in the set held this alias at some
    /// point. For mutable borrows, this is of size 1. For immutable borrows,
    /// it can be greater than 1 due to copying.
    pub attached: HashSet<Register>,
    /// Register which owns the data Alias refers to
    pub borrows: Register,
    /// The kind of borrow (&/&mut)
    pub kind: Mutability,
    /// Whether this alias was introduced in a with-stmt, which changes the
    /// behavior of borrow from NLL to scoped lifetimes.
    pub with_lend: bool,
}

/// Information about lifetimes for a [`Body`]
#[derive(Default, Debug)]
pub struct Lifetimes {
    pub aliases: HashMap<AliasId, Alias>,
    pub live_ranges: HashMap<AliasId, HashMap<BasicBlock, Range<usize>>>,
}

/// Variables (and fields or indicies) allow the extension
/// of the lifetime of Aliases.
/// For this Register, which Alias does it currently hold?
type HeldAliases = HashMap<Register, AliasId>;
type HeldAliasesLattice = lattice::Lattice<BasicBlock, HeldAliases>;

/// Which Aliases are currently alive?
type LiveAliasSet = HashSet<AliasId>;
type LiveAliasLattice = lattice::Lattice<BasicBlock, LiveAliasSet>;

pub fn compute(mir: &Mir) -> HashMap<(DefId, Option<AnyMode>), Lifetimes> {
    mir.bodies
        .iter()
        .map(|(&key, body)| (key, compute_lifetimes(body)))
        .collect()
}

pub fn compute_lifetimes(body: &Body) -> Lifetimes {
    let mut aliases = collect_alias_births(body);
    let held_aliases = compute_held_aliases(body);
    record_attached_locals(body, &held_aliases, &mut aliases);
    let live_aliases = compute_live_aliases(body, &held_aliases, &aliases);
    let live_ranges = compute_live_ranges(body, &held_aliases, &live_aliases, &aliases);
    Lifetimes {
        aliases,
        live_ranges,
    }
}

fn place_needs_deref(place: &Place) -> bool {
    place
        .projections
        .iter()
        .any(|projection| matches!(projection, Projection::Deref))
}

fn creates_alias(stmt: &Statement) -> bool {
    matches!(&stmt.kind, StatementKind::Assign(_, Rvalue::Ref { .. }))
}

fn remove_local_from_held_aliases(held: &mut HeldAliases, local: Local) {
    held.retain(|register, _| register.owner != local);
}

fn remove_register_from_held_aliases(held: &mut HeldAliases, register: &Register) {
    if register.subregister.is_empty() {
        remove_local_from_held_aliases(held, register.owner);
    } else {
        held.remove(register);
    }
}

fn collect_alias_births(body: &Body) -> HashMap<AliasId, Alias> {
    let mut aliases = HashMap::new();
    for block in &body.basic_blocks {
        let mut pending_with_lend = HashSet::new();
        for stmt in &block.statements {
            if let StatementKind::WithLend(local) = stmt.kind {
                // Keep track of which are introduced with with blocks
                pending_with_lend.insert(local);
                continue;
            }

            if let StatementKind::Assign(
                place,
                Rvalue::Ref {
                    mutability,
                    place: borrowed,
                },
            ) = &stmt.kind
            {
                aliases.insert(
                    stmt.id,
                    Alias {
                        id: stmt.id,
                        attached: HashSet::new(),
                        borrows: register_of(borrowed),
                        kind: *mutability,
                        with_lend: pending_with_lend.remove(&place.local),
                    },
                );
            }
        }
    }
    aliases
}

fn compute_held_aliases(body: &Body) -> HeldAliasesLattice {
    let mut lattice: HeldAliasesLattice = Default::default();
    let preds = body.predecessors();

    for index in 0..body.basic_blocks.len() {
        let id = BasicBlock::from_usize(index);
        lattice.set_entry(id, HeldAliases::default());
        lattice.set_exit(id, HeldAliases::default());
    }

    let mut changed = true;
    while changed {
        changed = false;

        for (index, block) in body.basic_blocks.iter().enumerate() {
            let id = BasicBlock::from_usize(index);

            // Use meet on the predecessors
            let pred_states: Vec<&HeldAliases> = preds
                .of(id)
                .iter()
                .filter_map(|&pred| lattice.exit(pred))
                .collect();
            let old_entry = lattice
                .entry(id)
                .expect("every block's entry is given above")
                .clone();
            let new_entry = meet_held_aliases(&pred_states);
            lattice.set_entry(id, new_entry.clone());

            // Update changed if so
            if old_entry != new_entry {
                changed = true;
            }

            // Now the transfer functions
            let mut new_exit = new_entry;
            for stmt in &block.statements {
                apply_statement_to_held_aliases(&mut new_exit, stmt);
            }
            apply_terminator_to_held_aliases(&mut new_exit, &block.terminator);

            // Update changed if so
            let old_exit = lattice.exit(id).expect("every block's exit is given above");
            if *old_exit != new_exit {
                changed = true;
            }
            lattice.set_exit(id, new_exit);
        }
    }

    lattice
}

/// meet for calculating held aliases
fn meet_held_aliases(predecessor_states: &[&HeldAliases]) -> HeldAliases {
    let mut iter = predecessor_states.iter();
    let Some(first) = iter.next() else {
        return HeldAliases::new();
    };
    let mut agreed = (*first).clone();
    for state in iter {
        agreed.retain(|local, alias| state.get(local) == Some(alias));
    }
    agreed
}

/// Transfer function for the lattice calcuating held aliases
fn apply_statement_to_held_aliases(held: &mut HeldAliases, stmt: &Statement) {
    match &stmt.kind {
        StatementKind::StorageLive(local) | StatementKind::StorageDead(local) => {
            remove_local_from_held_aliases(held, *local);
        }
        StatementKind::Assign(place, rvalue) => {
            let place_register = register_of(place);

            match rvalue {
                Rvalue::Ref { .. } => {
                    held.insert(place_register, stmt.id);
                }
                // Below is for copies and moves of already held borrows
                Rvalue::Use(Operand::Copy(src)) => {
                    // Check whether what we are assigning with is a borrow
                    // that already has been linked
                    let held_by_src = held.get(&register_of(src)).copied();
                    match held_by_src {
                        Some(alias) => {
                            // Reassign to the new borrow
                            held.insert(place_register, alias);
                        }
                        None => {
                            // "Reassign" to new value that is not a borrow
                            // can be basically thought of as a no-op
                            remove_register_from_held_aliases(held, &place_register);
                        }
                    }
                }
                Rvalue::Use(Operand::Move(src)) => {
                    let held_by_src = held.remove(&register_of(src));
                    match held_by_src {
                        Some(alias) => {
                            held.insert(place_register, alias);
                        }
                        None => {
                            // "Reassign" to new value that is not a borrow
                            // can be basically thought of as a no-op
                            remove_register_from_held_aliases(held, &place_register);
                        }
                    }
                }
                _ => {
                    remove_register_from_held_aliases(held, &place_register);
                }
            }
        }
        StatementKind::PlaceMention(_)
        | StatementKind::SetDiscriminant { .. }
        | StatementKind::WithLend(_) => {}
    }
}

/// Transfer function for the lattice calcuating held aliases
/// This is the part that deals with the terminator
fn apply_terminator_to_held_aliases(held: &mut HeldAliases, terminator: &Terminator) {
    match &terminator.kind {
        TerminatorKind::Call { destination, .. } => {
            remove_register_from_held_aliases(held, &register_of(destination));
        }
        TerminatorKind::Drop { place, .. } | TerminatorKind::DropIso { place, .. } => {
            remove_register_from_held_aliases(held, &register_of(place));
        }
        TerminatorKind::Goto { .. }
        | TerminatorKind::SwitchInt { .. }
        | TerminatorKind::Assert { .. }
        | TerminatorKind::Return
        | TerminatorKind::Unreachable => {}
    }
}

fn record_attached_locals(
    body: &Body,
    held_aliases: &HeldAliasesLattice,
    aliases: &mut HashMap<AliasId, Alias>,
) {
    for (index, block) in body.basic_blocks.iter().enumerate() {
        let id = BasicBlock::from_usize(index);
        let entry = held_aliases
            .entry(id)
            .expect("every block's entry is given above");
        let mut states = held_aliases_before_each_statement(entry, block);
        states.push(
            held_aliases
                .exit(id)
                .expect("every block's exit is given above")
                .clone(),
        );
        // map the alias id's to the actual alias instance for each state
        for state in &states {
            for (register, &alias_id) in state {
                if let Some(alias) = aliases.get_mut(&alias_id) {
                    alias.attached.insert(register.clone());
                }
            }
        }
    }
}

fn held_aliases_before_each_statement(
    entry: &HeldAliases,
    block: &BasicBlockData,
) -> Vec<HeldAliases> {
    let mut states = Vec::with_capacity(block.statements.len() + 1);
    let mut current = entry.clone();
    states.push(current.clone());
    for stmt in &block.statements {
        apply_statement_to_held_aliases(&mut current, stmt);
        states.push(current.clone());
    }
    states
}

/// Marks every alias that `place` reads through as used at this point, chasing as many
/// `Deref` projections as needed. Each `Deref` crosses into a different allocation, so the
/// register accumulated so far (`place.local` plus the `Field`/`ConstantIndex` projections
/// seen before that `Deref`) is looked up in `held`; if it currently holds an alias, that
/// alias is marked used and the walk continues from `alias.borrows` (the register the alias
/// itself points into) with the remaining projections. If `held` has nothing at that
/// register, the chain is broken -- there is no alias to chase into for the rest of the
/// projections, so the walk stops instead of folding them onto the stale register.
fn mark_place_alias_used(
    aliases: &HashMap<AliasId, Alias>,
    held: &HeldAliases,
    place: &Place,
    used: &mut LiveAliasSet,
) {
    let mut register = Register {
        owner: place.local,
        subregister: Vec::new(),
    };
    for &projection in &place.projections {
        match projection {
            Projection::Field(n) => register.subregister.push(SubRegisters::Field(n)),
            Projection::ConstantIndex(n) => {
                register.subregister.push(SubRegisters::ConstantIndex(n))
            }
            Projection::Downcast(_) | Projection::Index(_) => {}
            Projection::Deref => {
                let Some(&alias_id) = held.get(&register) else {
                    return;
                };
                used.insert(alias_id);
                register = aliases[&alias_id].borrows.clone();
            }
        }
    }

    if let Some(&alias_id) = held.get(&register) {
        used.insert(alias_id);
    }
}

fn mark_operand_alias_used(
    aliases: &HashMap<AliasId, Alias>,
    held: &HeldAliases,
    operand: &Operand,
    used: &mut LiveAliasSet,
) {
    match operand {
        Operand::Copy(place) | Operand::Move(place) => {
            mark_place_alias_used(aliases, held, place, used)
        }
        Operand::Constant(_) => {}
    }
}

fn mark_rvalue_aliases_used(
    aliases: &HashMap<AliasId, Alias>,
    held: &HeldAliases,
    rvalue: &Rvalue,
    used: &mut LiveAliasSet,
) {
    match rvalue {
        Rvalue::Use(operand)
        | Rvalue::UnaryOp(_, operand)
        | Rvalue::Cast { operand, .. }
        | Rvalue::New(operand)
        | Rvalue::Unsize { operand, .. } => {
            mark_operand_alias_used(aliases, held, operand, used);
        }
        Rvalue::BinaryOp(_, lhs, rhs)
        | Rvalue::CheckedBinaryOp(_, lhs, rhs)
        | Rvalue::NewArray {
            elem: lhs,
            count: rhs,
        } => {
            mark_operand_alias_used(aliases, held, lhs, used);
            mark_operand_alias_used(aliases, held, rhs, used);
        }
        Rvalue::Aggregate(_, operands) => {
            for operand in operands {
                mark_operand_alias_used(aliases, held, operand, used);
            }
        }
        Rvalue::Ref { place, .. } | Rvalue::Discriminant(place) | Rvalue::Len(place) => {
            mark_place_alias_used(aliases, held, place, used);
        }
    }
}

fn mark_statement_aliases_used(
    aliases: &HashMap<AliasId, Alias>,
    held: &HeldAliases,
    stmt: &Statement,
    used: &mut LiveAliasSet,
) {
    match &stmt.kind {
        StatementKind::Assign(place, rvalue) => {
            if place_needs_deref(place) {
                mark_place_alias_used(aliases, held, place, used);
            }
            mark_rvalue_aliases_used(aliases, held, rvalue, used);
        }
        StatementKind::PlaceMention(place) => {
            mark_place_alias_used(aliases, held, place, used);
        }
        StatementKind::SetDiscriminant { place, .. } => {
            mark_place_alias_used(aliases, held, place, used)
        }
        StatementKind::StorageLive(_) | StatementKind::StorageDead(_) => {}
        StatementKind::WithLend(_) => {}
    }
}

fn mark_terminator_aliases_used(
    aliases: &HashMap<AliasId, Alias>,
    held: &HeldAliases,
    terminator: &Terminator,
    used: &mut LiveAliasSet,
) {
    match &terminator.kind {
        TerminatorKind::SwitchInt { discr, .. } => {
            mark_operand_alias_used(aliases, held, discr, used)
        }
        TerminatorKind::Assert { cond, msg, .. } => {
            mark_operand_alias_used(aliases, held, cond, used);
            if let Some(msg) = msg.user_message() {
                mark_operand_alias_used(aliases, held, msg, used);
            }
        }
        TerminatorKind::Call {
            func,
            args,
            destination,
            ..
        } => {
            mark_operand_alias_used(aliases, held, func, used);
            for arg in args {
                mark_operand_alias_used(aliases, held, arg, used);
            }
            if place_needs_deref(destination) {
                mark_place_alias_used(aliases, held, destination, used);
            }
        }
        TerminatorKind::Drop { place, .. } | TerminatorKind::DropIso { place, .. } => {
            mark_place_alias_used(aliases, held, place, used)
        }
        TerminatorKind::Goto { .. } | TerminatorKind::Return | TerminatorKind::Unreachable => {}
    }
}

fn compute_live_aliases(
    body: &Body,
    held_aliases: &HeldAliasesLattice,
    aliases: &HashMap<AliasId, Alias>,
) -> LiveAliasLattice {
    let mut lattice: LiveAliasLattice = Default::default();

    for index in 0..body.basic_blocks.len() {
        let id = BasicBlock::from_usize(index);
        lattice.set_entry(id, LiveAliasSet::default());
        lattice.set_exit(id, LiveAliasSet::default());
    }

    let mut changed = true;
    while changed {
        changed = false;

        for (index, block) in body.basic_blocks.iter().enumerate() {
            let id = BasicBlock::from_usize(index);

            // We are going backwards, check the successors of the block
            // to get our value for the exit value of the block
            let succ_states: Vec<&LiveAliasSet> = body
                .successors(id)
                .filter_map(|succ| lattice.entry(succ))
                .collect();
            let old_exit = lattice
                .exit(id)
                .expect("every block's exit is given above")
                .clone();
            let new_exit = meet_live_aliases(&succ_states);
            lattice.set_exit(id, new_exit.clone());

            // Update changed if so
            if old_exit != new_exit {
                changed = true;
            }

            let entry_held = held_aliases
                .entry(id)
                .expect("every block's entry is given above");
            let held_states = held_aliases_before_each_statement(entry_held, block);

            let mut current = new_exit;
            mark_terminator_aliases_used(
                aliases,
                &held_states[block.statements.len()],
                &block.terminator,
                &mut current,
            );
            for (index, stmt) in block.statements.iter().enumerate().rev() {
                if creates_alias(stmt) {
                    current.remove(&stmt.id);
                }
                mark_statement_aliases_used(aliases, &held_states[index], stmt, &mut current);
            }

            let old_entry = lattice
                .entry(id)
                .expect("every block's entry is given above");
            if *old_entry != current {
                changed = true;
            }
            lattice.set_entry(id, current);
        }
    }

    lattice
}

/// meet for live aliases
fn meet_live_aliases(states: &[&LiveAliasSet]) -> LiveAliasSet {
    let mut merged = LiveAliasSet::new();
    for state in states {
        merged.extend((*state).iter().copied());
    }
    merged
}

fn live_aliases_before_each_statement(
    exit: &LiveAliasSet,
    held_states: &[HeldAliases],
    block: &BasicBlockData,
    aliases: &HashMap<AliasId, Alias>,
) -> Vec<LiveAliasSet> {
    let mut before_each = vec![LiveAliasSet::default(); block.statements.len() + 1];
    let mut current = exit.clone();
    mark_terminator_aliases_used(
        aliases,
        &held_states[block.statements.len()],
        &block.terminator,
        &mut current,
    );
    before_each[block.statements.len()] = current.clone();
    for (index, stmt) in block.statements.iter().enumerate().rev() {
        if creates_alias(stmt) {
            current.remove(&stmt.id);
            before_each[index + 1].insert(stmt.id);
        }
        mark_statement_aliases_used(aliases, &held_states[index], stmt, &mut current);
        before_each[index] = current.clone();
    }
    before_each
}

fn compute_live_ranges(
    body: &Body,
    held_aliases: &HeldAliasesLattice,
    live_aliases: &LiveAliasLattice,
    aliases: &HashMap<AliasId, Alias>,
) -> HashMap<AliasId, HashMap<BasicBlock, Range<usize>>> {
    let mut live_ranges: HashMap<AliasId, HashMap<BasicBlock, Range<usize>>> = HashMap::new();

    for (index, block) in body.basic_blocks.iter().enumerate() {
        let id = BasicBlock::from_usize(index);
        let entry_held = held_aliases
            .entry(id)
            .expect("every block's entry is given above");
        let held_states = held_aliases_before_each_statement(entry_held, block);
        let exit_live = live_aliases
            .exit(id)
            .expect("every block's exit is given above");
        let live_states =
            live_aliases_before_each_statement(exit_live, &held_states, block, aliases);

        for alias in aliases.values() {
            let mut start = None;
            let mut end = 0;
            for (point, held_state) in held_states.iter().enumerate() {
                let alive = if alias.with_lend {
                    held_state.values().any(|&holder| holder == alias.id)
                } else {
                    live_states[point].contains(&alias.id)
                };
                if alive {
                    start.get_or_insert(point);
                    end = point + 1;
                }
            }
            if let Some(start) = start {
                live_ranges
                    .entry(alias.id)
                    .or_default()
                    .insert(id, start..end);
            }
        }
    }

    live_ranges
}

#[cfg(test)]
mod tests {
    use crate::ast::Mutability;
    use crate::ast::interner::Interner;
    use crate::diagnostics::DiagCtx;
    use crate::hir::Hir;
    use crate::mir::BasicBlock;
    use crate::mir::checks::borrowck::lifetimes::compute_lifetimes;
    use crate::mir::lower::Mir;
    use crate::testing::{first_function, lower_to_hir_with_ops};

    fn lower_mir_src(src: &str) -> (Hir, Mir) {
        let hir = lower_to_hir_with_ops(src);
        DiagCtx::clear();
        let checked = crate::typeck::check(&hir);
        let diagnostics = DiagCtx::diagnostics();
        assert!(
            diagnostics.is_empty(),
            "unexpected diagnostics for {src:?}: {diagnostics:?}"
        );
        let crate::typeck::TypeckOutput { mut tcx, types } = checked;
        let program =
            crate::mir::lower::lower(&hir, &mut tcx, &types, crate::driver::cli::Mode::Debug);
        (hir, program)
    }

    fn first_function_body<'a>(program: &'a Mir, hir: &Hir) -> &'a crate::mir::Body {
        let def_id = first_function(hir);
        program
            .bodies
            .get(&(def_id, None))
            .unwrap_or_else(|| panic!("no lowered body for the first function"))
    }

    #[test]
    fn a_borrow_used_once_right_after_its_birth_is_alive_for_exactly_those_two_statements() {
        let (hir, program) = lower_mir_src("fun f(a: i32) { let r = &a; let _ = *r; }");
        let body = first_function_body(&program, &hir);
        let lifetimes = compute_lifetimes(body);
        assert_eq!(lifetimes.aliases.len(), 1);
        let alias = lifetimes.aliases.values().next().unwrap();
        assert!(!alias.with_lend);
        assert_eq!(alias.kind, Mutability::Immutable);
        let ranges = &lifetimes.live_ranges[&alias.id];
        let range = ranges.get(&BasicBlock::from_usize(0)).unwrap();
        assert_eq!(range.end - range.start, 2);
    }

    #[test]
    fn a_borrow_never_used_again_dies_right_after_its_birth_statement() {
        let (hir, program) = lower_mir_src("fun f(a: i32) { let r = &a; let _ = 1; let _ = 2; }");
        let body = first_function_body(&program, &hir);
        let lifetimes = compute_lifetimes(body);
        let alias = lifetimes.aliases.values().next().unwrap();
        let range = lifetimes.live_ranges[&alias.id]
            .get(&BasicBlock::from_usize(0))
            .unwrap();
        assert_eq!(range.end - range.start, 1);
    }

    #[test]
    fn a_with_lend_alias_stays_alive_through_the_end_of_the_with_block_with_no_more_uses() {
        let (hir, program) = lower_mir_src(
            "fun f(a: i32) {
                 with r = &a {
                     let _ = *r;
                     let _ = 1;
                     let _ = 2;
                 }
             }",
        );
        let body = first_function_body(&program, &hir);
        let lifetimes = compute_lifetimes(body);
        let alias = lifetimes
            .aliases
            .values()
            .find(|alias| alias.with_lend)
            .expect("the with lend produced a with-scoped alias");
        let range = lifetimes.live_ranges[&alias.id]
            .get(&BasicBlock::from_usize(0))
            .unwrap();
        assert!(range.end - range.start > 2);
    }

    #[test]
    fn reassigning_a_with_lend_local_manually_does_not_extend_the_new_borrows_lifetime() {
        let (hir, program) = lower_mir_src(
            "fun f(a: i32, b: i32) {
                 with r = &a {
                     let _ = *r;
                     r = &b;
                     let _ = *r;
                     let _ = 1;
                     let _ = 2;
                 }
             }",
        );
        let body = first_function_body(&program, &hir);
        let lifetimes = compute_lifetimes(body);
        let mut with_lend_flags: Vec<bool> = lifetimes
            .aliases
            .values()
            .map(|alias| alias.with_lend)
            .collect();
        with_lend_flags.sort();
        assert_eq!(
            with_lend_flags,
            vec![false, true],
            "only the with clause's own initializer is with-scoped, not the later reassignment"
        );
        let reassigned = lifetimes
            .aliases
            .values()
            .find(|alias| !alias.with_lend)
            .unwrap();
        let range = lifetimes.live_ranges[&reassigned.id]
            .get(&BasicBlock::from_usize(0))
            .unwrap();
        assert_eq!(
            range.end - range.start,
            2,
            "the reassigned borrow follows NLL and dies right after its one use"
        );
    }

    #[test]
    fn a_shared_borrow_copied_into_another_local_attaches_to_both() {
        let (hir, program) = lower_mir_src(
            "fun f(a: i32) {
                 let r = &a;
                 let s = r;
                 let _ = *r;
                 let _ = *s;
             }",
        );
        let body = first_function_body(&program, &hir);
        let lifetimes = compute_lifetimes(body);
        let alias = lifetimes.aliases.values().next().unwrap();
        assert_eq!(alias.attached.len(), 2);
        let name_of = |register: &crate::mir::checks::borrowck::Register| {
            body.local_decls[register.owner.index()]
                .name
                .map(|name| Interner::resolve(name.text).to_string())
        };
        let mut names: Vec<_> = alias.attached.iter().filter_map(name_of).collect();
        names.sort();
        assert_eq!(names, vec!["r".to_string(), "s".to_string()]);
    }

    #[test]
    fn a_mutable_borrow_moved_into_another_local_transfers_rather_than_duplicates() {
        let (hir, program) = lower_mir_src(
            "fun f(a: i32) {
                 let mut a = a;
                 let r = &mut a;
                 let s = r;
                 let _ = *s;
             }",
        );
        let body = first_function_body(&program, &hir);
        let lifetimes = compute_lifetimes(body);
        let alias = lifetimes.aliases.values().next().unwrap();
        assert_eq!(alias.kind, Mutability::Mutable);
        assert_eq!(alias.attached.len(), 2);
    }

    #[test]
    fn a_borrow_of_one_struct_field_is_tracked_as_a_field_precise_alias() {
        let (hir, program) = lower_mir_src(
            "struct P { x: i32, y: i32 }
             fun f(p: P) {
                 let r = &p.x;
                 let _ = *r;
             }",
        );
        let body = first_function_body(&program, &hir);
        let lifetimes = compute_lifetimes(body);
        assert_eq!(
            lifetimes.aliases.len(),
            1,
            "a field borrow is now a tracked alias, not invisible to this pass"
        );
        let alias = lifetimes.aliases.values().next().unwrap();
        assert!(
            !alias.borrows.subregister.is_empty(),
            "the alias remembers which field it borrows from, not just the owning local"
        );
    }

    #[test]
    fn borrows_of_disjoint_struct_fields_are_independent_aliases() {
        let (hir, program) = lower_mir_src(
            "struct P { x: i32, y: i32 }
             fun f(p: P) {
                 let rx = &p.x;
                 let ry = &p.y;
                 let _ = *rx;
                 let _ = *ry;
             }",
        );
        let body = first_function_body(&program, &hir);
        let lifetimes = compute_lifetimes(body);
        assert_eq!(
            lifetimes.aliases.len(),
            2,
            "borrowing two disjoint fields of the same local produces two independent aliases"
        );
        let borrowed_registers: Vec<_> = lifetimes
            .aliases
            .values()
            .map(|alias| alias.borrows.subregister.clone())
            .collect();
        assert_ne!(
            borrowed_registers[0], borrowed_registers[1],
            "the two aliases are keyed to different fields, not merged onto the whole local"
        );
    }

    #[test]
    fn a_borrow_of_an_array_element_is_still_tracked_but_collapses_to_the_whole_array() {
        let (hir, program) = lower_mir_src(
            "fun f(a: [i32; 4], i: i32) {
                 let r = &a[i];
                 let _ = *r;
             }",
        );
        let body = first_function_body(&program, &hir);
        let lifetimes = compute_lifetimes(body);
        assert_eq!(
            lifetimes.aliases.len(),
            1,
            "an index borrow is still tracked, unlike a Deref-involving place"
        );
        let alias = lifetimes.aliases.values().next().unwrap();
        assert!(
            alias.borrows.subregister.is_empty(),
            "a runtime index isn't representable as a SubRegisters entry, so the borrow \
             collapses to the whole array's register rather than a slot-precise one"
        );
    }

    #[test]
    fn borrows_of_two_different_array_elements_are_not_told_apart() {
        let (hir, program) = lower_mir_src(
            "fun f(a: [i32; 4], i: i32, j: i32) {
                 let rx = &a[i];
                 let ry = &a[j];
                 let _ = *rx;
                 let _ = *ry;
             }",
        );
        let body = first_function_body(&program, &hir);
        let lifetimes = compute_lifetimes(body);
        assert_eq!(
            lifetimes.aliases.len(),
            2,
            "a[i] and a[j] are still two separate borrow expressions, so two aliases"
        );
        let borrowed_registers: Vec<_> = lifetimes
            .aliases
            .values()
            .map(|alias| alias.borrows.clone())
            .collect();
        assert_eq!(
            borrowed_registers[0], borrowed_registers[1],
            "unlike p.x vs p.y, a[i] and a[j] collapse onto the exact same register: the \
             compiler can't prove i != j, so it can't tell these two borrows apart"
        );
    }

    #[test]
    fn a_literal_array_index_is_more_precise_than_a_variable_one() {
        let (hir, program) = lower_mir_src(
            "fun f(a: [i32; 4]) {
                 let rx = &a[0];
                 let ry = &a[1];
                 let _ = *rx;
                 let _ = *ry;
             }",
        );
        let body = first_function_body(&program, &hir);
        let lifetimes = compute_lifetimes(body);
        let borrowed_registers: Vec<_> = lifetimes
            .aliases
            .values()
            .map(|alias| alias.borrows.clone())
            .collect();
        assert_ne!(
            borrowed_registers[0], borrowed_registers[1],
            "a[0] and a[1] lower through Projection::ConstantIndex, which register_of carries \
             into distinct SubRegisters::ConstantIndex entries -- unlike a[i], these two borrows \
             are told apart as disjoint slots of a"
        );
    }
}
