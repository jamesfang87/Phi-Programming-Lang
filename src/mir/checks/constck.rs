//! Constness-check pass: walks every [`Body`] and reports each [`StatementKind::CheckMutable`]
//! that fails, per that variant's own docs. Runs once per generic body, ahead of monomorphization.

use crate::diagnostics::mir::constck::report_not_mutable;
use crate::driver::source::SrcSpan;
use crate::mir::lower::Mir;
use crate::mir::{Body, Place, Projection, StatementKind};

/// Checks every body in `program`, reporting a diagnostic for each place that a `=`, a compound
/// assignment, or an explicit `&mut` borrow reaches but may not write to directly.
pub fn check(program: &Mir) {
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

/// Walks `place`'s projection down to its root local without crossing a `Deref`, and reports a
/// diagnostic if that root is an immutable `let`.
fn check_place(body: &Body, place: &Place, span: SrcSpan) {
    if place.projections.contains(&Projection::Deref) {
        return;
    }
    let decl = &body.local_decls[place.local.index()];
    if decl.mutability == crate::ast::Mutability::Immutable {
        let name = decl.name.unwrap_or_else(|| {
            panic!("an immutable local reachable through CheckMutable is always named")
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

    /// A field chain and an index are both checked against the root binding's own mutability,
    /// but stop being checked once the chain crosses a reference.
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

    /// `let mut` (or a plain `let`) applies to every name a pattern introduces, not only the
    /// first one written.
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

    /// A `for` binding, a `match` arm's, and a `with` lend have no `mut` syntax of their own, so
    /// this check leaves all three unrestricted.
    #[test]
    fn constness_does_not_restrict_bindings_that_are_not_a_let() {
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

        accepts(
            "enum Option<T> { some: T, none }
             fun f(o: Option<i32>) {
                 match o {
                     .some(x) => { x = 1; },
                     .none => {},
                 }
             }",
        );

        accepts(
            "fun f() {
                 let a = 1;
                 let b = 2;
                 with x = &a { x = &b; }
             }",
        );
    }

    /// Neither a parameter nor `self` has `mut` syntax of its own, so this check leaves both
    /// unrestricted.
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

    /// A `&mut self` call reached two levels down an immutable `let` is rooted at the chain's
    /// root binding, not the field it was reached through.
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

    #[test]
    fn a_mut_self_call_through_an_existing_reference_is_unrestricted() {
        accepts(
            "struct Counter { n: i32 }
             extend Counter { fun bump(&mut self) {} }
             fun f(c: &mut Counter) { let r = c; r.bump(); }",
        );
    }

    #[test]
    fn a_ref_self_call_is_never_restricted() {
        accepts(
            "struct Counter { n: i32 }
             extend Counter { fun peek(&self) -> i32 { return self.n; } }
             fun f() { let c = Counter { n: 0 }; let _ = c.peek(); }",
        );
    }
}
