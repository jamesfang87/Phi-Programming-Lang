use crate::ast::{Ident, Mutability};
use crate::diagnostics::checks::mutability::report_not_mutable;
use crate::driver::source::SrcSpan;
use crate::hir::visit::{self, Visitor};
use crate::hir::{
    AccessArgs, ExprKind, Hir, HirId, Local, OwnerNode, PatKind, Payload, Res, StmtKind,
};
use crate::session::Session;
use crate::typeck::results::TypeResolutions;
use crate::typeck::ty::TyKind;
use crate::typeck::ty::ctx::TyCtx;

pub fn check(session: &Session, hir: &Hir, tcx: &TyCtx, types: &TypeResolutions) {
    let mut pass = LetScopes {
        session,
        hir,
        tcx,
        types,
    };
    for def_id in hir.def_ids() {
        match hir.def(def_id) {
            OwnerNode::Function(function) => {
                if let Some(block) = function.block {
                    pass.visit_block(block.into());
                }
            }
            OwnerNode::Closure(closure) => pass.visit_block(closure.block.into()),
            _ => {}
        }
    }
}

// TODO: why is it called LetScopes?
struct LetScopes<'hir, 'a> {
    session: &'a Session,
    hir: &'hir Hir,
    tcx: &'a TyCtx,
    types: &'a TypeResolutions,
}

impl<'hir> Visitor<'hir> for LetScopes<'hir, '_> {
    fn hir(&self) -> &'hir Hir {
        self.hir
    }

    fn visit_block(&mut self, id: HirId) {
        let block = self.hir.block(id);
        for (index, &stmt_id) in block.stmts.iter().enumerate() {
            let StmtKind::Let {
                mutability: Mutability::Immutable,
                pat,
                ..
            } = self.hir.stmt(stmt_id).kind
            else {
                continue;
            };
            for_each_binding(self.hir, pat, &mut |binding, name| {
                let mut scan = MutationScan {
                    session: self.session,
                    hir: self.hir,
                    tcx: self.tcx,
                    types: self.types,
                    binding,
                    name,
                };
                for &later in &block.stmts[index + 1..] {
                    scan.visit_stmt(later.into());
                }
                if let Some(tail) = block.expr {
                    scan.visit_expr(tail.into());
                }
            });
        }
        visit::walk_block(self, id);
    }
}

fn for_each_binding(hir: &Hir, pat: impl Into<HirId>, f: &mut impl FnMut(HirId, Ident)) {
    let pat = pat.into();
    match &hir.pat(pat).kind {
        PatKind::Binding { name, .. } => f(pat, *name),
        PatKind::Tuple(elems) => {
            for &elem in elems {
                for_each_binding(hir, elem, f);
            }
        }
        PatKind::Variant { payload, .. } => match payload {
            Payload::None => {}
            Payload::Single(value) => for_each_binding(hir, *value, f),
            Payload::Record(fields) => {
                for field in fields {
                    for_each_binding(hir, field.value, f);
                }
            }
        },
        PatKind::Wildcard | PatKind::Literal(_) | PatKind::Error => {}
    }
}

struct MutationScan<'hir, 'a> {
    session: &'a Session,
    hir: &'hir Hir,
    tcx: &'a TyCtx,
    types: &'a TypeResolutions,
    binding: HirId,
    name: Ident,
}

impl<'hir> Visitor<'hir> for MutationScan<'hir, '_> {
    fn hir(&self) -> &'hir Hir {
        self.hir
    }

    fn visit_expr(&mut self, id: HirId) {
        let expr = self.hir.expr(id);
        let span = expr.span;
        match &expr.kind {
            ExprKind::Assign { lhs, .. } | ExprKind::AssignOp { lhs, .. } => {
                self.check_root(*lhs, span)
            }
            ExprKind::Borrow {
                mutability: Mutability::Mutable,
                operand,
            } => self.check_root(*operand, span),
            ExprKind::Access {
                base,
                args: AccessArgs::Call(_),
                ..
            } => {
                let base = *base;
                if self.takes_mut_self(id) {
                    self.check_root(base, span);
                }
            }
            _ => {}
        }
        visit::walk_expr(self, id);
    }
}

impl MutationScan<'_, '_> {
    fn check_root(&self, expr: impl Into<HirId>, span: SrcSpan) {
        let expr = expr.into();
        if self.reaches_through_ref(expr) {
            return;
        }
        match &self.hir.expr(expr).kind {
            ExprKind::Path(path) => {
                if let Res::Local(Local::Variable(id)) = path.res
                    && id == self.binding
                {
                    report_not_mutable(self.session, self.name, span);
                }
            }
            ExprKind::Access {
                base,
                args: AccessArgs::None,
                ..
            }
            | ExprKind::Index { base, .. } => self.check_root(*base, span),
            _ => {}
        }
    }

    fn reaches_through_ref(&self, expr: HirId) -> bool {
        self.types
            .ty(expr)
            .is_some_and(|ty| matches!(self.tcx.kind(ty), TyKind::Ref { .. }))
    }

    fn takes_mut_self(&self, call: HirId) -> bool {
        let Some(resolved) = self.types.call(call) else {
            return false;
        };
        let Some(sig) = self.types.ty_of_def(resolved.def) else {
            return false;
        };
        let TyKind::Fun { params, .. } = self.tcx.kind(sig) else {
            return false;
        };
        matches!(
            params.first().map(|&ty| self.tcx.kind(ty)),
            Some(TyKind::Ref {
                mutability: Mutability::Mutable,
                ..
            })
        )
    }
}
#[cfg(test)]
mod tests {
    use crate::testing::{mutability_accepts as accepts, mutability_rejects as rejects};

    #[test]
    fn assignment_through_a_bare_local_is_checked_against_its_own_let() {
        accepts("fun f() { let mut a = 1; a = 2; }");
        rejects(
            "fun f() { let a = 1; a = 2; }",
            "cannot assign to `a`, which is not declared `mut`",
        );
    }

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

    #[test]
    fn a_mutation_nested_inside_the_lets_scope_is_still_reached() {
        rejects(
            "fun f() { let a = 1; { { a = 2; } } }",
            "cannot assign to `a`, which is not declared `mut`",
        );
        rejects(
            "fun f(c: bool) { let a = 1; if c { a = 2; } }",
            "cannot assign to `a`, which is not declared `mut`",
        );
        rejects(
            "fun f(c: bool) { let a = 1; while c { a = 2; } }",
            "cannot assign to `a`, which is not declared `mut`",
        );
    }

    #[test]
    fn a_mutation_in_the_blocks_tail_expression_is_reached() {
        rejects(
            "fun f() -> i32 { let a = 1; { a = 2; 0 } }",
            "cannot assign to `a`, which is not declared `mut`",
        );
    }

    #[test]
    fn a_later_let_mut_shadowing_the_same_name_is_a_separate_binding() {
        accepts("fun f() { let a = 1; let mut a = 2; a = 3; }");
        rejects(
            "fun f() { let mut a = 1; let a = 2; a = 3; }",
            "cannot assign to `a`, which is not declared `mut`",
        );
    }

    #[test]
    fn a_lets_scope_does_not_reach_statements_written_before_it() {
        accepts("fun f() { let mut a = 1; a = 2; let a = 3; let _ = a; }");
    }
}
