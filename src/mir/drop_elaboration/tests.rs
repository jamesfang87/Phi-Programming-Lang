use std::collections::HashMap;

use crate::ast::interner::Interner;
use crate::hir::Hir;
use crate::mir::{
    BasicBlock, Body, ConstKind, Instance, Local, Operand, Place, Projection, Rvalue,
    StatementKind, SwitchTargets, TerminatorKind, VariantIdx,
};
use crate::testing::{OPS_PREAMBLE, lower_mir_src_files, lower_to_mir, named_def};
use crate::typeck::tyctx::TyCtx;

/// Monomorphizes `src`, then runs drop elaboration over every resulting `Body`.
fn elaborated(src: &str) -> (Hir, TyCtx, HashMap<Instance, Body>) {
    let (hir, mut tcx, _types, _mir, instances) = lower_to_mir(src);
    let instances = super::elaborate_drops(&mut tcx, instances);
    (hir, tcx, instances)
}

/// Like [`elaborated`], but with [`OPS_PREAMBLE`] compiled alongside `src` -- `while`'s condition
/// desugars to `if !cond { break }`, which needs a real `Not` (and `Copy`) impl on `bool` to exist
/// to type check at all, so any fixture using `while` needs this rather than plain [`elaborated`].
fn elaborated_with_ops(src: &str) -> (Hir, TyCtx, HashMap<Instance, Body>) {
    let (hir, mut tcx, _types, _mir, instances) = lower_mir_src_files(&[OPS_PREAMBLE, src]);
    let instances = super::elaborate_drops(&mut tcx, instances);
    (hir, tcx, instances)
}

/// The `Body` of the (non-generic, non-`any`) instance of the top-level definition named `name`.
fn body_for<'a>(instances: &'a HashMap<Instance, Body>, hir: &Hir, name: &str) -> &'a Body {
    let def = named_def(hir, name);
    instances
        .iter()
        .find(|(instance, _)| instance.def == def && instance.args.is_empty())
        .map(|(_, body)| body)
        .unwrap_or_else(|| panic!("no elaborated instance for {name:?}"))
}

/// `body`'s own local declared with source name `name`.
fn named_local(body: &Body, name: &str) -> Local {
    body.local_decls
        .iter()
        .position(|decl| decl.name.is_some_and(|n| Interner::resolve(n.text) == name))
        .map(Local::from_usize)
        .unwrap_or_else(|| panic!("no local named {name:?} in {body:?}"))
}

/// Every place a `TerminatorKind::Drop` names, visited by walking the CFG from the start block
/// along each terminator's (assumed single, since these fixtures are all straight-line) successor
/// -- not by `basic_blocks`' own vec order, which need not match execution order once blocks have
/// been split.
fn linear_drop_order(body: &Body) -> Vec<Local> {
    let mut order = Vec::new();
    let mut current = BasicBlock::START_BLOCK;
    let mut visited = std::collections::HashSet::new();
    loop {
        if !visited.insert(current) {
            break;
        }
        let block = &body.basic_blocks[current.index()];
        if let TerminatorKind::Drop { place, target } = &block.terminator.kind {
            order.push(place.local);
            current = *target;
            continue;
        }
        let mut successors = block.terminator.successors();
        let Some(next) = successors.next() else {
            break;
        };
        current = next;
    }
    order
}

/// Whether `body` contains a `TerminatorKind::Drop` for `local` immediately followed (in its
/// target block) by that same local's `StorageDead` as the first statement -- i.e. the drop was
/// spliced in exactly at the local's existing end-of-scope point, not somewhere else.
fn drops_right_before_storage_dead(body: &Body, local: Local) -> bool {
    body.basic_blocks.iter().any(|block| {
        let TerminatorKind::Drop { place, target } = &block.terminator.kind else {
            return false;
        };
        if place.local != local {
            return false;
        }
        let target_block = &body.basic_blocks[target.index()];
        matches!(
            target_block.statements.first().map(|s| &s.kind),
            Some(StatementKind::StorageDead(dead)) if *dead == local
        )
    })
}

#[test]
fn a_local_of_a_type_that_does_not_need_dropping_gets_no_drop_terminator() {
    let (hir, _tcx, instances) = elaborated("fun f() { let x = 1; }");
    let body = body_for(&instances, &hir, "f");
    assert!(
        linear_drop_order(body).is_empty(),
        "a plain i32 local must not be dropped: {body:?}"
    );
}

#[test]
fn an_owned_iso_local_is_dropped_right_before_its_storage_dead() {
    let (hir, _tcx, instances) = elaborated("fun f() { let x = new 1; }");
    let body = body_for(&instances, &hir, "f");
    let x = named_local(body, "x");
    assert_eq!(
        linear_drop_order(body),
        vec![x],
        "the only local in scope, `x`, owns an `iso` value and is never moved, so it must be \
         dropped exactly once"
    );
    assert!(
        drops_right_before_storage_dead(body, x),
        "the drop must be spliced in immediately before `x`'s own StorageDead: {body:?}"
    );
}

#[test]
fn an_iso_local_moved_out_by_return_is_not_dropped() {
    let (hir, _tcx, instances) = elaborated("fun f() -> iso i32 { let x = new 1; return x; }");
    let body = body_for(&instances, &hir, "f");
    assert!(
        linear_drop_order(body).is_empty(),
        "`x` is moved into the return place, so it no longer owns the value by the time its \
         StorageDead runs: {body:?}"
    );
}

#[test]
fn two_iso_locals_are_dropped_in_the_reverse_of_their_declaration_order() {
    let (hir, _tcx, instances) = elaborated(
        "fun f() {
             let a = new 1;
             let b = new 2;
         }",
    );
    let body = body_for(&instances, &hir, "f");
    let a = named_local(body, "a");
    let b = named_local(body, "b");
    assert_eq!(
        linear_drop_order(body),
        vec![b, a],
        "drops run in the same reverse-declaration order their StorageDead statements already \
         do: {body:?}"
    );
}

#[test]
fn an_iso_parameter_never_moved_is_dropped_at_function_exit() {
    let (hir, _tcx, instances) = elaborated("fun f(p: iso i32) { }");
    let body = body_for(&instances, &hir, "f");
    let p = named_local(body, "p");
    assert_eq!(
        linear_drop_order(body),
        vec![p],
        "`f` never moves `p` anywhere, so it still owns the value when the function returns and \
         must be dropped: {body:?}"
    );
}

#[test]
fn an_iso_parameter_moved_into_a_call_is_not_dropped_by_its_caller_scope() {
    let (hir, _tcx, instances) = elaborated(
        "fun consume(p: iso i32) { }
         fun f(p: iso i32) { consume(p); }",
    );
    let body = body_for(&instances, &hir, "f");
    assert!(
        linear_drop_order(body).is_empty(),
        "`f` moves `p` into `consume`, so `f` itself no longer owns it by the time `p`'s \
         StorageDead runs: {body:?}"
    );
}

#[test]
fn drop_elaboration_preserves_the_set_of_instances_monomorphize_produced() {
    let (_hir, mut tcx, _types, _mir, before) =
        lower_to_mir("fun f() { let x = new 1; } fun g() -> i32 { return 1; }");
    let before_keys: std::collections::HashSet<_> = before.keys().cloned().collect();
    let after = super::elaborate_drops(&mut tcx, before);
    let after_keys: std::collections::HashSet<_> = after.keys().cloned().collect();
    assert_eq!(
        before_keys, after_keys,
        "drop elaboration only rewrites each Body's statements/terminators, never the set of \
         instances monomorphize decided to emit"
    );
}

// -----------------------------------------------------------------
// Conditional drops (drop flags) and recursive (struct/enum field) drops
// -----------------------------------------------------------------
//
// The tests above only ever exercise a local that is either unconditionally moved out or
// unconditionally still owned by the time its `StorageDead` runs -- there is exactly one path
// through the body to check. The tests below exercise the two things that make drop elaboration
// its own pass rather than "insert an unconditional Drop before every StorageDead":
//
// 1. A local can be moved out on *one* branch of a conditional and not the other. Since MIR has
//    no phi nodes, there is no single static answer to "does this local still need dropping" at
//    the point the branches rejoin -- it depends on which branch actually ran. This pass must
//    thread a runtime drop flag (a synthesized `bool` local, set `true` where the value starts
//    being owned and `false` at each point it is moved out) through the rejoined control flow, and
//    replace the unconditional `Drop` with a `SwitchInt` on that flag: drop on one arm, skip on
//    the other.
// 2. A local's type can itself be a struct or enum that doesn't need dropping as a whole, but
//    contains a field that does (e.g. a plain-by-value struct with an `iso` field, or an enum
//    where only one variant owns something). Dropping such a local means recursing into its
//    fields -- for a struct, an unconditional `Drop` on each field that needs it (with a `Place`
//    projected through `Projection::Field`); for an enum, a runtime switch on the value's own
//    discriminant (via `Rvalue::Discriminant`), dropping only the fields (through
//    `Projection::Downcast` then `Projection::Field`) of whichever variant turns out to be active.

/// Walks forward from `start` along whatever single successor each block's terminator has
/// (`Goto`, a `Call` with a continuation, another local's own `Drop`, ...), until either:
/// - a `Drop` terminator names `target_local` -- returns `Some` of that `Drop`'s own `Place`
///   (which may be `target_local` itself, or `target_local` projected into a field), without
///   continuing past it (a real elaboration only ever drops a given local once on any one path);
/// - `resumes_at` says the current block is where the walk's caller considers the question
///   settled without a drop having happened -- returns `None`.
///
/// Panics if a block along the way has anything other than exactly one successor (a branch, or a
/// dead end) before either of those is reached, since every fixture below reaches its own resume
/// point deterministically once the specific path being walked has already resolved which
/// branches were taken.
fn drop_place_before(
    body: &Body,
    mut current: BasicBlock,
    target_local: Local,
    resumes_at: impl Fn(&crate::mir::BasicBlockData) -> bool,
) -> Option<Place> {
    loop {
        let block = &body.basic_blocks[current.index()];
        if resumes_at(block) {
            return None;
        }
        if let TerminatorKind::Drop { place, .. } = &block.terminator.kind
            && place.local == target_local
        {
            return Some(place.clone());
        }
        let mut successors = block.terminator.successors();
        let next = successors.next().unwrap_or_else(|| {
            panic!("drop_place_before: dead end before reaching a resume point for {target_local:?}: {body:?}")
        });
        assert!(
            successors.next().is_none(),
            "drop_place_before: expected a single successor before a resume point for \
             {target_local:?}, found a branch in {block:?}: {body:?}"
        );
        current = next;
    }
}

/// [`drop_place_before`], resuming at `target_local`'s own `StorageDead` -- i.e. was it still
/// owned by the time its scope ended.
fn drop_place_before_storage_dead(
    body: &Body,
    current: BasicBlock,
    target_local: Local,
) -> Option<Place> {
    drop_place_before(body, current, target_local, |block| {
        block
            .statements
            .iter()
            .any(|s| matches!(&s.kind, StatementKind::StorageDead(l) if *l == target_local))
    })
}

/// [`drop_place_before`], resuming at a block whose first statement reassigns `target_local`
/// itself (a plain `Assign` to it, no projection) -- i.e. was the *old* value it held still owned
/// right before that assignment overwrites it. Unlike the `StorageDead` case, `target_local`'s own
/// very first assignment (its `let` initializer) is never itself such a resume point from the
/// walk's perspective; a fixture that starts the walk anywhere at or after that initializer will
/// only ever reach a later, genuine reassignment this way.
fn drop_place_before_reassignment(
    body: &Body,
    current: BasicBlock,
    target_local: Local,
) -> Option<Place> {
    drop_place_before(body, current, target_local, |block| {
        matches!(
            block.statements.first().map(|s| &s.kind),
            Some(StatementKind::Assign(p, _)) if p.local == target_local && p.projections.is_empty()
        )
    })
}

/// Whether `body` contains a `TerminatorKind::Drop` for `local` immediately followed (in its
/// target block) by a plain reassignment of `local` as that block's first statement -- i.e. the
/// old value `local` held was dropped right before being overwritten. Since `local`'s own `let`
/// initializer is always the *first* `Assign` to it in program order, and nothing should ever
/// precede that first assignment with a drop (there is no old value yet), any `Drop` this finds
/// can only be guarding a genuine reassignment.
fn drops_right_before_reassignment(body: &Body, local: Local) -> bool {
    body.basic_blocks.iter().any(|block| {
        let TerminatorKind::Drop { place, target } = &block.terminator.kind else {
            return false;
        };
        if place.local != local {
            return false;
        }
        let target_block = &body.basic_blocks[target.index()];
        matches!(
            target_block.statements.first().map(|s| &s.kind),
            Some(StatementKind::Assign(p, _)) if p.local == local && p.projections.is_empty()
        )
    })
}

/// A local this pass set directly from a literal `bool` constant somewhere in `body` -- the
/// signature of a synthesized drop flag (initialized `true` where a value starts being owned, set
/// `false` at each point it is moved out), as opposed to a local computed from a real comparison
/// (`Rvalue::BinaryOp`), which is how every condition the *source* itself writes (an `if`'s own
/// condition, a `while`'s own guard) gets its value instead.
fn is_bool_constant_flag_candidate(body: &Body, local: Local) -> bool {
    body.basic_blocks
        .iter()
        .flat_map(|b| &b.statements)
        .any(|s| {
            matches!(
                &s.kind,
                StatementKind::Assign(place, Rvalue::Use(Operand::Constant(c)))
                    if place.local == local
                        && place.projections.is_empty()
                        && matches!(c.kind, ConstKind::Bool(_))
            )
        })
}

/// The `SwitchInt` terminator in `body` whose `discr` reads a drop-flag candidate (per
/// [`is_bool_constant_flag_candidate`]), plus that switch's own block -- i.e. the switch drop
/// elaboration introduced itself to read a synthesized flag, distinct from any switch the
/// source's own `if`/`while`/`match` already lowered to (whose discr is never set from a literal
/// `bool` constant). `None` if the body needed no such flag at all.
fn find_flag_switch(body: &Body) -> Option<(BasicBlock, &SwitchTargets)> {
    let mut found = None;
    for (index, block) in body.basic_blocks.iter().enumerate() {
        if let TerminatorKind::SwitchInt { discr, targets } = &block.terminator.kind {
            let discr_local = match discr {
                Operand::Copy(p) | Operand::Move(p) => p.local,
                Operand::Constant(_) => continue,
            };
            if is_bool_constant_flag_candidate(body, discr_local) {
                assert!(
                    found.is_none(),
                    "expected at most one drop-flag switch, found a second in block {index}: \
                     {body:?}"
                );
                found = Some((BasicBlock::from_usize(index), targets));
            }
        }
    }
    found
}

/// [`find_flag_switch`], but panics rather than returning `None` -- for a fixture where the pass
/// has no choice but to synthesize a flag.
fn flag_switch(body: &Body) -> (BasicBlock, &SwitchTargets) {
    find_flag_switch(body)
        .unwrap_or_else(|| panic!("expected a synthesized drop-flag switch, found none: {body:?}"))
}

/// Every arm of `targets` (each explicit value's target, plus `otherwise`), each walked forward
/// via [`drop_place_before_storage_dead`] for `target_local`. Returns the arms that *do* drop it
/// (their own `Drop`'s `Place`) separately from the arms that reach `StorageDead` without
/// dropping it, so a test can assert the split is exactly one drop-arm versus the rest skip-arms,
/// without hard-coding which raw discriminant/flag value that drop-arm carries.
fn partition_switch_arms(
    body: &Body,
    targets: &SwitchTargets,
    target_local: Local,
) -> (Vec<Place>, usize) {
    let mut dropped = Vec::new();
    let mut skipped = 0;
    for target in targets
        .values
        .iter()
        .map(|(_, t)| *t)
        .chain([targets.otherwise])
    {
        match drop_place_before_storage_dead(body, target, target_local) {
            Some(place) => dropped.push(place),
            None => skipped += 1,
        }
    }
    (dropped, skipped)
}

/// [`partition_switch_arms`], but for a flag switch guarding a pre-reassignment drop (via
/// [`drop_place_before_reassignment`]) rather than one guarding an end-of-scope drop.
fn partition_switch_arms_before_reassignment(
    body: &Body,
    targets: &SwitchTargets,
    target_local: Local,
) -> (Vec<Place>, usize) {
    let mut dropped = Vec::new();
    let mut skipped = 0;
    for target in targets
        .values
        .iter()
        .map(|(_, t)| *t)
        .chain([targets.otherwise])
    {
        match drop_place_before_reassignment(body, target, target_local) {
            Some(place) => dropped.push(place),
            None => skipped += 1,
        }
    }
    (dropped, skipped)
}

/// The `SwitchInt` terminator in `body` whose `discr` reads `discr_local` directly, plus its
/// targets -- for finding the branch the *source*'s own `if`/`while` lowered to (as opposed to
/// [`find_flag_switch`], which finds one drop elaboration introduced).
fn find_switch_on(body: &Body, discr_local: Local) -> &SwitchTargets {
    body.basic_blocks
        .iter()
        .find_map(|block| match &block.terminator.kind {
            TerminatorKind::SwitchInt { discr, targets } => match discr {
                Operand::Copy(p) | Operand::Move(p) if p.local == discr_local => Some(targets),
                _ => None,
            },
            _ => None,
        })
        .unwrap_or_else(|| panic!("expected a SwitchInt reading {discr_local:?}: {body:?}"))
}

#[test]
fn an_iso_local_moved_on_only_one_arm_of_a_plain_if_is_not_dropped_on_that_arm() {
    // A plain `if` has exactly two, statically known predecessors feeding the point right after
    // it -- a correct implementation can resolve "was `x` moved" here either by open-coding the
    // drop separately into each of those two predecessors (no runtime state at all), or by
    // threading a synthesized flag through a shared join, same as the loop case below. Both are
    // valid; this test accepts either rather than pinning down which.
    let (hir, _tcx, instances) = elaborated(
        "fun consume(p: iso i32) { }
         fun f(cond: bool) {
             let x = new 1;
             if cond {
                 consume(x);
             }
         }",
    );
    let body = body_for(&instances, &hir, "f");
    let cond = named_local(body, "cond");
    let x = named_local(body, "x");

    let cond_targets = find_switch_on(body, cond);
    let moved_arm = cond_targets
        .values
        .iter()
        .find(|&&(value, _)| value == 1)
        .map(|&(_, target)| target)
        .expect("`if cond { .. }` switches on `cond == 1` for its then-arm");
    let not_moved_arm = cond_targets.otherwise;

    match find_flag_switch(body) {
        Some((_, flag_targets)) => {
            let (dropped, skipped) = partition_switch_arms(body, flag_targets, x);
            assert_eq!(
                dropped,
                vec![Place::from_local(x)],
                "exactly one arm of the synthesized drop-flag switch must drop `x`: {body:?}"
            );
            assert_eq!(
                skipped, 1,
                "the other arm (reached when `consume(x)` ran) must skip the drop: {body:?}"
            );
        }
        None => {
            assert_eq!(
                drop_place_before_storage_dead(body, moved_arm, x),
                None,
                "the arm that moved `x` into `consume` must never drop it: {body:?}"
            );
            assert_eq!(
                drop_place_before_storage_dead(body, not_moved_arm, x),
                Some(Place::from_local(x)),
                "the arm that never moved `x` must still drop it: {body:?}"
            );
        }
    }
}

#[test]
fn an_iso_local_conditionally_moved_inside_a_loop_needs_a_real_drop_flag() {
    // Unlike the plain `if` above, whether `x` was moved by the time the loop exits depends on
    // how many iterations ran -- there is no finite, statically enumerable set of predecessors to
    // open-code the drop into (the loop header's own back-edge predecessor carries the same
    // unresolved question recursively). This is exactly the case real drop elaboration (rustc's
    // included) reaches for an actual runtime flag for, rather than duplicating code per
    // predecessor: a value whose ownership state must survive across a loop's back edge.
    let (hir, _tcx, instances) = elaborated_with_ops(
        "fun consume(p: iso i32) { }
         fun f(n: i32) {
             let x = new 1;
             let mut i = 0;
             while i < n {
                 if i == 0 {
                     consume(x);
                 }
                 i = i + 1;
             }
         }",
    );
    let body = body_for(&instances, &hir, "f");
    let x = named_local(body, "x");

    let (_switch_block, targets) = flag_switch(body);
    let (dropped, skipped) = partition_switch_arms(body, targets, x);
    assert_eq!(
        dropped,
        vec![Place::from_local(x)],
        "one arm of the drop-flag switch must still drop `x`, for the case no iteration ever ran \
         `consume`: {body:?}"
    );
    assert_eq!(
        skipped, 1,
        "the other arm must skip the drop, for the case some iteration did: {body:?}"
    );
}

#[test]
fn dropping_a_struct_drops_only_the_field_that_needs_dropping() {
    let (hir, _tcx, instances) = elaborated(
        "struct Handle { public owned: iso i32, public tag: i32 }
         fun f() { let h = Handle { owned: new 1, tag: 0 }; }",
    );
    let body = body_for(&instances, &hir, "f");
    let h = named_local(body, "h");

    let dropped = drop_place_before_storage_dead(body, BasicBlock::START_BLOCK, h);
    assert_eq!(
        dropped,
        Some(Place {
            local: h,
            projections: vec![Projection::Field(0)],
        }),
        "`Handle` itself needs no drop glue (it is not `iso`), but its `owned` field does -- the \
         elaborated drop must target that field specifically, not the whole struct: {body:?}"
    );
}

#[test]
fn a_struct_with_no_field_that_needs_dropping_is_never_dropped() {
    let (hir, _tcx, instances) = elaborated(
        "struct Point { public x: i32, public y: i32 }
         fun f() { let p = Point { x: 1, y: 2 }; }",
    );
    let body = body_for(&instances, &hir, "f");
    let p = named_local(body, "p");
    assert_eq!(
        drop_place_before_storage_dead(body, BasicBlock::START_BLOCK, p),
        None,
        "neither of `Point`'s fields needs dropping, so `p` must reach its StorageDead with no \
         Drop terminator at all: {body:?}"
    );
}

#[test]
fn dropping_a_struct_with_two_owned_fields_drops_both() {
    let (hir, _tcx, instances) = elaborated(
        "struct Pair { public a: iso i32, public b: iso i32 }
         fun f() { let p = Pair { a: new 1, b: new 2 }; }",
    );
    let body = body_for(&instances, &hir, "f");
    let p = named_local(body, "p");

    // Both fields need separate `Drop` terminators; walk the whole body collecting every `Drop`
    // that projects into `p`, rather than assuming a fixed order between the two fields.
    let dropped_fields: std::collections::HashSet<u32> = body
        .basic_blocks
        .iter()
        .filter_map(|block| match &block.terminator.kind {
            TerminatorKind::Drop { place, .. } if place.local == p => match place.projections[..] {
                [Projection::Field(n)] => Some(n),
                _ => None,
            },
            _ => None,
        })
        .collect();
    assert_eq!(
        dropped_fields,
        std::collections::HashSet::from([0, 1]),
        "both of `Pair`'s owned fields must be dropped individually: {body:?}"
    );
}

#[test]
fn dropping_an_enum_only_drops_the_field_of_whichever_variant_is_actually_active() {
    let (hir, _tcx, instances) = elaborated(
        "enum Box_ { present: iso i32, absent }
         fun f(cond: bool) {
             let b: Box_ = if cond { .present(new 1) } else { .absent };
         }",
    );
    let body = body_for(&instances, &hir, "f");
    let cond = named_local(body, "cond");
    let b = named_local(body, "b");

    // The switch elaboration introduces must read `b`'s own discriminant, not a plain flag --
    // find the temp an `Rvalue::Discriminant(b)` was assigned into, then the switch reading it.
    let discr_temp = body
        .basic_blocks
        .iter()
        .flat_map(|block| &block.statements)
        .find_map(|s| match &s.kind {
            StatementKind::Assign(dest, Rvalue::Discriminant(place)) if place.local == b => {
                Some(dest.local)
            }
            _ => None,
        })
        .unwrap_or_else(|| {
            panic!("expected an Rvalue::Discriminant(b) read somewhere in {body:?}")
        });
    assert_ne!(
        discr_temp, cond,
        "the discriminant read must be of `b`, not a re-use of the unrelated `cond` parameter"
    );

    let targets = find_switch_on(body, discr_temp);
    let (dropped, skipped) = partition_switch_arms(body, targets, b);
    assert_eq!(
        dropped,
        vec![Place {
            local: b,
            projections: vec![
                Projection::Downcast(VariantIdx::from_usize(0)),
                Projection::Field(0)
            ],
        }],
        "only the `.present` arm owns a value, and only that arm's own payload field (downcast \
         into variant 0, field 0) is dropped: {body:?}"
    );
    assert_eq!(
        skipped, 1,
        "the `.absent` arm owns nothing, so it must reach `b`'s StorageDead with no drop at all: \
         {body:?}"
    );
}

// -----------------------------------------------------------------
// Drops before reassignment
// -----------------------------------------------------------------
//
// Every test above only ever drops a local once, at the end of its scope. But a place can also
// lose its old value earlier, mid-scope, by being written over: `p = new_value;` for an already
// (possibly conditionally) initialized `p` overwrites whatever pointer `p` held with no help from
// `StorageDead` at all. If that old value still needed dropping and nothing runs its drop first,
// the old value's own resources become unreachable with nothing left pointing at them -- a leak,
// the mirror image of the double-free a *missing* moved-out check would cause. So a genuine
// reassignment (not a local's own first, initializing assignment -- there is nothing to drop
// there) needs a `Drop` spliced in immediately before it, gated on the exact same "was it moved by
// now" analysis as the end-of-scope case: unconditionally dropped if definitely still owned,
// skipped if definitely moved out already, and behind a synthesized flag if that disagrees across
// the reassignment's own predecessors.

#[test]
fn reassigning_an_owned_iso_local_drops_the_old_value_first() {
    let (hir, _tcx, instances) = elaborated(
        "fun f() {
             let mut x = new 1;
             x = new 2;
         }",
    );
    let body = body_for(&instances, &hir, "f");
    let x = named_local(body, "x");
    assert!(
        drops_right_before_reassignment(body, x),
        "`x` still owns its first value when `x = new 2` overwrites it, so that old value must \
         be dropped immediately before the reassignment: {body:?}"
    );
}

#[test]
fn reassigning_after_an_unconditional_move_needs_no_drop() {
    let (hir, _tcx, instances) = elaborated(
        "fun consume(p: iso i32) { }
         fun f() {
             let mut p = new 1;
             consume(p);
             p = new 2;
         }",
    );
    let body = body_for(&instances, &hir, "f");
    let p = named_local(body, "p");
    assert!(
        !drops_right_before_reassignment(body, p),
        "`p` was already moved into `consume` before `p = new 2` runs, so there is no old value \
         left to drop: {body:?}"
    );
}

#[test]
fn reassigning_after_a_conditional_move_needs_a_drop_flag_at_the_reassignment_too() {
    // Mirrors `reassigning_after_a_move_cures_it` in `borrowck/definite_init.rs` (which is only
    // about permitting the later *use* of `p`) -- here the same conditional-move shape means the
    // reassignment itself needs the same runtime flag machinery the end-of-scope case does,
    // because whether `p` still owns its old value when `p = new 2` runs depends on which branch
    // of the `if` ran.
    let (hir, _tcx, instances) = elaborated(
        "fun consume(p: iso i32) { }
         fun f(cond: bool) {
             let mut p = new 1;
             if cond {
                 consume(p);
             }
             p = new 2;
         }",
    );
    let body = body_for(&instances, &hir, "f");
    let cond = named_local(body, "cond");
    let p = named_local(body, "p");

    let cond_targets = find_switch_on(body, cond);
    let moved_arm = cond_targets
        .values
        .iter()
        .find(|&&(value, _)| value == 1)
        .map(|&(_, target)| target)
        .expect("`if cond { .. }` switches on `cond == 1` for its then-arm");
    let not_moved_arm = cond_targets.otherwise;

    match find_flag_switch(body) {
        Some((_, flag_targets)) => {
            let (dropped, skipped) =
                partition_switch_arms_before_reassignment(body, flag_targets, p);
            assert_eq!(
                dropped,
                vec![Place::from_local(p)],
                "exactly one arm of the synthesized drop-flag switch must drop `p`'s old value \
                 right before it is overwritten: {body:?}"
            );
            assert_eq!(
                skipped, 1,
                "the other arm (reached when `consume(p)` ran) must skip that drop: {body:?}"
            );
        }
        None => {
            assert_eq!(
                drop_place_before_reassignment(body, moved_arm, p),
                None,
                "the arm that moved `p` into `consume` must not drop it again before the \
                 reassignment: {body:?}"
            );
            assert_eq!(
                drop_place_before_reassignment(body, not_moved_arm, p),
                Some(Place::from_local(p)),
                "the arm that never moved `p` must still drop its old value before the \
                 reassignment: {body:?}"
            );
        }
    }
}

#[test]
fn reassigning_a_struct_drops_only_its_owned_field_first() {
    let (hir, _tcx, instances) = elaborated(
        "struct Handle { public owned: iso i32, public tag: i32 }
         fun f() {
             let mut h = Handle { owned: new 1, tag: 0 };
             h = Handle { owned: new 2, tag: 1 };
         }",
    );
    let body = body_for(&instances, &hir, "f");
    let h = named_local(body, "h");
    assert!(
        drops_right_before_reassignment(body, h),
        "`h`'s old `owned` field is still live when `h` is overwritten wholesale, so it must be \
         dropped first (the struct itself is not `iso`, so only the field is dropped, same as \
         the end-of-scope struct-field tests above): {body:?}"
    );
}

fn shallow_freed_places(body: &Body) -> Vec<Place> {
    body.basic_blocks
        .iter()
        .filter_map(|block| match &block.terminator.kind {
            TerminatorKind::DropIso { place, .. } => Some(place.clone()),
            _ => None,
        })
        .collect()
}

fn dropped_places(body: &Body) -> Vec<Place> {
    body.basic_blocks
        .iter()
        .filter_map(|block| match &block.terminator.kind {
            TerminatorKind::Drop { place, .. } => Some(place.clone()),
            _ => None,
        })
        .collect()
}

fn flag_switch_count(body: &Body) -> usize {
    body.basic_blocks
        .iter()
        .filter(|block| match &block.terminator.kind {
            TerminatorKind::SwitchInt { discr, .. } => match discr {
                Operand::Copy(p) | Operand::Move(p) => {
                    is_bool_constant_flag_candidate(body, p.local)
                }
                Operand::Constant(_) => false,
            },
            _ => false,
        })
        .count()
}

#[test]
fn an_iso_whose_pointee_was_moved_out_has_only_its_allocation_released() {
    let (hir, _tcx, instances) = elaborated(
        "struct Handle { public owned: iso i32, public tag: i32 }
         fun f() {
             let h = new Handle { owned: new 1, tag: 2 };
             let inner = *h;
             let t = inner.tag;
         }",
    );
    let body = body_for(&instances, &hir, "f");
    let h = named_local(body, "h");
    let inner = named_local(body, "inner");

    assert_eq!(
        shallow_freed_places(body),
        vec![Place::from_local(h)],
        "`h`'s pointee now belongs to `inner`, so `h` has only its allocation left to release: \
         {body:?}"
    );
    assert!(
        !dropped_places(body).contains(&Place::from_local(h)),
        "dropping `h` as a whole would run drop glue over a pointee `inner` owns now: {body:?}"
    );
    assert!(
        dropped_places(body).contains(&Place {
            local: inner,
            projections: vec![Projection::Field(0)],
        }),
        "the value that was moved out is still dropped, through the local that took it: {body:?}"
    );
}

#[test]
fn assigning_through_a_pointer_drops_the_old_value_it_pointed_at() {
    let (hir, _tcx, instances) = elaborated(
        "struct Handle { public owned: iso i32, public tag: i32 }
         fun f(p: &mut Handle) {
             *p = Handle { owned: new 2, tag: 3 };
         }",
    );
    let body = body_for(&instances, &hir, "f");
    let p = named_local(body, "p");

    assert!(
        dropped_places(body).contains(&Place {
            local: p,
            projections: vec![Projection::Deref, Projection::Field(0)],
        }),
        "the `Handle` `p` pointed at owned an allocation, and overwriting it wholesale is the \
         last moment anything can reach it: {body:?}"
    );
    assert!(
        !dropped_places(body).contains(&Place::from_local(p)),
        "`p` itself is a borrow, and this body owns nothing to drop through it: {body:?}"
    );
}

#[test]
fn a_struct_and_one_of_its_fields_conditionally_moved_apart_need_a_flag_each() {
    let (hir, _tcx, instances) = elaborated(
        "struct Handle { public owned: iso i32, public tag: i32 }
         fun consume(p: iso i32) { }
         fun take(h: Handle) { }
         fun f(a: bool, b: bool) {
             let h = Handle { owned: new 1, tag: 2 };
             if a {
                 take(h);
             } else {
                 if b {
                     consume(h.owned);
                 }
             }
         }",
    );
    let body = body_for(&instances, &hir, "f");
    let h = named_local(body, "h");

    assert_eq!(
        flag_switch_count(body),
        2,
        "one flag for `h` itself and one for its separately moved `owned` field: {body:?}"
    );
    assert_eq!(
        dropped_places(body),
        vec![Place {
            local: h,
            projections: vec![Projection::Field(0)],
        }],
        "`h`'s only droppable field is the one drop this body has to guard: {body:?}"
    );
}

#[test]
fn a_closure_value_is_dropped_at_the_end_of_its_scope() {
    let (hir, _tcx, instances) =
        elaborated("fun f() -> i32 { let n = 1; let g = || n; return g(); }");
    let body = body_for(&instances, &hir, "f");
    let g = named_local(body, "g");
    assert!(
        dropped_places(body).contains(&Place::from_local(g)),
        "the closure value has to be dropped, since it may own an environment: {body:?}"
    );
}

#[test]
fn a_closure_value_moved_into_a_call_is_not_dropped_by_its_caller_scope() {
    let (hir, _tcx, instances) = elaborated(
        "fun call(g: fun() -> i32) -> i32 { return g(); }
         fun f() -> i32 { let n = 1; let g = || n; return call(g); }",
    );
    let body = body_for(&instances, &hir, "f");
    let g = named_local(body, "g");
    assert!(
        !dropped_places(body).contains(&Place::from_local(g)),
        "`call` took ownership of the closure, so `f` no longer has one to drop: {body:?}"
    );
}

#[test]
fn calling_a_closure_does_not_consume_it() {
    let (hir, _tcx, instances) =
        elaborated("fun f() -> i32 { let n = 1; let g = || n; let a = g(); return g(); }");
    let body = body_for(&instances, &hir, "f");
    let g = named_local(body, "g");
    assert!(
        dropped_places(body).contains(&Place::from_local(g)),
        "calling a closure reads it rather than consuming it, so it is still there to drop \
         afterwards: {body:?}"
    );
}

#[test]
fn a_reference_match_does_not_drop_the_value_it_borrows() {
    let (hir, _tcx, instances) = elaborated(
        "struct Buf { public bytes: iso [u8] }\n\
         enum Holder { full: Buf, empty }\n\
         fun is_full(h: &Holder) -> bool {\n\
             return match h { .full(_) => true, .empty => false, };\n\
         }\n\
         fun main() {}",
    );
    let body = body_for(&instances, &hir, "is_full");
    assert!(
        !body
            .basic_blocks
            .iter()
            .any(|b| matches!(b.terminator.kind, TerminatorKind::Drop { .. })),
        "a match through a reference owns nothing, so it drops nothing: {body:?}"
    );
}
