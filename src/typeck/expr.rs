//! The expression forms whose checking needs more than a line: the ones that assign, the ones
//! that build a nominal value, the ones that branch, and the closure literal.
//!
//! [`check_expr`](Typeck::check_expr) keeps the arms that are a couple of lines each -- a literal,
//! a tuple, a path, an operator -- and dispatches the rest here. The split is by size, not by
//! kind; [`traits::method`](crate::typeck::traits::method) already holds the two forms whose
//! checking is a resolution problem rather than a typing one.
//!
//! ## Expectation
//!
//! Three of these forms cannot be checked bottom-up at all. `.{ x: 1 }` names no struct,
//! `.circle(1.0)` names no enum, and `|x| { x + 1 }` may annotate neither its parameters nor its
//! return type. Each is checked against the type its context demands instead, which
//! [`Typeck::ty_of_expecting`] is what carries: a `let`'s annotation, a call's parameter, the
//! enclosing function's return type, the left side of an assignment.
//!
//! An expectation is a hint and never a constraint on its own. Every one of these forms still
//! unifies what it produced with what the context wanted, at the site that established the
//! expectation, so a wrong expectation is reported there rather than silently taking effect here.

use std::collections::HashSet;

use crate::ast::interner::Interner;
use crate::ast::{BinaryOp, Ident, Mutability};
use crate::diag::{DiagCtx, Diagnostic};
use crate::driver::source::SrcSpan;
use crate::hir::{
    DefId, Hir, HirId, OwnerNode, Path, Payload, PayloadField, Res, TyDef, Type,
};
use crate::langitems::LangItem;
use crate::nameres::PrimTy;
use crate::typeck::Typeck;
use crate::typeck::pat::VariantTys;
use crate::typeck::ty::{Ty, TyKind};

impl<'hir> Typeck<'hir> {
    // -----------------------------------------------------------------
    // Assignment
    // -----------------------------------------------------------------

    /// Checks `lhs = rhs`.
    ///
    /// An assignment produces no value, so its type is `Unit` however the two sides check. What it
    /// requires is that the left side names somewhere a value can be put and that the right side
    /// fits there.
    ///
    /// Whether the *binding* on the left was declared `mut` is not checked here. Nothing in this
    /// pass tracks a local's mutability -- [`StmtKind::Let`](crate::hir::StmtKind::Let) records it
    /// and this pass reads only the pattern -- so assigning to an immutable binding is checked
    /// in a later pass that enforces mutability constraints.
    pub(crate) fn check_assign(&mut self, lhs: HirId, rhs: HirId, span: SrcSpan) -> Ty {
        let lhs_ty = self.ty_of(lhs);
        if !self.is_place_expr(lhs) {
            self.report_not_assignable(self.hir.expr(lhs).span);
        }

        let rhs_ty = self.ty_of_expecting(rhs, lhs_ty);
        if let Err(err) = self.unifier.unify(&self.tcx, lhs_ty, rhs_ty) {
            DiagCtx::emit(
                Diagnostic::error(self.cx().show(err).to_string(), span)
                    .with_label("this value cannot be assigned to the place on the left"),
            );
        }
        self.tcx.unit()
    }

    /// Checks `lhs += rhs` and the rest of the compound assignments.
    ///
    /// The same two requirements as [`Typeck::check_assign`], plus the one that makes it compound:
    /// the operator has to apply to the type on the left, exactly as it would in `lhs + rhs`. That
    /// check is [`Typeck::check_operator`], shared with [`ExprKind::Binary`](crate::hir::ExprKind::Binary), so an `extend Foo
    /// with Add` block is what makes `foo += bar` legal as much as `foo + bar`.
    pub(crate) fn check_assign_op(
        &mut self,
        op: BinaryOp,
        lhs: HirId,
        rhs: HirId,
        span: SrcSpan,
    ) -> Ty {
        let lhs_ty = self.ty_of(lhs);
        if !self.is_place_expr(lhs) {
            self.report_not_assignable(self.hir.expr(lhs).span);
        }

        let rhs_ty = self.ty_of_expecting(rhs, lhs_ty);
        if let Err(err) = self.unifier.unify(&self.tcx, lhs_ty, rhs_ty) {
            DiagCtx::emit(
                Diagnostic::error(self.cx().show(err).to_string(), span)
                    .with_label("both sides of a compound assignment must have the same type"),
            );
            return self.tcx.unit();
        }

        let operand = self.unifier.root(lhs_ty);
        let produced = self.check_operator(op, operand, lhs.owner, span);
        // `foo += bar` stores the operator's result back into `foo`, so an operator that produces
        // something else -- `Eq` produces `bool` -- cannot be compounded.
        if let Err(err) = self.unifier.unify(&self.tcx, operand, produced) {
            DiagCtx::emit(
                Diagnostic::error(self.cx().show(err).to_string(), span).with_label(
                    "this operator does not produce the type it would be assigned back to",
                ),
            );
        }
        self.tcx.unit()
    }

    /// Checks `&operand` and `&mut operand`.
    ///
    /// An expectation of `&T` is passed down as `T`, so `let p: &Pair = &.{ x: 1 };` reaches the
    /// struct literal with something to name it by.
    pub(crate) fn check_borrow(
        &mut self,
        mutability: Mutability,
        operand: HirId,
        expected: Option<Ty>,
    ) -> Ty {
        let inner = expected.and_then(|expected| match *self.tcx.kind(expected) {
            TyKind::Ref { base, mutability: m } if m == mutability => Some(base),
            _ => None,
        });
        let ty = self.ty_of_maybe_expecting(operand, inner);
        self.tcx.mk_ref(ty, mutability)
    }

    // -----------------------------------------------------------------
    // Indexing
    // -----------------------------------------------------------------

    /// Checks `base[index]`.
    ///
    /// An array is indexed built-in, exactly as a primitive is added built-in: no `extend` block
    /// backs `[i32; 4]`, so there is nothing for the solver to find. Everything else goes through
    /// the `index` method of the [`LangItem::Index`] trait, dispatched by the same machinery a
    /// written `base.index(index)` would use -- which is what makes an `extend<K, V> Map<K, V>
    /// with Index<K, V>` block apply here, including reading `V` back out of the block's
    /// arguments.
    ///
    /// So the type of `m[k]` is whatever that trait's `index` returns, which `lib/core/ops.phi`
    /// declares as `&V`. Nothing here inserts a dereference: this pass has no adjustment for one,
    /// and inventing a deref that no later pass would carry out would make the recorded type a
    /// lie.
    pub(crate) fn check_index(&mut self, base: HirId, index: HirId, span: SrcSpan) -> Ty {
        let base_ty = self.ty_of(base);
        let base_ty = self.resolve_deep(base_ty);

        if matches!(self.tcx.kind(base_ty), TyKind::Error) {
            self.ty_of(index);
            return self.tcx.error();
        }
        if matches!(self.tcx.kind(base_ty), TyKind::Var(_)) {
            self.report_index_base_unknown(self.hir.expr(base).span);
            self.ty_of(index);
            return self.tcx.error();
        }

        // A reference to an array indexes as the array does, the same way a reference to a struct
        // reaches its fields.
        let (peeled, _layers) = self.peel_receiver(base_ty);
        if let TyKind::Array { elem, .. } = *self.tcx.kind(peeled) {
            let int = self.tcx.next_int_var();
            let index_ty = self.ty_of(index);
            if let Err(err) = self.unifier.unify(&self.tcx, int, index_ty) {
                DiagCtx::emit(
                    Diagnostic::error(self.cx().show(err).to_string(), span)
                        .with_label("an array is indexed by an integer"),
                );
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
            self.report_not_indexable(peeled, span);
            self.ty_of(index);
            return self.tcx.error();
        }
        self.check_method_call(base, member, &[index], span)
    }

    // -----------------------------------------------------------------
    // Building a nominal value
    // -----------------------------------------------------------------

    /// Checks a struct literal: `Pair { fst: 1, snd: 2 }`, or the elided `.{ fst: 1, snd: 2 }`.
    ///
    /// The written form names its struct; the elided form has only the expectation, which is
    /// the reason the form exists. Either way the struct's generic arguments are inference
    /// variables the field initializers settle -- `Wrap { inner: 1 }` is `Wrap<{integer}>` until
    /// something says otherwise -- so a written path is unified with the expectation as well, which
    /// is what makes `let w: Wrap<i32> = Wrap { inner: 1 };` pin `T` from the annotation.
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
            self.report_not_a_struct_literal(self_ty, span);
            for field in payload {
                self.ty_of(field.value);
            }
            return self.tcx.error();
        };

        let mut written = HashSet::new();
        for field in payload {
            if !written.insert(field.name.text) {
                self.report_duplicate_field(field.name);
            }
            match declared.iter().find(|(name, _)| name.text == field.name.text) {
                Some(&(_, want)) => {
                    let got = self.ty_of_expecting(field.value, want);
                    if let Err(err) = self.unifier.unify(&self.tcx, want, got) {
                        DiagCtx::emit(
                            Diagnostic::error(
                                self.cx().show(err).to_string(),
                                self.hir.expr(field.value).span,
                            )
                            .with_label("this value does not match the field's declared type"),
                        );
                    }
                }
                None => {
                    self.report_no_such_field(field.name, self_ty);
                    self.ty_of(field.value);
                }
            }
        }

        // Every field has to be given a value: a struct with a field left out is not a value of
        // that struct, and there is no default to fall back on.
        let missing: Vec<&'static str> = declared
            .iter()
            .filter(|(name, _)| !written.contains(&name.text))
            .map(|(name, _)| Interner::resolve(name.text))
            .collect();
        if !missing.is_empty() {
            self.report_missing_fields(&missing, self_ty, span);
        }

        self_ty
    }

    /// Which struct a [`ExprKind::Ctor`](crate::hir::ExprKind::Ctor) builds: the one its path names, or -- for the elided
    /// form -- the one the expectation is.
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
                    self.report_elided_ctor_unknown(span);
                    None
                }
            };
        };

        match path.res {
            Res::Type(Type::Def(TyDef::Struct(def))) => {
                // The literal writes no argument list, so the struct's parameters start out as
                // variables. Unifying with the expectation is what settles them from an
                // annotation; a failure is left for the site that set the expectation to report,
                // since it is the one that knows what to say about it.
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
            // Already reported by name resolution.
            Res::Err => None,
            _ => {
                self.report_ctor_not_a_struct(span);
                None
            }
        }
    }

    /// Checks an enum variant being built: `.none`, `.circle(1.0)`, `.square { l: 2.0 }`.
    ///
    /// A variant names no enum of its own -- see [`Res`], which deliberately has no `Variant` arm
    /// -- so the expectation is the only thing that says which enum is meant. That is what makes
    /// `fun f() -> Result<Option<i32>, bool> { return .ok(.none); }` check: the return type reaches
    /// `.ok`, whose declared payload then reaches `.none`.
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
                self.report_variant_enum_unknown(variant, span);
                self.check_payload_exprs_only(payload);
                return self.tcx.error();
            }
        };

        let Some(found) = self.lookup_variant(self_ty, variant.text) else {
            self.report_no_such_variant(variant, self_ty);
            self.check_payload_exprs_only(payload);
            return self.tcx.error();
        };

        match (&found.payload, payload) {
            (VariantTys::Unit, Payload::None) => {}
            (VariantTys::Single(want), Payload::Single(value)) => {
                let want = *want;
                let got = self.ty_of_expecting(*value, want);
                if let Err(err) = self.unifier.unify(&self.tcx, want, got) {
                    DiagCtx::emit(
                        Diagnostic::error(
                            self.cx().show(err).to_string(),
                            self.hir.expr(*value).span,
                        )
                        .with_label("this value does not match the variant's declared payload"),
                    );
                }
            }
            (VariantTys::Record(want), Payload::Record(fields)) => {
                let want = want.clone();
                self.check_variant_record(&want, fields, found.id);
            }
            _ => {
                let declared = found.payload.describe();
                let variant_span = self.hir.variant(found.id).span;
                DiagCtx::emit(
                    Diagnostic::error(
                        format!(
                            "variant `{}` carries {declared}",
                            Interner::resolve(variant.text)
                        ),
                        span,
                    )
                    .with_label(format!("built with a payload that is not {declared}"))
                    .with_secondary(variant_span, "declared here"),
                );
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
                self.report_duplicate_field(field.name);
            }
            match declared.iter().find(|(name, _)| name.text == field.name.text) {
                Some(&(_, want)) => {
                    let got = self.ty_of_expecting(field.value, want);
                    if let Err(err) = self.unifier.unify(&self.tcx, want, got) {
                        DiagCtx::emit(
                            Diagnostic::error(
                                self.cx().show(err).to_string(),
                                self.hir.expr(field.value).span,
                            )
                            .with_label("this value does not match the field's declared type"),
                        );
                    }
                }
                None => {
                    let variant_span = self.hir.variant(variant).span;
                    DiagCtx::emit(
                        Diagnostic::error(
                            format!(
                                "no field `{}` on this variant",
                                Interner::resolve(field.name.text)
                            ),
                            field.name.span,
                        )
                        .with_label("not declared by this variant")
                        .with_secondary(variant_span, "declared here"),
                    );
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
            let variant_span = self.hir.variant(variant).span;
            DiagCtx::emit(
                Diagnostic::error(
                    format!("this variant's payload is missing {}", list(&missing)),
                    variant_span,
                )
                .with_label("every declared field has to be given a value"),
            );
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

    /// Checks `if cond { .. } else { .. }`.
    ///
    /// An `else if` chain lowered to `else { if .. }`, so there is only ever one `else` block here
    /// however long the chain was.
    ///
    /// With both branches present the `if` is an expression and the two have to agree, which is
    /// what its type is. With only one branch there is nothing for a value to be on the path
    /// not taken, so the `if` produces `Unit` and its block has to as well.
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
            DiagCtx::emit(
                Diagnostic::error(self.cx().show(err).to_string(), self.hir.expr(cond).span)
                    .with_label("an `if` condition has to be a `bool`"),
            );
        }

        let then_ty = self.check_block_expecting(then_block, expected);
        let Some(else_block) = else_block else {
            let unit = self.tcx.unit();
            if let Err(err) = self.unifier.unify(&self.tcx, unit, then_ty) {
                DiagCtx::emit(
                    Diagnostic::error(self.cx().show(err).to_string(), span)
                        .with_label("an `if` with no `else` produces no value")
                        .with_help(
                            "the block's last expression would be the `if`'s value, and there is \
                             no `else` branch to produce one on the other path",
                        ),
                );
            }
            return unit;
        };

        let else_ty = self.check_block_expecting(else_block, expected);
        if let Err(err) = self.unifier.unify(&self.tcx, then_ty, else_ty) {
            DiagCtx::emit(
                Diagnostic::error(self.cx().show(err).to_string(), span)
                    .with_label("both branches of an `if` have to produce the same type"),
            );
            return self.tcx.error();
        }
        self.unifier.root(then_ty)
    }

    /// Checks `match scrutinee { pat => { .. }, .. }`.
    ///
    /// Every arm's pattern is checked against the scrutinee's type -- which is what binds the names
    /// each arm's body then uses -- and every arm's body against one type, the `match` result's type.
    ///
    /// Whether the arms *cover* the scrutinee is not checked here. Exhaustiveness is a question
    /// about the set of patterns rather than about any one of them, and needs a pass that can see
    /// them all at once.
    pub(crate) fn check_match(
        &mut self,
        scrutinee: HirId,
        arms: &'hir [HirId],
        expected: Option<Ty>,
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

        for &arm in arms {
            let arm_node = self.hir.arm(arm);
            let (pat, block, arm_span) = (arm_node.pat, arm_node.block, arm_node.span);

            self.check_pat(pat, scrutinee_ty);
            let body = self.check_block_expecting(block, Some(result));
            if let Err(err) = self.unifier.unify(&self.tcx, result, body) {
                DiagCtx::emit(
                    Diagnostic::error(self.cx().show(err).to_string(), arm_span)
                        .with_label("every arm of a `match` has to produce the same type"),
                );
            }
        }

        self.unifier.root(result)
    }

    // -----------------------------------------------------------------
    // Error propagation
    // -----------------------------------------------------------------

    /// Checks `operand?`.
    ///
    /// Two shapes, both keyed on a lang item: a `Result<T, E>` produces `T` and propagates `E`, an
    /// `Option<T>` produces `T` and propagates `none`. Which means the enclosing function's return
    /// type has to be able to carry what is propagated -- there is nowhere else for it to go -- so
    /// that is checked here rather than left to whatever the `?` desugars into later.
    pub(crate) fn check_try(&mut self, operand: HirId, span: SrcSpan, owner: DefId) -> Ty {
        let operand_ty = self.ty_of(operand);
        let operand_ty = self.resolve_deep(operand_ty);

        if matches!(self.tcx.kind(operand_ty), TyKind::Error) {
            return self.tcx.error();
        }
        if matches!(self.tcx.kind(operand_ty), TyKind::Var(_)) {
            self.report_try_operand_unknown(span);
            return self.tcx.error();
        }

        let TyKind::Adt { def, args } = self.tcx.kind(operand_ty).clone() else {
            self.report_not_try(operand_ty, span);
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

        self.report_not_try(operand_ty, span);
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
            self.report_try_outside(operand_ty, span);
            return;
        };
        let ret = self.resolve_deep(ret);
        if matches!(self.tcx.kind(ret), TyKind::Error) {
            return;
        }

        let expected_def = self.hir.lang_items().get(item);
        let TyKind::Adt { def, args } = self.tcx.kind(ret).clone() else {
            self.report_try_return_mismatch(operand_ty, ret, span);
            return;
        };
        if Some(def) != expected_def {
            self.report_try_return_mismatch(operand_ty, ret, span);
            return;
        }

        // A `Result`'s error type leaves the function, so it is the one thing
        // about the return type that has to match exactly rather than merely be the same enum.
        if let (Some(error_ty), Some(ret_error)) = (error_ty, args.get(1).copied())
            && let Err(err) = self.unifier.unify(&self.tcx, ret_error, error_ty)
        {
            DiagCtx::emit(
                Diagnostic::error(self.cx().show(err).to_string(), span).with_label(
                    "`?` propagates this error out of the function, whose declared error type it \
                     has to match",
                ),
            );
        }
    }

    /// The return type the definition `owner` names declares, if it declares one.
    ///
    /// Reads the signature the table holds rather than the HIR, so it answers for a closure as
    /// well as a function -- [`Typeck::check_closure`] records one under the closure's
    /// definition before checking its body, exactly so that a `return` or a `?` inside it has
    /// something to check against.
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
    ///
    /// Both endpoints are checked and required to agree, and then there is nothing to give the
    /// expression as a type: a range is a value of some `Range` type, and the core library
    /// declares none -- there is no lang item for one either. So this reports rather than
    /// inventing a type no later pass could lower.
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
            DiagCtx::emit(
                Diagnostic::error(self.cx().show(err).to_string(), span)
                    .with_label("a range's two endpoints have to have the same type"),
            );
        }

        self.report_no_range_type(span);
        self.tcx.error()
    }

    // -----------------------------------------------------------------
    // Closures
    // -----------------------------------------------------------------

    /// Checks a closure literal and produces its function type.
    ///
    /// A closure owns its arena, so this is the one place a body is checked from inside
    /// another body rather than from the stage-two walk -- which is why
    /// [`Check::visit_closure`](crate::typeck::Typeck) asserts it is never reached from there.
    ///
    /// What a closure may leave out is what makes this more than a second `check_function`. An
    /// unannotated parameter takes its type from the expectation if the context supplied a
    /// function type of the right arity, and otherwise starts as an inference variable the body
    /// settles. The return type is always recorded as a variable before the body is checked, even
    /// when it was declared, so that a `return` inside the body has a signature to check against
    /// exactly as it would in a function.
    pub(crate) fn check_closure(&mut self, def: DefId, expected: Option<Ty>) -> Ty {
        let hir: &'hir Hir = self.hir;
        let OwnerNode::Closure(closure) = hir.def(def) else {
            unreachable!("root of a Closure owner is always OwnerNode::Closure");
        };

        // Only a function type of matching arity is a usable hint: a shorter or longer one says
        // nothing about which parameter is which, and reporting that mismatch belongs to whoever
        // set the expectation.
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
        let provisional = self.tcx.mk_fun(param_tys.clone(), Some(ret_var));
        self.types.record_def(def, provisional);

        let body = self.check_block_expecting(closure.block, Some(ret_var));
        if let Err(err) = self.unifier.unify(&self.tcx, ret_var, body) {
            DiagCtx::emit(
                Diagnostic::error(self.cx().show(err).to_string(), closure.span).with_label(
                    "this closure's body does not produce the return type it was checked against",
                ),
            );
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

    // -----------------------------------------------------------------
    // Diagnostics
    // -----------------------------------------------------------------

    fn report_not_assignable(&self, span: SrcSpan) {
        DiagCtx::emit(
            Diagnostic::error("this expression cannot be assigned to", span)
                .with_label("not a place")
                .with_help(
                    "the left side of an assignment has to name somewhere a value lives -- a \
                     local, a field, or an element -- rather than produce one",
                ),
        );
    }

    fn report_index_base_unknown(&self, span: SrcSpan) {
        DiagCtx::emit(
            Diagnostic::error(
                "type annotations needed: the type being indexed is still unknown",
                span,
            )
            .with_label("the type here is still unknown")
            .with_help(
                "what `[..]` means depends on the type it is written on: an array indexes \
                 built-in, and everything else through an `extend .. with Index` block",
            ),
        );
    }

    fn report_not_indexable(&self, base: Ty, span: SrcSpan) {
        DiagCtx::emit(
            Diagnostic::error(
                format!("`{}` cannot be indexed", self.cx().show(base)),
                span,
            )
            .with_label("no `index` method on this type")
            .with_help(
                "indexing an array is built in; every other type is indexed through an \
                 `extend .. with Index<K, V>` block",
            ),
        );
    }

    fn report_elided_ctor_unknown(&self, span: SrcSpan) {
        DiagCtx::emit(
            Diagnostic::error(
                "type annotations needed: `.{ .. }` names no struct, and the type it is expected \
                 to produce is unknown here",
                span,
            )
            .with_label("cannot tell which struct this builds")
            .with_help(
                "write the struct's name instead, or give the surrounding binding, parameter, or \
                 return type an annotation",
            ),
        );
    }

    fn report_ctor_not_a_struct(&self, span: SrcSpan) {
        DiagCtx::emit(
            Diagnostic::error("only a struct can be built with `{ .. }`", span)
                .with_label("not a struct"),
        );
    }

    fn report_not_a_struct_literal(&self, ty: Ty, span: SrcSpan) {
        DiagCtx::emit(
            Diagnostic::error(
                format!("`{}` is not a struct", self.cx().show(ty)),
                span,
            )
            .with_label("only a struct is built with `{ .. }`")
            .with_help("an enum variant is built with `.variant`, not with a struct literal"),
        );
    }

    fn report_no_such_field(&self, field: Ident, ty: Ty) {
        DiagCtx::emit(
            Diagnostic::error(
                format!(
                    "no field `{}` on `{}`",
                    Interner::resolve(field.text),
                    self.cx().show(ty)
                ),
                field.span,
            )
            .with_label("not a field of this struct"),
        );
    }

    fn report_duplicate_field(&self, field: Ident) {
        DiagCtx::emit(
            Diagnostic::error(
                format!(
                    "field `{}` is given a value twice",
                    Interner::resolve(field.text)
                ),
                field.span,
            )
            .with_label("already given a value above"),
        );
    }

    fn report_missing_fields(&self, missing: &[&str], ty: Ty, span: SrcSpan) {
        DiagCtx::emit(
            Diagnostic::error(
                format!(
                    "`{}` is missing {}",
                    self.cx().show(ty),
                    list(missing)
                ),
                span,
            )
            .with_label("every field has to be given a value"),
        );
    }

    fn report_variant_enum_unknown(&self, variant: Ident, span: SrcSpan) {
        DiagCtx::emit(
            Diagnostic::error(
                format!(
                    "type annotations needed: the enum `.{}` belongs to is unknown here",
                    Interner::resolve(variant.text)
                ),
                span,
            )
            .with_label("cannot tell which enum this variant belongs to")
            .with_help(
                "a `.variant` takes its enum from the type it is expected to produce -- from a \
                 binding's annotation, a parameter, or the enclosing function's return type",
            ),
        );
    }

    fn report_no_such_variant(&self, variant: Ident, ty: Ty) {
        DiagCtx::emit(
            Diagnostic::error(
                format!(
                    "no variant `{}` on `{}`",
                    Interner::resolve(variant.text),
                    self.cx().show(ty)
                ),
                variant.span,
            )
            .with_label("not a variant of this type"),
        );
    }

    fn report_try_operand_unknown(&self, span: SrcSpan) {
        DiagCtx::emit(
            Diagnostic::error(
                "type annotations needed: the type `?` is applied to is still unknown",
                span,
            )
            .with_label("the type here is still unknown")
            .with_help("`?` produces what a `Result` or an `Option` carries, so it needs one"),
        );
    }

    fn report_not_try(&self, ty: Ty, span: SrcSpan) {
        DiagCtx::emit(
            Diagnostic::error(
                format!("`?` cannot be applied to `{}`", self.cx().show(ty)),
                span,
            )
            .with_label("not a `Result` or an `Option`")
            .with_help("`?` takes the value out of a `Result` or an `Option`, propagating the rest"),
        );
    }

    fn report_try_outside(&self, operand_ty: Ty, span: SrcSpan) {
        DiagCtx::emit(
            Diagnostic::error(
                format!(
                    "`?` on `{}` has nowhere to propagate to",
                    self.cx().show(operand_ty)
                ),
                span,
            )
            .with_label("the enclosing definition declares no return type")
            .with_help(
                "`?` returns early on the failing case, so the enclosing function has to return \
                 the same kind of value",
            ),
        );
    }

    fn report_try_return_mismatch(&self, operand_ty: Ty, ret: Ty, span: SrcSpan) {
        DiagCtx::emit(
            Diagnostic::error(
                format!(
                    "`?` on `{}` cannot propagate out of a function returning `{}`",
                    self.cx().show(operand_ty),
                    self.cx().show(ret)
                ),
                span,
            )
            .with_label("the two are not the same kind of value")
            .with_help(
                "`?` returns early with what it did not unwrap, so the enclosing function's return \
                 type has to be able to carry it",
            ),
        );
    }

    fn report_no_range_type(&self, span: SrcSpan) {
        DiagCtx::emit(
            Diagnostic::error("a range expression has no type yet", span)
                .with_label("`..` produces a value the core library declares no type for")
                .with_help(
                    "a range is a value of a `Range` type, and there is no such type in `core` \
                     and no lang item naming one; iterate with `for x in ..` over a collection \
                     instead",
                ),
        );
    }
}

/// `a`, `a and b`, `a, b and c` -- the field names in a diagnostic, read as a sentence.
fn list(names: &[&str]) -> String {
    let quoted: Vec<String> = names.iter().map(|name| format!("`{name}`")).collect();
    match quoted.split_last() {
        None => String::new(),
        Some((last, [])) => format!("field {last}"),
        Some((last, rest)) => format!("fields {} and {last}", rest.join(", ")),
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
        rejects(
            "fun f() { let x: bool = 1; }",
            "mismatched types",
        );
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
        rejects("fun f(x: i32) -> &mut i32 { return &x; }", "mismatched types");
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
        rejects("fun f(a: [i32; 4]) -> i32 { return a[true]; }", "mismatched types");
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
}
