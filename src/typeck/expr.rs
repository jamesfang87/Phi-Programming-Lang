use std::collections::HashSet;

use crate::ast::interner::Interner;
use crate::ast::{BinaryOp, Ident, Mutability};
use crate::diagnostics::typeck::expr::{
    report_assign_mismatch, report_closure_body_mismatch, report_compound_assign_mismatch,
    report_compound_assign_result_mismatch, report_ctor_not_a_struct, report_duplicate_field,
    report_elided_ctor_unknown, report_field_type_mismatch, report_if_branches_mismatch,
    report_if_cond_not_bool, report_if_no_else_mismatch, report_index_base_unknown,
    report_index_not_int, report_match_arm_mismatch, report_missing_fields, report_no_range_type,
    report_no_such_field, report_no_such_variant, report_not_a_struct_literal,
    report_not_assignable, report_not_indexable, report_not_try, report_range_endpoints_mismatch,
    report_record_field_unknown, report_try_error_mismatch, report_try_operand_unknown,
    report_try_outside, report_try_return_mismatch, report_variant_enum_unknown,
    report_variant_expr_payload_shape, report_variant_missing_fields,
    report_variant_payload_mismatch,
};
use crate::driver::source::SrcSpan;
use crate::hir::{DefId, Hir, HirId, OwnerNode, Path, Payload, PayloadField, Res, TyDef, Type};
use crate::langitems::LangItem;
use crate::nameres::PrimTy;
use crate::typeck::Typeck;
use crate::typeck::pat::VariantTys;
use crate::typeck::ty::{Ty, TyKind};

impl<'hir> Typeck<'hir> {
    // -----------------------------------------------------------------
    // Assignment
    // -----------------------------------------------------------------

    pub(crate) fn check_assign(&mut self, lhs: HirId, rhs: HirId, span: SrcSpan) -> Ty {
        let lhs_ty = self.ty_of(lhs);
        if !self.is_place_expr(lhs) {
            report_not_assignable(self.hir.expr(lhs).span);
        }

        let rhs_ty = self.ty_of_expecting(rhs, lhs_ty);
        if let Err(err) = self.unifier.unify(&self.tcx, lhs_ty, rhs_ty) {
            report_assign_mismatch(self.cx(), err, span);
        }
        self.tcx.unit()
    }

    pub(crate) fn check_assign_op(
        &mut self,
        op: BinaryOp,
        lhs: HirId,
        rhs: HirId,
        span: SrcSpan,
    ) -> Ty {
        let lhs_ty = self.ty_of(lhs);
        if !self.is_place_expr(lhs) {
            report_not_assignable(self.hir.expr(lhs).span);
        }

        let rhs_ty = self.ty_of_expecting(rhs, lhs_ty);
        if let Err(err) = self.unifier.unify(&self.tcx, lhs_ty, rhs_ty) {
            report_compound_assign_mismatch(self.cx(), err, span);
            return self.tcx.unit();
        }

        let operand = self.unifier.root(lhs_ty);
        let produced = self.check_operator(op, operand, lhs.owner, span);
        // `foo += bar` stores the operator's result back into `foo`, so an operator that produces
        // something else cannot be compounded.
        if let Err(err) = self.unifier.unify(&self.tcx, operand, produced) {
            report_compound_assign_result_mismatch(self.cx(), err, span);
        }
        self.tcx.unit()
    }

    /// Checks `&operand` and `&mut operand`.
    pub(crate) fn check_borrow(
        &mut self,
        mutability: Mutability,
        operand: HirId,
        expected: Option<Ty>,
    ) -> Ty {
        let inner = expected.and_then(|expected| match *self.tcx.kind(expected) {
            TyKind::Ref {
                base,
                mutability: m,
            } if m == mutability => Some(base),
            _ => None,
        });
        let ty = self.ty_of_expecting_opt(operand, inner);
        self.tcx.mk_ref(ty, mutability)
    }

    // -----------------------------------------------------------------
    // Indexing
    // -----------------------------------------------------------------

    /// Checks `base[index]`.
    pub(crate) fn check_index(&mut self, base: HirId, index: HirId, span: SrcSpan) -> Ty {
        let base_ty = self.ty_of(base);
        let base_ty = self.resolve_deep(base_ty);

        if matches!(self.tcx.kind(base_ty), TyKind::Error) {
            self.ty_of(index);
            return self.tcx.error();
        }
        if matches!(self.tcx.kind(base_ty), TyKind::Var(_)) {
            report_index_base_unknown(self.hir.expr(base).span);
            self.ty_of(index);
            return self.tcx.error();
        }

        let (peeled, _layers) = self.peel_receiver(base_ty);
        if let TyKind::Array { elem, .. } = *self.tcx.kind(peeled) {
            let int = self.tcx.next_int_var();
            let index_ty = self.ty_of(index);
            if let Err(err) = self.unifier.unify(&self.tcx, int, index_ty) {
                report_index_not_int(self.cx(), err, span);
            }
            return elem;
        }

        let member = Ident {
            text: Interner::intern("index"),
            span,
        };
        if self
            .method_candidates(peeled, member.text, base.owner)
            .is_empty()
        {
            report_not_indexable(self.cx(), peeled, span);
            self.ty_of(index);
            return self.tcx.error();
        }
        self.check_method_call(base, member, &[index], span)
    }

    // -----------------------------------------------------------------
    // Building a nominal value
    // -----------------------------------------------------------------

    pub(crate) fn check_ctor(
        &mut self,
        path: Option<&'hir Path>,
        payload: &'hir [PayloadField],
        expected: Option<Ty>,
        span: SrcSpan,
        owner: DefId,
    ) -> Ty {
        let Some(self_ty) = self.ctor_ty(path, expected, span, owner) else {
            for field in payload {
                self.ty_of(field.value);
            }
            return self.tcx.error();
        };

        let Some(declared) = self.struct_fields(self_ty) else {
            report_not_a_struct_literal(self.cx(), self_ty, span);
            for field in payload {
                self.ty_of(field.value);
            }
            return self.tcx.error();
        };

        let mut written = HashSet::new();
        for field in payload {
            if !written.insert(field.name.text) {
                report_duplicate_field(field.name);
            }
            match declared
                .iter()
                .find(|(name, _)| name.text == field.name.text)
            {
                Some(&(_, want)) => {
                    let got = self.ty_of_expecting(field.value, want);
                    if let Err(err) = self.unifier.unify(&self.tcx, want, got) {
                        report_field_type_mismatch(self.cx(), err, self.hir.expr(field.value).span);
                    }
                }
                None => {
                    report_no_such_field(self.cx(), field.name, self_ty);
                    self.ty_of(field.value);
                }
            }
        }

        let missing: Vec<&'static str> = declared
            .iter()
            .filter(|(name, _)| !written.contains(&name.text))
            .map(|(name, _)| Interner::resolve(name.text))
            .collect();
        if !missing.is_empty() {
            report_missing_fields(self.cx(), &missing, self_ty, span);
        }

        self_ty
    }

    fn ctor_ty(
        &mut self,
        path: Option<&Path>,
        expected: Option<Ty>,
        span: SrcSpan,
        owner: DefId,
    ) -> Option<Ty> {
        let Some(path) = path else {
            let expected = expected.map(|ty| self.resolve_deep(ty));
            return match expected {
                Some(ty) if matches!(self.tcx.kind(ty), TyKind::Adt { .. }) => Some(ty),
                Some(ty) if matches!(self.tcx.kind(ty), TyKind::Error) => None,
                _ => {
                    report_elided_ctor_unknown(span);
                    None
                }
            };
        };

        match path.res {
            Res::Type(Type::Def(TyDef::Struct(def))) => {
                let OwnerNode::Struct(struct_) = self.hir.def(def) else {
                    unreachable!("a TyDef::Struct always names a Struct owner");
                };
                let args: Vec<Ty> = struct_
                    .generics
                    .iter()
                    .map(|_| self.tcx.next_ty_var())
                    .collect();
                let ty = self.tcx.mk_adt(def, args);
                if let Some(expected) = expected {
                    let _ = self.unifier.unify(&self.tcx, expected, ty);
                }
                Some(ty)
            }
            Res::SelfTy(_) => Some(self.self_ty(owner, span)),
            Res::Err => None, // already reported by name resolution
            _ => {
                report_ctor_not_a_struct(span);
                None
            }
        }
    }

    /// Checks an enum variant being built: `.none`, `.circle(1.0)`, `.square { l: 2.0 }`.
    pub(crate) fn check_variant_expr(
        &mut self,
        variant: Ident,
        payload: &'hir Payload,
        expected: Option<Ty>,
        span: SrcSpan,
    ) -> Ty {
        let expected = expected.map(|ty| self.resolve_deep(ty));
        let self_ty = match expected {
            Some(ty) if matches!(self.tcx.kind(ty), TyKind::Error) => {
                self.check_payload_exprs_only(payload);
                return self.tcx.error();
            }
            Some(ty) if !matches!(self.tcx.kind(ty), TyKind::Var(_)) => ty,
            _ => {
                report_variant_enum_unknown(variant, span);
                self.check_payload_exprs_only(payload);
                return self.tcx.error();
            }
        };

        let Some(found) = self.lookup_variant(self_ty, variant.text) else {
            report_no_such_variant(self.cx(), variant, self_ty);
            self.check_payload_exprs_only(payload);
            return self.tcx.error();
        };

        match (&found.payload, payload) {
            (VariantTys::Unit, Payload::None) => {}
            (VariantTys::Single(want), Payload::Single(value)) => {
                let want = *want;
                let got = self.ty_of_expecting(*value, want);
                if let Err(err) = self.unifier.unify(&self.tcx, want, got) {
                    report_variant_payload_mismatch(self.cx(), err, self.hir.expr(*value).span);
                }
            }
            (VariantTys::Record(want), Payload::Record(fields)) => {
                let want = want.clone();
                self.check_variant_record(&want, fields, found.id);
            }
            _ => {
                let declared = found.payload.describe();
                report_variant_expr_payload_shape(self.hir, variant, span, declared, found.id);
                self.check_payload_exprs_only(payload);
            }
        }

        self_ty
    }

    /// Checks the field initializers of a record payload against what the variant declares.
    fn check_variant_record(
        &mut self,
        declared: &[(Ident, Ty)],
        written: &'hir [PayloadField],
        variant: HirId,
    ) {
        let mut seen = HashSet::new();
        for field in written {
            if !seen.insert(field.name.text) {
                report_duplicate_field(field.name);
            }
            match declared
                .iter()
                .find(|(name, _)| name.text == field.name.text)
            {
                Some(&(_, want)) => {
                    let got = self.ty_of_expecting(field.value, want);
                    if let Err(err) = self.unifier.unify(&self.tcx, want, got) {
                        report_field_type_mismatch(self.cx(), err, self.hir.expr(field.value).span);
                    }
                }
                None => {
                    report_record_field_unknown(self.hir, field.name, variant);
                    self.ty_of(field.value);
                }
            }
        }

        let missing: Vec<&'static str> = declared
            .iter()
            .filter(|(name, _)| !seen.contains(&name.text))
            .map(|(name, _)| Interner::resolve(name.text))
            .collect();
        if !missing.is_empty() {
            report_variant_missing_fields(self.hir, variant, &missing);
        }
    }

    /// Checks a payload's expressions and nothing else, for a variant that has already been
    /// reported on.
    fn check_payload_exprs_only(&mut self, payload: &'hir Payload) {
        match payload {
            Payload::None => {}
            Payload::Single(value) => {
                self.ty_of(*value);
            }
            Payload::Record(fields) => {
                for field in fields {
                    self.ty_of(field.value);
                }
            }
        }
    }

    // -----------------------------------------------------------------
    // Branching
    // -----------------------------------------------------------------

    pub(crate) fn check_if(
        &mut self,
        cond: HirId,
        then_block: HirId,
        else_block: Option<HirId>,
        expected: Option<Ty>,
        span: SrcSpan,
    ) -> Ty {
        let cond_ty = self.ty_of(cond);
        let bool_ty = self.tcx.mk_prim(PrimTy::Bool);
        if let Err(err) = self.unifier.unify(&self.tcx, bool_ty, cond_ty) {
            report_if_cond_not_bool(self.cx(), err, self.hir.expr(cond).span);
        }

        let then_ty = self.check_block_expecting(then_block, expected);
        let Some(else_block) = else_block else {
            let unit = self.tcx.unit();
            if let Err(err) = self.unifier.unify(&self.tcx, unit, then_ty) {
                report_if_no_else_mismatch(self.cx(), err, span);
            }
            return unit;
        };

        let else_ty = self.check_block_expecting(else_block, expected);
        if let Err(err) = self.unifier.unify(&self.tcx, then_ty, else_ty) {
            report_if_branches_mismatch(self.cx(), err, span);
            return self.tcx.error();
        }
        self.unifier.root(then_ty)
    }

    /// Checks `match scrutinee { pat => { .. }, .. }`.
    pub(crate) fn check_match(
        &mut self,
        scrutinee: HirId,
        arms: &'hir [HirId],
        expected: Option<Ty>,
        span: SrcSpan,
    ) -> Ty {
        let scrutinee_ty = self.ty_of(scrutinee);

        // A `match` with no arms can produce no value, since no arm ever runs to produce one.
        // `Never` is what says that, and it unifies with whatever the context wanted.
        if arms.is_empty() {
            return self.tcx.never();
        }

        let result = match expected {
            Some(expected) => expected,
            None => self.tcx.next_ty_var(),
        };

        // Whether any arm's own pattern already failed to check -- an unknown variant, a
        // mismatched literal, and so on. Exhaustiveness is skipped in that case: a pattern that
        // does not resolve to a real variant cannot count toward covering one, so asking would
        // report the same mistake a second time as a phantom "not covered".
        let mut pat_failed = false;

        for &arm in arms {
            let arm_node = self.hir.arm(arm);
            let (pat, block, arm_span) = (arm_node.pat, arm_node.block, arm_node.span);

            self.check_pat(pat, scrutinee_ty);
            let pat_ty = self.types.ty(pat);
            pat_failed |= pat_ty.is_some_and(|ty| matches!(self.tcx.kind(ty), TyKind::Error));
            let body = self.check_block_expecting(block, Some(result));
            if let Err(err) = self.unifier.unify(&self.tcx, result, body) {
                report_match_arm_mismatch(self.cx(), err, arm_span);
            }
        }

        // Every arm's pattern is checked above regardless of whether it turns out to be
        // reachable; exhaustiveness is a separate question asked once, over the whole set, now
        // that every arm has had its own chance to report its own mistake first.
        if !pat_failed {
            self.check_match_exhaustive(scrutinee_ty, arms, span);
        }

        self.unifier.root(result)
    }

    // -----------------------------------------------------------------
    // Error propagation
    // -----------------------------------------------------------------

    /// Checks `operand?`.
    pub(crate) fn check_try(&mut self, operand: HirId, span: SrcSpan, owner: DefId) -> Ty {
        let operand_ty = self.ty_of(operand);
        let operand_ty = self.resolve_deep(operand_ty);

        if matches!(self.tcx.kind(operand_ty), TyKind::Error) {
            return self.tcx.error();
        }
        if matches!(self.tcx.kind(operand_ty), TyKind::Var(_)) {
            report_try_operand_unknown(span);
            return self.tcx.error();
        }

        let TyKind::Adt { def, args } = self.tcx.kind(operand_ty).clone() else {
            report_not_try(self.cx(), operand_ty, span);
            return self.tcx.error();
        };

        let result = self.hir.lang_items().get(LangItem::Result);
        let option = self.hir.lang_items().get(LangItem::Option);

        if Some(def) == result && args.len() == 2 {
            self.check_try_return(operand_ty, LangItem::Result, Some(args[1]), span, owner);
            return args[0];
        }
        if Some(def) == option && args.len() == 1 {
            self.check_try_return(operand_ty, LangItem::Option, None, span, owner);
            return args[0];
        }

        report_not_try(self.cx(), operand_ty, span);
        self.tcx.error()
    }

    /// Checks that the enclosing definition returns the same lang item `?` is propagating out of,
    /// and -- for a `Result` -- that the two agree on the error type.
    fn check_try_return(
        &mut self,
        operand_ty: Ty,
        item: LangItem,
        error_ty: Option<Ty>,
        span: SrcSpan,
        owner: DefId,
    ) {
        let Some(ret) = self.owner_ret(owner) else {
            report_try_outside(self.cx(), operand_ty, span);
            return;
        };
        let ret = self.resolve_deep(ret);
        if matches!(self.tcx.kind(ret), TyKind::Error) {
            return;
        }

        let expected_def = self.hir.lang_items().get(item);
        let TyKind::Adt { def, args } = self.tcx.kind(ret).clone() else {
            report_try_return_mismatch(self.cx(), operand_ty, ret, span);
            return;
        };
        if Some(def) != expected_def {
            report_try_return_mismatch(self.cx(), operand_ty, ret, span);
            return;
        }

        // A `Result`'s error type leaves the function, so it is the one thing
        // about the return type that has to match exactly rather than merely be the same enum.
        if let (Some(error_ty), Some(ret_error)) = (error_ty, args.get(1).copied())
            && let Err(err) = self.unifier.unify(&self.tcx, ret_error, error_ty)
        {
            report_try_error_mismatch(self.cx(), err, span);
        }
    }

    /// Gets the return type the definition `owner` names declares, if it declares one.
    fn owner_ret(&mut self, owner: DefId) -> Option<Ty> {
        let sig = self.recorded_ty_of_def(owner)?;
        let TyKind::Fun { ret, .. } = self.tcx.kind(sig) else {
            return None;
        };
        *ret
    }

    // -----------------------------------------------------------------
    // Ranges
    // -----------------------------------------------------------------

    /// Checks `lo..hi`.
    pub(crate) fn check_range(
        &mut self,
        lo: Option<HirId>,
        hi: Option<HirId>,
        span: SrcSpan,
    ) -> Ty {
        let lo_ty = lo.map(|lo| self.ty_of(lo));
        let hi_ty = hi.map(|hi| self.ty_of(hi));

        if let (Some(lo_ty), Some(hi_ty)) = (lo_ty, hi_ty)
            && let Err(err) = self.unifier.unify(&self.tcx, lo_ty, hi_ty)
        {
            report_range_endpoints_mismatch(self.cx(), err, span);
        }

        // TODO: unify with the std::Range
        report_no_range_type(span);
        self.tcx.error()
    }

    // -----------------------------------------------------------------
    // Closures
    // -----------------------------------------------------------------

    pub(crate) fn check_closure(&mut self, def: DefId, expected: Option<Ty>) -> Ty {
        let hir: &'hir Hir = self.hir;
        let OwnerNode::Closure(closure) = hir.def(def) else {
            unreachable!("root of a Closure owner is always OwnerNode::Closure");
        };

        // Only a function type of matching arity is a usable hint:
        let hint = expected.and_then(|expected| match self.tcx.kind(expected).clone() {
            TyKind::Fun { params, ret } if params.len() == closure.params.len() => {
                Some((params, ret))
            }
            _ => None,
        });

        let mut param_tys = Vec::with_capacity(closure.params.len());
        for (index, &id) in closure.params.iter().enumerate() {
            let ty = match hir.closure_param(id).ty {
                Some(annotation) => self.lower_ty(annotation),
                None => match hint.as_ref().map(|(params, _)| params[index]) {
                    Some(ty) => ty,
                    None => self.tcx.next_ty_var(),
                },
            };
            self.types.record(id, ty);
            param_tys.push(ty);
        }

        let declared = closure.ret.map(|ret| self.lower_ty(ret));
        let ret_var = self.tcx.next_ty_var();
        if let Some(declared) = declared {
            let _ = self.unifier.unify(&self.tcx, declared, ret_var);
        } else if let Some(ret) = hint.as_ref().and_then(|(_, ret)| *ret) {
            let _ = self.unifier.unify(&self.tcx, ret, ret_var);
        }

        // Recorded before the body is checked, so a `return` or a `?` inside it resolves the
        // enclosing signature through `owner_ret` the same way it would in a function.
        let temp = self.tcx.mk_fun(param_tys.clone(), Some(ret_var));
        self.types.record_def(def, temp);

        let body = self.check_block_expecting(closure.block, Some(ret_var));
        if let Err(err) = self.unifier.unify(&self.tcx, ret_var, body) {
            report_closure_body_mismatch(self.cx(), err, closure.span);
        }

        // A closure whose body produces nothing has no return type at all, which is what a
        // function with no `-> T` lowers to -- so the two are the same `TyKind::Fun`.
        let ret = self.unifier.root(ret_var);
        let ret = (ret != self.tcx.unit()).then_some(ret);
        let sig = self.tcx.mk_fun(param_tys, ret);
        self.types.record_def(def, sig);

        // The closure's nodes live in its arena, so the enclosing function's writeback would
        // never reach them.
        self.writeback(def);
        sig
    }
}

#[cfg(test)]
mod tests {
    use crate::testing::{typeck_accepts as accepts, typeck_rejects as rejects};

    // -----------------------------------------------------------------
    // Bindings and patterns
    // -----------------------------------------------------------------

    /// The rule the whole of body checking rests on: a `let`'s initializer is what gives the name
    /// it binds a type, so a later use of that name is checked against it.
    #[test]
    fn a_let_binding_takes_its_type_from_its_initializer() {
        accepts("fun f() -> i32 { let x = 1; return x; }");
        rejects(
            "fun f() -> bool { let x = 1; return x; }",
            "mismatched types",
        );
    }

    #[test]
    fn a_let_annotation_is_what_the_initializer_is_checked_against() {
        accepts("fun f() -> i64 { let x: i64 = 1; return x; }");
        rejects("fun f() { let x: bool = 1; }", "mismatched types");
    }

    #[test]
    fn a_tuple_pattern_binds_each_element_separately() {
        accepts(
            "fun f() -> bool {
                 let (a, b) = (1, true);
                 return b;
             }",
        );
        rejects(
            "fun f() -> bool {
                 let (a, b) = (1, true);
                 return a;
             }",
            "mismatched types",
        );
    }

    #[test]
    fn a_tuple_pattern_of_the_wrong_arity_is_reported() {
        rejects("fun f() { let (a, b, c) = (1, true); }", "mismatched types");
    }

    #[test]
    fn a_with_lend_binds_its_pattern_like_a_let() {
        accepts(
            "fun f() {
                 let x = 1;
                 with borrowed = &x { let y: &i32 = borrowed; }
             }",
        );
    }

    // -----------------------------------------------------------------
    // Assignment
    // -----------------------------------------------------------------

    #[test]
    fn an_assignment_checks_the_value_against_the_place() {
        accepts("fun f() { let mut x = 1; x = 2; }");
        rejects("fun f() { let mut x = 1; x = true; }", "mismatched types");
    }

    #[test]
    fn assigning_to_something_that_is_not_a_place_is_reported() {
        rejects("fun f() { 1 = 2; }", "cannot be assigned to");
    }

    /// An assignment produces nothing, so it cannot be the value of the block it ends. Read
    /// through a closure, whose body *is* checked against its return type -- a function's is not;
    /// see the note on [`Typeck::check_function`](crate::typeck::Typeck).
    #[test]
    fn an_assignment_produces_no_value() {
        rejects(
            "fun f() { let mut x = 1; let g: fun() -> i32 = || { x = 2 }; }",
            "mismatched types",
        );
    }

    /// `+=` asks the same question of its left side that `+` asks of its operands, so a struct
    /// with no `Add` impl is rejected for the same reason.
    #[test]
    fn a_compound_assignment_needs_the_operators_trait() {
        accepts("fun f() { let mut x = 1; x += 2; }");
        rejects(
            "module core::ops;

             public trait Add { fun add(&self, other: &Self) -> Self; }

             struct Foo { x: i32 }

             fun f(a: Foo, b: Foo) { let mut c = a; c += b; }",
            "does not implement `Add`",
        );
    }

    // -----------------------------------------------------------------
    // Borrows
    // -----------------------------------------------------------------

    #[test]
    fn a_borrow_produces_a_reference_to_its_operands_type() {
        accepts("fun f(x: i32) -> &i32 { return &x; }");
        rejects("fun f(x: i32) -> &bool { return &x; }", "mismatched types");
    }

    #[test]
    fn a_mutable_borrow_is_not_a_shared_one() {
        accepts("fun f(x: i32) -> &mut i32 { return &mut x; }");
        rejects(
            "fun f(x: i32) -> &mut i32 { return &x; }",
            "mismatched types",
        );
    }

    // -----------------------------------------------------------------
    // Struct literals
    // -----------------------------------------------------------------

    #[test]
    fn a_struct_literal_checks_each_field_against_its_declared_type() {
        accepts(
            "struct Pair { fst: i32, snd: bool }
             fun f() -> Pair { return Pair { fst: 1, snd: true }; }",
        );
        rejects(
            "struct Pair { fst: i32, snd: bool }
             fun f() -> Pair { return Pair { fst: true, snd: true }; }",
            "mismatched types",
        );
    }

    #[test]
    fn a_struct_literal_missing_a_field_is_reported() {
        rejects(
            "struct Pair { fst: i32, snd: bool }
             fun f() -> Pair { return Pair { fst: 1 }; }",
            "is missing field `snd`",
        );
    }

    #[test]
    fn a_struct_literal_with_a_field_that_is_not_declared_is_reported() {
        rejects(
            "struct Pair { fst: i32 }
             fun f() -> Pair { return Pair { fst: 1, third: 2 }; }",
            "no field `third`",
        );
    }

    /// The elided form names no struct at all, so the expectation is the only thing that says
    /// which one it builds.
    #[test]
    fn an_elided_struct_literal_takes_its_struct_from_the_expectation() {
        accepts(
            "struct Pair { fst: i32, snd: bool }
             fun f() -> Pair { return .{ fst: 1, snd: true }; }",
        );
    }

    #[test]
    fn an_elided_struct_literal_with_nothing_expecting_it_is_reported() {
        rejects(
            "struct Pair { fst: i32 }
             fun f() { let p = .{ fst: 1 }; }",
            "names no struct",
        );
    }

    /// A written path leaves the struct's parameters as inference variables, and the expectation
    /// is what pins them.
    #[test]
    fn a_generic_struct_literals_arguments_come_from_the_annotation() {
        accepts(
            "struct Wrap<T> { inner: T }
             fun f() { let w: Wrap<i64> = Wrap { inner: 1 }; }",
        );
        rejects(
            "struct Wrap<T> { inner: T }
             fun f() { let w: Wrap<bool> = Wrap { inner: 1 }; }",
            "mismatched types",
        );
    }

    // -----------------------------------------------------------------
    // Enum variants
    // -----------------------------------------------------------------

    #[test]
    fn a_variant_takes_its_enum_from_the_expectation() {
        accepts(
            "enum Shape { unit, circle: f64 }
             fun f() -> Shape { return .circle(1.0); }",
        );
    }

    /// The shape the whole expectation mechanism exists for: the return type reaches `.ok`, and
    /// `.ok`'s declared payload reaches the `.none` inside it.
    #[test]
    fn a_variants_payload_carries_the_expectation_further_down() {
        accepts(
            "enum Option<T> { some: T, none }
             enum Result<T, E> { ok: T, err: E }
             fun f() -> Result<Option<i32>, bool> { return .ok(.none); }",
        );
    }

    #[test]
    fn a_variant_with_nothing_expecting_it_is_reported() {
        rejects(
            "enum Shape { unit }
             fun f() { let s = .unit; }",
            "the enum `.unit` belongs to is unknown",
        );
    }

    #[test]
    fn a_variant_the_enum_does_not_declare_is_reported() {
        rejects(
            "enum Shape { unit }
             fun f() -> Shape { return .square; }",
            "no variant `square`",
        );
    }

    #[test]
    fn a_variant_built_with_the_wrong_payload_shape_is_reported() {
        rejects(
            "enum Shape { unit, circle: f64 }
             fun f() -> Shape { return .unit(1.0); }",
            "carries no payload",
        );
    }

    #[test]
    fn a_record_variants_fields_are_checked_against_their_declarations() {
        accepts(
            "enum Shape { square: { l: f64 } }
             fun f() -> Shape { return .square { l: 1.0 }; }",
        );
        rejects(
            "enum Shape { square: { l: f64 } }
             fun f() -> Shape { return .square { l: true }; }",
            "mismatched types",
        );
    }

    // -----------------------------------------------------------------
    // Branching
    // -----------------------------------------------------------------

    #[test]
    fn an_if_condition_has_to_be_a_bool() {
        accepts("fun f(c: bool) { if c {} }");
        rejects("fun f() { if 1 {} }", "mismatched types");
    }

    #[test]
    fn both_branches_of_an_if_expression_have_to_agree() {
        accepts("fun f(c: bool) -> i32 { return if c { 1 } else { 2 }; }");
        rejects(
            "fun f(c: bool) -> i32 { return if c { 1 } else { true }; }",
            "mismatched types",
        );
    }

    /// With no `else` there is no value on the path not taken, so the `if` produces nothing and
    /// its block has to as well.
    #[test]
    fn an_if_without_an_else_produces_nothing() {
        rejects("fun f(c: bool) { if c { 1 } }", "mismatched types");
    }

    #[test]
    fn every_arm_of_a_match_is_checked_against_the_scrutinee() {
        accepts(
            "enum Shape { unit, circle: f64 }
             fun f(s: Shape) -> i32 { return match s { .circle(r) => 1, .unit => 2, }; }",
        );
        rejects(
            "enum Shape { unit, circle: f64 }
             fun f(s: Shape) -> i32 { return match s { .square => 1, .unit => 2, }; }",
            "no variant `square`",
        );
    }

    #[test]
    fn every_arm_of_a_match_has_to_produce_the_same_type() {
        rejects(
            "enum Shape { unit, circle: f64 }
             fun f(s: Shape) { let m = match s { .circle(r) => 1, .unit => true, }; }",
            "mismatched types",
        );
    }

    /// A variant pattern's payload binds at the type the variant declares, read through the
    /// scrutinee's generic arguments.
    #[test]
    fn a_variant_pattern_binds_its_payload_at_the_declared_type() {
        accepts(
            "enum Option<T> { some: T, none }
             fun f(o: Option<bool>) -> bool { return match o { .some(v) => v, .none => false, }; }",
        );
        rejects(
            "enum Option<T> { some: T, none }
             fun f(o: Option<bool>) -> i32 { return match o { .some(v) => v, .none => 0, }; }",
            "mismatched types",
        );
    }

    // -----------------------------------------------------------------
    // Indexing
    // -----------------------------------------------------------------

    #[test]
    fn an_array_is_indexed_by_an_integer_and_produces_its_element() {
        accepts("fun f(a: [i32; 4]) -> i32 { return a[0]; }");
        rejects(
            "fun f(a: [i32; 4]) -> i32 { return a[true]; }",
            "mismatched types",
        );
    }

    /// Everything that is not an array indexes through the `Index` trait, so `V` is read back out
    /// of the `extend` block's arguments.
    #[test]
    fn a_type_with_an_index_impl_is_indexed_through_it() {
        accepts(
            "module core::ops;

             public trait Index<K, V> { fun index(&self, key: K) -> &V; }

             struct Map { value: bool }

             extend Map with Index<i32, bool> {
                 fun index(&self, key: i32) -> &bool { return &self.value; }
             }

             fun f(m: Map) -> &bool { return m[0]; }",
        );
    }

    #[test]
    fn indexing_a_type_with_no_index_impl_is_reported() {
        rejects(
            "struct Foo { x: i32 }
             fun f(a: Foo) { let b = a[0]; }",
            "cannot be indexed",
        );
    }

    // -----------------------------------------------------------------
    // Error propagation
    // -----------------------------------------------------------------

    #[test]
    fn try_on_a_result_produces_what_it_carries() {
        accepts(
            "module core::result;

             public enum Result<T, E> { ok: T, err: E }

             fun f(r: Result<i32, bool>) -> Result<i32, bool> {
                 let v = r?;
                 return .ok(v);
             }",
        );
    }

    /// The error type leaves the function, so it is the part of the return type
    /// that has to match.
    #[test]
    fn try_checks_the_error_against_the_enclosing_return_type() {
        rejects(
            "module core::result;

             public enum Result<T, E> { ok: T, err: E }

             fun f(r: Result<i32, bool>) -> Result<i32, i32> {
                 let v = r?;
                 return .ok(v);
             }",
            "mismatched types",
        );
    }

    #[test]
    fn try_on_something_that_is_neither_a_result_nor_an_option_is_reported() {
        rejects(
            "struct Foo { x: i32 }
             fun f(a: Foo) { let b = a?; }",
            "`?` cannot be applied to `Foo`",
        );
    }

    // -----------------------------------------------------------------
    // Closures
    // -----------------------------------------------------------------

    #[test]
    fn a_closure_checks_to_a_function_type_of_its_parameters_and_body() {
        accepts("fun f() { let g: fun(i32) -> i32 = |x: i32| { x + 1 }; }");
        rejects(
            "fun f() { let g: fun(i32) -> bool = |x: i32| { x + 1 }; }",
            "mismatched types",
        );
    }

    /// An unannotated parameter takes its type from the expectation, which is the only thing that
    /// can say what it is.
    #[test]
    fn an_unannotated_closure_parameter_takes_its_type_from_the_expectation() {
        accepts("fun f() { let g: fun(i64) -> i64 = |x| { x + 1 }; }");
    }

    #[test]
    fn a_closure_is_called_at_its_own_signature() {
        accepts("fun f() -> i32 { let g = |x: i32| { x + 1 }; return g(1); }");
        rejects(
            "fun f() -> i32 { let g = |x: i32| { x + 1 }; return g(true); }",
            "mismatched types",
        );
    }

    /// A closure records a signature for itself before its body is checked, so a `return` inside
    /// one resolves the same way it does in a function.
    #[test]
    fn a_return_inside_a_closure_is_checked_against_the_closures_return_type() {
        accepts("fun f() { let g = |x: i32| -> i32 { return x; }; }");
        rejects(
            "fun f() { let g = |x: i32| -> bool { return x; }; }",
            "mismatched types",
        );
    }

    // -----------------------------------------------------------------
    // The blocks that produce nothing
    // -----------------------------------------------------------------

    #[test]
    fn spawn_and_concurrent_run_their_blocks_and_produce_nothing() {
        accepts(
            "fun f() {
                 let mut x = 1;
                 spawn { x = 2; }
                 concurrent { x = 3; }
             }",
        );
        rejects(
            "fun f() { let mut x = 1; spawn { x = true; } }",
            "mismatched types",
        );
    }

    // -----------------------------------------------------------------
    // What has no type yet
    // -----------------------------------------------------------------

    /// Neither of these is a gap in this pass: a string literal and a range are values of types
    /// the core library does not declare and no lang item names, so there is nothing for either
    /// to be. Both report rather than panicking, which is what they used to do.
    #[test]
    fn a_string_literal_and_a_range_report_that_they_have_no_type() {
        rejects("fun f() { let s = \"hi\"; }", "string literal has no type");
        rejects("fun f() { let r = 1..2; }", "range expression has no type");
    }

    // -----------------------------------------------------------------
    // Loops: `while`, `for`, `while let` (all desugar to `ExprKind::Loop`; see `hir::lower::desugar`)
    // -----------------------------------------------------------------

    #[test]
    fn a_while_conditions_type_has_to_be_bool() {
        accepts("fun f(c: bool) { while c {} }");
        // `while` desugars to `loop { if !cond { break }; .. }`, so a non-bool condition is
        // caught by the desugared `if`'s own check, not by anything `while`-specific -- `!x`
        // itself is accepted (`!` is a built-in operator on any primitive, `i32` included), and
        // it is the `if` around it that then rejects the non-`bool` result.
        rejects("fun f(x: i32) { while x {} }", "mismatched types");
    }

    #[test]
    fn a_loop_nested_inside_another_loop_checks() {
        accepts("fun f(a: bool, b: bool) { while a { while b {} } }");
    }

    #[test]
    fn a_while_let_matches_against_an_enum_and_binds_its_payload() {
        accepts(
            "enum Shape { unit, circle: f64 }
             fun f(s: Shape) {
                 while let .circle(r) = s {
                     let y: f64 = r;
                 }
             }",
        );
        rejects(
            "enum Shape { unit, circle: f64 }
             fun f(s: Shape) {
                 while let .circle(r) = s {
                     let y: bool = r;
                 }
             }",
            "mismatched types",
        );
    }

    /// `for pat in iter { .. }` desugars through the iterator protocol: `iter.next()` returning
    /// an `Option`, matched against `.some(pat)`/`.none` -- see `hir::lower::desugar::lower_for`.
    /// No `Iterator` trait or lang item is required for this to work; method resolution finds
    /// `next` the same way it finds any other method.
    #[test]
    fn a_for_loop_binds_its_pattern_to_the_iterators_item_type() {
        accepts(
            "module core::option;
             public enum Option<T> { some: T, none }
             struct Counter { n: i32 }
             extend Counter { fun next(&mut self) -> Option<i32> { return .none; } }
             fun f() {
                 let c = Counter { n: 0 };
                 for x in c { let y: i32 = x; }
             }",
        );
        rejects(
            "module core::option;
             public enum Option<T> { some: T, none }
             struct Counter { n: i32 }
             extend Counter { fun next(&mut self) -> Option<i32> { return .none; } }
             fun f() {
                 let c = Counter { n: 0 };
                 for x in c { let y: bool = x; }
             }",
            "mismatched types",
        );
    }

    // -----------------------------------------------------------------
    // Method chains and generic nesting
    // -----------------------------------------------------------------

    /// Chaining `&self` methods off a call's result does not work -- the result is a temporary,
    /// and `&self` needs a place to borrow (see `a_temporary_receiver_needing_a_place_is_rejected`
    /// below) -- so this chains through `self`-by-value methods instead, which need no place at
    /// all.
    #[test]
    fn method_calls_chain_left_to_right() {
        accepts(
            "struct A {}
             struct B {}
             struct C {}
             extend A { fun to_b(self) -> B { return .{}; } }
             extend B { fun to_c(self) -> C { return .{}; } }
             fun f(a: A) -> C { return a.to_b().to_c(); }",
        );
    }

    /// The chain's other half: a method taking `&self` cannot be called on a temporary, because
    /// there is nothing for the implicit borrow to reach.
    #[test]
    fn a_temporary_receiver_needing_a_place_is_rejected() {
        rejects(
            "struct A {}
             struct B {}
             extend A { fun to_b(self) -> B { return .{}; } }
             extend B { fun show(&self) {} }
             fun f(a: A) { a.to_b().show(); }",
            "receiver is a temporary",
        );
    }

    #[test]
    fn a_nested_generic_struct_literal_checks() {
        accepts(
            "struct Wrap<T> { inner: T }
             fun f() -> Wrap<Wrap<i32>> {
                 return Wrap { inner: Wrap { inner: 1 } };
             }",
        );
        rejects(
            "struct Wrap<T> { inner: T }
             fun f() -> Wrap<Wrap<i32>> {
                 return Wrap { inner: Wrap { inner: true } };
             }",
            "mismatched types",
        );
    }

    #[test]
    fn deeply_nested_if_and_match_expressions_share_one_result_type() {
        accepts(
            "enum Shape { unit, circle: f64 }
             fun f(s: Shape, c: bool) -> i32 {
                 return if c {
                     match s {
                         .unit => 1,
                         .circle(r) => if r > 0.0 { 2 } else { 3 },
                     }
                 } else {
                     4
                 };
             }",
        );
    }

    // -----------------------------------------------------------------
    // `?` on `Option`, closures returning closures
    // -----------------------------------------------------------------

    #[test]
    fn try_on_an_option_produces_what_it_carries() {
        accepts(
            "module core::option;
             public enum Option<T> { some: T, none }
             fun f(o: Option<i32>) -> Option<i32> {
                 let v = o?;
                 return .some(v);
             }",
        );
    }

    #[test]
    fn a_closure_may_capture_an_enclosing_closures_parameter() {
        accepts(
            "fun f() {
                 let make_adder = |x: i32| {
                     let adder = |y: i32| { x + y };
                     adder
                 };
             }",
        );
    }

    // -----------------------------------------------------------------
    // Assignment through a place other than a bare local
    // -----------------------------------------------------------------

    #[test]
    fn assignment_through_a_field_checks_against_the_fields_type() {
        accepts("struct P { x: i32 } fun f(p: P) { p.x = 1; }");
        rejects(
            "struct P { x: i32 } fun f(p: P) { p.x = true; }",
            "mismatched types",
        );
    }

    #[test]
    fn assignment_through_an_index_checks_against_the_elements_type() {
        accepts("fun f(a: [i32; 4]) { a[0] = 1; }");
        rejects("fun f(a: [i32; 4]) { a[0] = true; }", "mismatched types");
    }

    // -----------------------------------------------------------------
    // Unit structs
    // -----------------------------------------------------------------

    #[test]
    fn a_unit_struct_constructs_and_checks() {
        accepts("struct Unit {} fun f() -> Unit { return Unit {}; }");
    }
}
