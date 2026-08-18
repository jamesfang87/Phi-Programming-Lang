//! The constness-check pass: walks every [`Body`] [`crate::mir::lower::lower_program`] produced
//! and reports each [`StatementKind::CheckMutable`] that fails, per that variant's own docs.
//!
//! This runs once per generic [`LoweredProgram`] body, ahead of monomorphization, rather than
//! once per monomorphized [`crate::mir::Instance`], so a generic definition checked against many
//! call sites is still only ever diagnosed once, at its own declaration.
//!
//! A `&mut self` method receiver's own mutability is deliberately not checked here: lowering a
//! call does not yet materialize a receiver's implicit auto-ref as an explicit
//! [`Rvalue::Ref`](crate::mir::Rvalue::Ref), so there is no [`StatementKind::CheckMutable`] for
//! this pass to see at a receiver position. That check still runs in `typeck`, at
//! [`crate::typeck::Typeck::place_mutable_root`]'s one remaining call site.

use crate::diagnostics::mir::constck::report_not_mutable;
use crate::driver::source::SrcSpan;
use crate::mir::lower::LoweredProgram;
use crate::mir::{Body, Place, PlaceElem, StatementKind};

/// Checks every body [`crate::mir::lower::lower_program`] produced, reporting a diagnostic for
/// each place a `=`, a compound assignment, or an explicit `&mut` borrow reaches that may not be
/// written to directly.
pub fn check(program: &LoweredProgram) {
    for body in program.bodies.values() {
        check_body(body);
    }
}

fn check_body(body: &Body) {
    for block in &body.basic_blocks {
        for stmt in &block.statements {
            if let StatementKind::CheckMutable(place) = &stmt.kind {
                check_place(body, place, stmt.span);
            }
        }
    }
}

/// Reports a diagnostic if `place` may not be written to directly: walking its projection down
/// to a bare local, without ever crossing a `Deref`, reaches a local other than an unadorned
/// `let`'s -- see [`StatementKind::CheckMutable`]'s own docs.
fn check_place(body: &Body, place: &Place, span: SrcSpan) {
    if place.projection.contains(&PlaceElem::Deref) {
        return;
    }
    let decl = &body.local_decls[place.local.index()];
    if decl.mutability == crate::ast::Mutability::Immutable {
        let name = decl.name.unwrap_or_else(|| {
            panic!(
                "mir::constck: an immutable local reachable through a CheckMutable place is \
                 always named, since only a plain `let` binding is ever declared immutable"
            )
        });
        report_not_mutable(name, span);
    }
}

#[cfg(test)]
mod tests {
    use crate::testing::{mir_constck_accepts as accepts, mir_constck_rejects as rejects};

    #[test]
    fn assignment_through_a_bare_local_is_checked_against_its_own_let() {
        accepts("fun f() { let mut a = 1; a = 2; }");
        rejects(
            "fun f() { let a = 1; a = 2; }",
            "cannot assign to `a`, which is not declared `mut`",
        );
    }

    /// This walk exercised end to end: a two-level field chain and an index, each rejected when
    /// rooted in a plain `let` and accepted once the `let` is `mut`; and, separately, accepted
    /// regardless of the root's own `mut`-ness the moment the chain crosses a reference, since a
    /// reference carries its own `&`/`&mut`-ness that this compiler does not enforce yet (there
    /// is no borrow checker to weigh it).
    #[test]
    fn assignment_through_a_chain_of_fields_and_indices_stops_checking_at_a_reference() {
        let structs = "struct Inner { fst: i32 } struct Outer { inner: Inner }";

        rejects(
            &format!("{structs} fun f(o: Outer) {{ let p = o; p.inner.fst = 1; }}"),
            "cannot assign to `p`, which is not declared `mut`",
        );
        accepts(&format!(
            "{structs} fun f(o: Outer) {{ let mut p = o; p.inner.fst = 1; }}"
        ));

        rejects(
            "fun f(a: [i32; 4]) { let arr = a; arr[0] = 1; }",
            "cannot assign to `arr`, which is not declared `mut`",
        );
        accepts("fun f(a: [i32; 4]) { let mut arr = a; arr[0] = 1; }");

        accepts(&format!(
            "{structs} fun f(o: &mut Outer) {{ let r = o; r.inner.fst = 1; }}"
        ));
    }

    /// `let mut` (or a plain `let`) applies to a whole pattern at once: every name it introduces
    /// is affected the same way, not only the first one written -- `b` here is rejected exactly
    /// as `a` would be, and both are accepted once the `let` that introduces them is `mut`.
    #[test]
    fn tuple_destructured_mutability_applies_to_every_binding_the_pattern_introduces() {
        accepts("fun f() { let mut (a, b) = (1, 2); a = 3; b = 4; }");
        rejects(
            "fun f() { let (a, b) = (1, 2); a = 3; }",
            "cannot assign to `a`, which is not declared `mut`",
        );
        rejects(
            "fun f() { let (a, b) = (1, 2); b = 4; }",
            "cannot assign to `b`, which is not declared `mut`",
        );
    }

    /// A `for` binding, a `match` arm's, and a `with` lend all resolve through the very same
    /// kind of local a `let` does, but none of them has `mut` syntax of its own to opt into -- so
    /// all three stay exactly as unrestricted by this check as they were before it existed.
    #[test]
    fn constness_does_not_restrict_bindings_that_are_not_a_let() {
        // A `for` binding, reassigned inside the loop body.
        accepts(
            "module core::option;
             public enum Option<T> { some: T, none }
             struct Counter { n: i32 }
             extend Counter { fun next(&mut self) -> Option<i32> { return .none; } }
             fun f() {
                 let c = Counter { n: 0 };
                 for x in c { x = 1; }
             }",
        );

        // A `match` arm's binding.
        accepts(
            "enum Option<T> { some: T, none }
             fun f(o: Option<i32>) {
                 match o {
                     .some(x) => { x = 1; },
                     .none => {},
                 }
             }",
        );

        // A `with` lend, reassigned to a different reference of the same type.
        accepts(
            "fun f() {
                 let a = 1;
                 let b = 2;
                 with x = &a { x = &b; }
             }",
        );
    }

    /// Neither a parameter nor `self` has `mut` syntax of its own, so this check leaves both
    /// exactly as assignable as they were before it existed -- direct reassignment, a mutable
    /// borrow, and writing through a field `self` owns.
    #[test]
    fn parameters_and_self_fields_remain_unrestricted() {
        accepts("fun f(x: i32) { x = 5; }");
        accepts("fun f(x: i32) -> &mut i32 { return &mut x; }");
        accepts(
            "struct Counter { n: i32 }
             extend Counter { fun bump(&mut self) { self.n = self.n + 1; } }",
        );
    }

    #[test]
    fn an_explicit_mutable_borrow_is_checked_the_same_as_an_assignment() {
        accepts("fun f() { let mut a = 1; let r = &mut a; let _ = r; }");
        rejects(
            "fun f() { let a = 1; let r = &mut a; let _ = r; }",
            "cannot assign to `a`, which is not declared `mut`",
        );
    }

    #[test]
    fn a_compound_assignment_is_checked_the_same_as_a_plain_one() {
        accepts("fun f() { let mut a = 1; a += 1; }");
        rejects(
            "fun f() { let a = 1; a += 1; }",
            "cannot assign to `a`, which is not declared `mut`",
        );
    }

    // -----------------------------------------------------------------
    // `&mut self` receivers
    // -----------------------------------------------------------------
    //
    // A `&mut self` call autorefs its receiver exactly as `&mut receiver` would --
    // `mir::lower::call` materializes that autoref as its own `Rvalue::Ref`, marked with the
    // same `CheckMutable` an assignment's or an explicit `&mut` borrow's own write is.

    #[test]
    fn a_mut_self_call_is_checked_the_same_as_an_explicit_mutable_borrow() {
        accepts(
            "struct Counter { n: i32 }
             extend Counter { fun bump(&mut self) {} }
             fun f() { let mut c = Counter { n: 0 }; c.bump(); }",
        );
        rejects(
            "struct Counter { n: i32 }
             extend Counter { fun bump(&mut self) {} }
             fun f() { let c = Counter { n: 0 }; c.bump(); }",
            "cannot assign to `c`, which is not declared `mut`",
        );
    }

    /// The receiver need not be a bare local for this to apply: a `&mut self` call reached two
    /// levels down an immutable `let` still names the chain's *root* binding -- `o`, not the
    /// `inner` field it was reached through -- exactly as an assignment through the same chain
    /// would.
    #[test]
    fn a_mut_self_call_reached_through_a_field_chain_is_rooted_at_the_lets_own_binding() {
        rejects(
            "struct Counter { n: i32 }
             extend Counter { fun bump(&mut self) {} }
             struct Outer { inner: Counter }
             fun f() {
                 let o = Outer { inner: Counter { n: 0 } };
                 o.inner.bump();
             }",
            "cannot assign to `o`, which is not declared `mut`",
        );
    }

    /// A receiver already behind a reference is unrestricted regardless of what holds that
    /// reference, exactly as an assignment reached the same way is (see
    /// `assignment_through_a_chain_of_fields_and_indices_stops_checking_at_a_reference`).
    #[test]
    fn a_mut_self_call_through_an_existing_reference_is_unrestricted() {
        accepts(
            "struct Counter { n: i32 }
             extend Counter { fun bump(&mut self) {} }
             fun f(c: &mut Counter) { let r = c; r.bump(); }",
        );
    }

    /// `&self` autorefs the same way `&mut self` does, just without a mutability check at the
    /// end of it -- exercised here mainly so `mir::lower::call`'s receiver adjustment is seen to
    /// lower cleanly for the shared case too, not only the mutable one above.
    #[test]
    fn a_ref_self_call_is_never_restricted() {
        accepts(
            "struct Counter { n: i32 }
             extend Counter { fun peek(&self) -> i32 { return self.n; } }
             fun f() { let c = Counter { n: 0 }; let _ = c.peek(); }",
        );
    }
}
