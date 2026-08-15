use std::collections::{HashMap, HashSet};

use crate::ast::{BinaryOp, Literal, Mutability, SelfMode, UnaryOp, Visibility};
use crate::diagnostics::typeck::display::DisplayCx;
use crate::diagnostics::typeck::pat::{
    report_irrefutable_let_with_else, report_refutable_let_without_else,
};
use crate::diagnostics::typeck::{
    report_binary_operand_mismatch, report_binding_type_mismatch, report_body_return_mismatch,
    report_int_suffix_on_float_literal, report_logic_op_needs_bool_operands,
    report_operand_has_unknown_type, report_return_mismatch, report_str_literal_untyped,
    report_unknown_literal_suffix,
};
use crate::driver::source::SrcSpan;
use crate::hir::visit::{self, Visitor};
use crate::hir::{
    DefId, ExprKind, Hir, HirId, Local, OwnerNode, PatKind, Payload, Res, StmtKind,
    VariantPayload,
};
use crate::langitems::LangItem;
use crate::nameres::symbol_table::prim_ty;
use crate::nameres::PrimTy;
use crate::typeck::results::TypeResolutions;
use crate::typeck::traits::bounds::ObligationCx;
use crate::typeck::traits::index::ImplIndex;
use crate::typeck::traits::solve::{Obligation, ParamEnv, TraitName};
use crate::typeck::ty::{Ty, TyKind, TyVar};
use crate::typeck::tyctx::TyCtx;
use crate::typeck::unify::{is_float, is_integer, Unifier, UnifyError};

pub mod cast;
pub mod expr;
pub mod lower_ty;
pub mod pat;
pub mod results;
pub mod traits;
pub mod ty;
pub mod tyctx;
pub mod unify;

pub struct Typeck<'hir> {
    hir: &'hir Hir,

    tcx: TyCtx,
    types: TypeResolutions,
    unifier: Unifier,

    /// Every `extend` block in the program, keyed for lookup. Empty until
    /// [`Typeck::build_impl_index`] runs, which is why nothing may ask the solver a question
    /// before then.
    impls: ImplIndex,

    /// What each definition may assume about its type parameters, determined on first use.
    /// See [`ParamEnv`].
    param_envs: HashMap<DefId, ParamEnv>,

    /// The trait goals currently being proved, outermost first. A goal that turns up while it is
    /// already on here is a cyclic bound, and the depth of the stack is what the solver's
    /// recursion limit counts; see [`traits::solve`].
    goal_stack: Vec<Obligation>,

    /// Bounds raised while collecting signatures, before the impl index existed to prove them
    /// against. Drained once, immediately after coherence; see [`traits::bounds`].
    program_obligations: ObligationCx,

    /// Bounds raised while checking one function body, drained at the end of it -- the first
    /// moment that body's inference has settled.
    body_obligations: ObligationCx,

    /// Which of the two contexts a registration goes to. A registration site cannot tell them
    /// apart on its own: lowering an annotation is the same act during collection and inside a
    /// body, and only the surrounding phase differs.
    in_body: bool,

    /// What `Self` lowers to inside each definition that introduces one, cached because `Self` is
    /// typically written many times in one body. Filled in on demand by
    /// [`Typeck::self_ty`](crate::typeck::Typeck).
    self_tys: HashMap<DefId, Ty>,

    /// The definitions whose `Self` is being computed right now, used to cut off a `Self` that
    /// is defined in terms of itself instead of recursing forever.
    computing_self_tys: HashSet<DefId>,

    /// What the context around the expression *about to be checked* demands of it, if it demands
    /// anything. Set by [`Typeck::ty_of_expecting`] for the duration of one call and taken by
    /// [`Typeck::check_expr`] at its top, so exactly one expression ever sees it -- a child that
    /// happens to be checked underneath does not inherit it, and an arm that wants to pass it on
    /// does so explicitly.
    ///
    /// A field rather than a parameter because [`Typeck::ty_of`] sits between the two: it is
    /// where a type enters the table, so every expectation has to travel through it, and giving it
    /// a second parameter would put one on every call site that has nothing to expect. See the
    /// [`expr` module docs](crate::typeck::expr) for what depends on this.
    expectation: Option<Ty>,

    /// Every binding a `let` pattern introduces, keyed by the same `HirId` its `Res::Local(
    /// Local::Variable(_))` addresses, mapped to the `mut`-ness the `let` itself declared.
    ///
    /// Populated only from `StmtKind::Let` (see [`Typeck::check_binding`]); a binding from a
    /// `match` arm, `for`, `while let`, or a `with` lend resolves through the very same
    /// `Local::Variable` arm but is never entered here, since none of those forms has `mut`
    /// syntax of its own to declare intent either way. A lookup that misses this table is read as
    /// "not a `let` binding" everywhere it is consulted, and left unrestricted.
    let_mutability: HashMap<HirId, Mutability>,
}

impl<'hir> Typeck<'hir> {
    pub fn new(hir: &'hir Hir) -> Self {
        Typeck {
            hir,
            tcx: TyCtx::new(),
            types: TypeResolutions::new(),
            unifier: Unifier::new(),
            impls: ImplIndex::new(),
            param_envs: HashMap::new(),
            goal_stack: Vec::new(),
            program_obligations: ObligationCx::new(),
            body_obligations: ObligationCx::new(),
            in_body: false,
            self_tys: HashMap::new(),
            computing_self_tys: HashSet::new(),
            expectation: None,
            let_mutability: HashMap::new(),
        }
    }

    pub fn collect_module(&mut self, module_id: DefId) {
        Collect(self).visit_module(module_id);
    }

    pub fn collect_function(&mut self, function: DefId) {
        // this allows us to avoid clones
        let hir: &'hir Hir = self.hir;
        let function_node = hir.function(function);
        let (generics, self_param, params, ret) = (
            &function_node.generics,
            function_node.self_param,
            &function_node.params,
            function_node.ret,
        );

        self.collect_generics(generics);

        // We desugar so we only have to check the signature
        let mut param_tys = Vec::with_capacity(params.len() + usize::from(self_param.is_some()));
        if let Some(id) = self_param {
            param_tys.push(self.collect_self_param(id));
        }
        for &id in params {
            let param = hir.param(id);

            let ty = self.lower_ty(param.ty);
            self.types.record(id, ty);
            param_tys.push(ty);
        }
        let ret = ret.map(|ret| self.lower_ty(ret));

        let sig = self.tcx.mk_fun(param_tys, ret);
        self.types.record_def(function, sig);
    }

    fn collect_self_param(&mut self, id: HirId) -> Ty {
        let self_param = self.hir.self_param(id);
        let (mode, span) = (self_param.mode, self_param.span);

        let self_ty = self.self_ty(id.owner, span);
        let ty = match mode {
            SelfMode::Immutable => self.tcx.mk_ref(self_ty, Mutability::Immutable),
            SelfMode::Mutable => self.tcx.mk_ref(self_ty, Mutability::Mutable),
            SelfMode::Move => self_ty,
            SelfMode::Any => self.tcx.mk_any(self_ty),
        };
        self.types.record(id, ty);
        ty
    }

    pub fn collect_struct(&mut self, r#struct: DefId) {
        let hir: &'hir Hir = self.hir;
        let struct_node = hir.struct_(r#struct);
        let (generics, fields, span) =
            (&struct_node.generics, &struct_node.fields, struct_node.span);

        // The generics have to be recorded first: the struct's type is itself applied to
        // them.
        self.collect_generics(generics);
        let self_ty = self.self_ty(r#struct, span);
        self.types.record_def(r#struct, self_ty);

        self.collect_fields(fields);
    }

    pub fn collect_enum(&mut self, r#enum: DefId) {
        let hir: &'hir Hir = self.hir;
        let enum_node = hir.enum_(r#enum);
        let (generics, variants, span) = (&enum_node.generics, &enum_node.variants, enum_node.span);

        self.collect_generics(generics);
        let self_ty = self.self_ty(r#enum, span);
        self.types.record_def(r#enum, self_ty);

        for &id in variants {
            let variant = hir.variant(id);

            match &variant.payload {
                VariantPayload::Unit => {}
                VariantPayload::Type(ty_id) => {
                    let ty = self.lower_ty(*ty_id);
                    self.types.record(id, ty);
                }
                VariantPayload::Record(fields) => self.collect_fields(fields),
            }
        }
    }

    pub fn collect_trait(&mut self, r#trait: DefId) {
        let hir: &'hir Hir = self.hir;
        let trait_node = hir.trait_(r#trait);
        let (generics, span) = (&trait_node.generics, trait_node.span);

        self.collect_generics(generics);
        // A trait names no type of its own, so what it gets recorded as is the `Self` it stands
        // for: the placeholder every implementing type substitutes.
        let self_ty = self.self_ty(r#trait, span);
        self.types.record_def(r#trait, self_ty);
    }

    pub fn collect_extend(&mut self, extend: DefId) {
        let hir: &'hir Hir = self.hir;
        let extend_node = hir.extend(extend);
        let (extend_generics, adt_generics, trait_generics, span) = (
            &extend_node.extend_generics,
            &extend_node.adt_generics,
            &extend_node.trait_generics,
            extend_node.span,
        );

        // The first group declares parameters, the other two apply arguments
        self.collect_generics(extend_generics);
        self.lower_tys(adt_generics);
        self.lower_tys(trait_generics);

        // Which is the extended type applied to `adt_generics`, so this is also what `Self`
        // means inside each method of the block.
        let self_ty = self.self_ty(extend, span);
        self.types.record_def(extend, self_ty);
    }

    fn collect_generics(&mut self, generics: &[HirId]) {
        for &id in generics {
            self.hir.generic(id);

            let ty = self.tcx.mk_generic(id);
            self.types.record(id, ty);
        }
    }

    fn collect_fields(&mut self, fields: &[HirId]) {
        for &id in fields {
            let field = self.hir.field(id);

            let ty = self.lower_ty(field.ty);
            self.types.record(id, ty);
        }
    }

    //-------------------------------------------------------------------------

    pub fn check_module(&mut self, module: DefId) {
        Check(self).visit_module(module);
    }

    /// The type of the expression `id` names, worked out on first use and remembered afterwards.
    ///
    /// This is the only place a type enters the table for an expression, and the only way one is
    /// read back out, which is what keeps the table and the unifier in step. A node cannot be
    /// checked without its type being recorded, because recording is what this does; and a
    /// recorded type cannot be read while it is still an unresolved inference variable, because
    /// every read goes through [`Unifier::root`].
    fn ty_of(&mut self, id: HirId) -> Ty {
        if let Some(ty) = self.types.ty(id) {
            return self.unifier.root(ty);
        }

        let ty = self.check_expr(id);
        self.types.record(id, ty);
        ty
    }

    /// [`Typeck::ty_of`], telling the expression what type the context around it wants.
    ///
    /// The expectation is a hint, not a constraint: this does not unify `expected` with what came
    /// back, because what to say when the two disagree is the caller's to decide -- a `let`'s
    /// annotation, an argument, and a `return` each report it differently. What it does is give
    /// the forms that name no type of their own something to be checked against; see the
    /// [`expr` module docs](crate::typeck::expr).
    ///
    /// The previous expectation is restored rather than cleared, so that an arm which checks one
    /// child with an expectation and another without does not have to put it back by hand.
    fn ty_of_expecting(&mut self, id: HirId, expected: Ty) -> Ty {
        let saved = self.expectation.replace(expected);
        let ty = self.ty_of(id);
        self.expectation = saved;
        ty
    }

    /// [`Typeck::ty_of_expecting`] where the caller may or may not have an expectation to pass on,
    /// which is the shape most of them are in: a `let` has one only if it was annotated, and an
    /// argument only if the signature it is measured against lined up.
    fn ty_of_expecting_opt(&mut self, id: HirId, expected: Option<Ty>) -> Ty {
        match expected {
            Some(expected) => self.ty_of_expecting(id, expected),
            None => self.ty_of(id),
        }
    }

    /// The type already recorded for `id`, resolved to what it has since unified with.
    ///
    /// Unlike [`Typeck::ty_of`] this never computes anything, because the nodes it is asked
    /// about -- a parameter, a `self`, a field -- are typed by the `collect_*` stage from what
    /// the user wrote, not by checking an expression.
    fn recorded_ty(&mut self, id: HirId) -> Option<Ty> {
        let ty = self.types.ty(id)?;
        Some(self.unifier.root(ty))
    }

    /// [`Typeck::recorded_ty`] for a definition's type: a `struct`'s type, a `fun`'s
    /// signature.
    fn recorded_ty_of_def(&mut self, def: DefId) -> Option<Ty> {
        let ty = self.types.ty_of_def(def)?;
        Some(self.unifier.root(ty))
    }

    /// Replaces every type recorded for `owner`'s nodes with its fully resolved form, once that
    /// owner's body has been checked.
    ///
    /// Within the pass, reading through [`Typeck::ty_of`] is enough to never see a stale
    /// inference variable. Afterwards there is no unifier to read through -- [`check`] hands back
    /// the table and the [`TyCtx`], not the union-find -- so the resolution is baked in here
    /// instead, and every consumer downstream reads plain types.
    fn writeback(&mut self, owner: DefId) {
        let entries: Vec<(HirId, Ty)> = self
            .types
            .iter()
            .filter(|(id, _)| id.owner == owner)
            .collect();

        for (id, ty) in entries {
            let resolved = self.resolve_deep(ty);
            let defaulted = self.default_unconstrained(resolved);
            self.types.record(id, defaulted);
        }

        // A resolved call's own `args` are recorded mid-checking (see
        // `TypeResolutions::record_call`'s call sites), before either of the above ever run on
        // them, so they need the same two-step treatment here.
        let call_entries: Vec<(HirId, DefId, Vec<Ty>)> = self
            .types
            .calls_iter()
            .filter(|(id, _)| id.owner == owner)
            .map(|(id, call)| (id, call.def, call.args.clone()))
            .collect();
        for (id, def, args) in call_entries {
            let defaulted = args
                .iter()
                .map(|&arg| {
                    let resolved = self.resolve_deep(arg);
                    self.default_unconstrained(resolved)
                })
                .collect();
            self.types.record_call(id, def, defaulted);
        }
    }

    /// Resolves `ty` and everything inside it: an `Adt`'s arguments, a tuple's elements, a
    /// function type's parameters, and so on.
    ///
    /// [`Unifier::root`] only answers for the type it is handed. A `Vec<T>` whose `T` unified
    /// with `i32` is still its own representative, so resolving it means rebuilding it from
    /// resolved parts.
    fn resolve_deep(&mut self, ty: Ty) -> Ty {
        let ty = self.unifier.root(ty);

        match self.tcx.kind(ty).clone() {
            TyKind::Adt { def, args } => {
                let args = self.resolve_deep_all(&args);
                self.tcx.mk_adt(def, args)
            }
            TyKind::Dyn { trait_, args } => {
                let args = self.resolve_deep_all(&args);
                self.tcx.mk_dyn(trait_, args)
            }
            TyKind::Tuple(elems) => {
                let elems = self.resolve_deep_all(&elems);
                self.tcx.mk_tuple(elems)
            }
            TyKind::Ref { base, mutability } => {
                let base = self.resolve_deep(base);
                self.tcx.mk_ref(base, mutability)
            }
            TyKind::Any(base) => {
                let base = self.resolve_deep(base);
                self.tcx.mk_any(base)
            }
            TyKind::Array { elem, len } => {
                let elem = self.resolve_deep(elem);
                self.tcx.mk_array(elem, len)
            }
            TyKind::Fun { params, ret } => {
                let params = self.resolve_deep_all(&params);
                let ret = ret.map(|ret| self.resolve_deep(ret));
                self.tcx.mk_fun(params, ret)
            }
            // Nothing nested to resolve. A `Var` that reaches here is one nothing ever unified
            // with, so it stays a variable -- a call site partway through checking a body (an
            // ambiguous-receiver check, say) still needs to see a genuinely unconstrained
            // variable as itself. Defaulting an unconstrained numeric variable to `i32`/`f64`
            // happens once, at [`Typeck::writeback`], via [`Typeck::default_unconstrained`].
            TyKind::Var(_)
            | TyKind::Primitive(_)
            | TyKind::Generic(_)
            | TyKind::SelfTy(_)
            | TyKind::Unit
            | TyKind::Never
            | TyKind::Error => ty,
        }
    }

    /// Defaults every unconstrained `TyVar::Int`/`TyVar::Float` still inside `ty` to `i32`/`f64`,
    /// the way an unsuffixed integer or float literal defaults when nothing else pins its type
    /// down. Called only from [`Typeck::writeback`], once a definition's body has finished
    /// checking and `ty` has already been through [`Typeck::resolve_deep`] -- everywhere else,
    /// a variable reaching here would still need to read as genuinely unconstrained.
    ///
    /// `TyVar::Any` is left untouched: reaching here as a bare variable means some expression's
    /// type was never constrained by anything at all, which is a typeck bug, not a legitimate
    /// unconstrained-literal case, so it is not guessed at. MIR lowering treats any `TyKind::Var`
    /// it still finds as exactly that: an internal-consistency panic, not a diagnostic.
    fn default_unconstrained(&mut self, ty: Ty) -> Ty {
        match self.tcx.kind(ty).clone() {
            TyKind::Var(TyVar::Int(_)) => self.tcx.mk_prim(PrimTy::I32),
            TyKind::Var(TyVar::Float(_)) => self.tcx.mk_prim(PrimTy::F64),
            TyKind::Adt { def, args } => {
                let args = args
                    .iter()
                    .map(|&a| self.default_unconstrained(a))
                    .collect();
                self.tcx.mk_adt(def, args)
            }
            TyKind::Dyn { trait_, args } => {
                let args = args
                    .iter()
                    .map(|&a| self.default_unconstrained(a))
                    .collect();
                self.tcx.mk_dyn(trait_, args)
            }
            TyKind::Tuple(elems) => {
                let elems = elems
                    .iter()
                    .map(|&a| self.default_unconstrained(a))
                    .collect();
                self.tcx.mk_tuple(elems)
            }
            TyKind::Ref { base, mutability } => {
                let base = self.default_unconstrained(base);
                self.tcx.mk_ref(base, mutability)
            }
            TyKind::Any(base) => {
                let base = self.default_unconstrained(base);
                self.tcx.mk_any(base)
            }
            TyKind::Array { elem, len } => {
                let elem = self.default_unconstrained(elem);
                self.tcx.mk_array(elem, len)
            }
            TyKind::Fun { params, ret } => {
                let params = params
                    .iter()
                    .map(|&a| self.default_unconstrained(a))
                    .collect();
                let ret = ret.map(|ret| self.default_unconstrained(ret));
                self.tcx.mk_fun(params, ret)
            }
            TyKind::Var(TyVar::Any(_))
            | TyKind::Primitive(_)
            | TyKind::Generic(_)
            | TyKind::SelfTy(_)
            | TyKind::Unit
            | TyKind::Never
            | TyKind::Error => ty,
        }
    }

    /// Unifies `found` against `expected`, the way [`Unifier::unify`] already does, except that
    /// an `expected` of `any T` additionally accepts a `found` that is `T`, `&T`, or `&mut T` --
    /// the coercion section 7 of the README describes: "any" lets a value be passed owned, by
    /// `&`, or by `&mut`, with the callee body using it uniformly. `found` is peeled by
    /// [`Typeck::peel_receiver`], the same peeling a method call's own receiver already gets, so
    /// this accepts exactly the forms a receiver would.
    ///
    /// This is not a general unification rule: `unify` itself only ever pairs `Any` against
    /// `Any`, so a plain call site still needs this wrapper wherever a declared type might be
    /// `any T` and the value offered for it might not already be. `check_receiver`'s own
    /// `SelfMode::Any` arm accepts every receiver form outright already, so `self` never needs
    /// this; it is for the two positions the README restricts `any` to, a parameter and a
    /// return.
    fn unify_allowing_any(&mut self, expected: Ty, found: Ty) -> Result<(), UnifyError> {
        if let TyKind::Any(inner) = *self.tcx.kind(expected) {
            let (peeled, _layers) = self.peel_receiver(found);
            return self.unifier.unify(&self.tcx, inner, peeled);
        }
        self.unifier.unify(&self.tcx, expected, found)
    }

    fn resolve_deep_all(&mut self, tys: &[Ty]) -> Vec<Ty> {
        tys.iter().map(|&ty| self.resolve_deep(ty)).collect()
    }

    /// Works out the type of the expression `id` names, without recording it. Private, and
    /// `#[must_use]`, because [`Typeck::ty_of`] is what puts a type in the table -- reaching
    /// this directly is how a computed type gets dropped on the floor.
    #[must_use]
    fn check_expr(&mut self, id: HirId) -> Ty {
        let expr = self.hir.expr(id);

        // Taken rather than read, so that the expectation reaches this expression and no other:
        // every `ty_of` below re-enters here with nothing set, and an arm that wants to pass it
        // down does so by name.
        let expected = self.expectation.take();

        let ty = match &expr.kind {
            ExprKind::Literal(lit) => self.check_literal(lit, expr.span),
            ExprKind::Tuple(elems) => {
                let tys = elems.iter().map(|&elem| self.ty_of(elem)).collect();
                self.tcx.mk_tuple(tys)
            }
            ExprKind::Path(path) => {
                match path.res {
                    // Every local was typed before this point: a parameter by `collect_function`,
                    // a `let`/`with` binding by `check_pat`, which records the type on the very
                    // `Node::Pat` that `Local::Variable` addresses.
                    //
                    // The fallback covers a use that somehow reaches its binding's pattern before
                    // the pattern was checked. One inference variable recorded against the
                    // pattern at least makes every use of that local agree with every other,
                    // instead of each one inventing a type of its own.
                    Res::Local(Local::Param(local) | Local::Variable(local)) => {
                        self.recorded_ty(local).unwrap_or_else(|| {
                            let ty = self.tcx.next_ty_var();
                            self.types.record(local, ty);
                            ty
                        })
                    }
                    Res::Local(Local::SelfParam(self_param)) => self
                        .recorded_ty(self_param)
                        .expect("collect_self_param always records the self parameter's type"),
                    Res::Function(def) => self
                        .recorded_ty_of_def(def)
                        .expect("collect_function always records a function's own signature"),
                    // Already reported by name resolution; staying quiet here keeps one mistake
                    // from producing a second diagnostic.
                    Res::Err => self.tcx.error(),

                    // Name resolution only ever resolves a value-position path to a local or a
                    // function (see `SymbolTable::lookup_value_path`); a type, a module, or
                    // `Self` can never come back from it. A variant is reached through `.v`,
                    // which lowers to `ExprKind::Access`, not through a path, so it is not among
                    // `Res`'s value-position answers either.
                    Res::Type(_) | Res::Module(_) | Res::SelfTy(_) => unreachable!(
                        "name resolution never resolves a value-position path to a type, a \
                         module, or Self"
                    ),
                }
            }
            ExprKind::Unary { op, operand } => {
                let operand_ty = self.ty_of(*operand);
                let resolved = self.unifier.root(operand_ty);

                let item = match op {
                    UnaryOp::Neg => LangItem::Neg,
                    UnaryOp::Not => LangItem::Not,
                };

                if self.is_builtin_operand(resolved) {
                    // `-`/`!` on `i32`/`bool` and friends -- or on a literal that is still only
                    // known to be numeric, which can only ever resolve to one of them -- are
                    // built in. No `extend` block backs a primitive, so there is nothing for
                    // the solver to find.
                    resolved
                } else if self.operator_holds(item, resolved, id.owner, expr.span) {
                    // Every `core::ops` trait an operator dispatches to returns `Self`, so the
                    // operand's own type is the result -- there is no associated type to project.
                    resolved
                } else {
                    self.tcx.error()
                }
            }
            ExprKind::Binary { op, lhs, rhs } => {
                let (lhs, rhs) = (self.ty_of(*lhs), self.ty_of(*rhs));
                if let Err(error) = self.unifier.unify(&self.tcx, lhs, rhs) {
                    report_binary_operand_mismatch(self.cx(), error, lhs, rhs, expr.span);
                    // The operands themselves are already reported as incompatible -- letting
                    // `check_operator` re-derive a bound or a `bool` requirement from whichever
                    // operand happened to be `lhs` would just restate the same mismatch a second
                    // time (worst offender: `&&`/`||`, which unify unconditionally against
                    // `bool`).
                    return self.tcx.error();
                }
                let resolved = self.unifier.root(lhs);
                self.check_operator(*op, resolved, id.owner, expr.span)
            }
            ExprKind::Assign { lhs, rhs } => self.check_assign(*lhs, *rhs, expr.span),
            ExprKind::AssignOp { op, lhs, rhs } => self.check_assign_op(*op, *lhs, *rhs, expr.span),
            ExprKind::Borrow {
                mutability,
                operand,
            } => self.check_borrow(*mutability, *operand, expected),
            ExprKind::Call { callee, args } => self.check_call(id, *callee, args, expr.span),
            ExprKind::Access { base, member, args } => {
                self.check_access(id, *base, *member, args, expr.span)
            }
            ExprKind::Index { base, index } => self.check_index(id, *base, *index, expr.span),
            ExprKind::Ctor { path, payload } => {
                self.check_ctor(path.as_ref(), payload, expected, expr.span, id.owner)
            }
            ExprKind::Variant { variant, payload } => {
                self.check_variant_expr(*variant, payload, expected, expr.span)
            }
            ExprKind::Range { lo, hi, .. } => self.check_range(*lo, *hi, expr.span),
            ExprKind::Try(operand) => self.check_try(*operand, expr.span, id.owner),
            ExprKind::If {
                cond,
                then_block,
                else_block,
            } => self.check_if(*cond, *then_block, *else_block, expected, expr.span),
            ExprKind::Match { scrutinee, arms } => {
                self.check_match(*scrutinee, arms, expected, expr.span)
            }
            ExprKind::Loop { block, .. } => {
                self.check_block(*block);
                // A `loop`/`while`/`for` expression produces no value of its own.
                self.tcx.unit()
            }
            // Both run their block for its effects rather than for a value: `spawn` starts it
            // elsewhere, and `concurrent` runs its statements against each other. Neither has a
            // value to hand back to the expression it sits in.
            ExprKind::Spawn(block) | ExprKind::Concurrent(block) => {
                self.check_block(*block);
                self.tcx.unit()
            }
            ExprKind::Block(block_id) => self.check_block_expecting(*block_id, expected),
            ExprKind::Closure(def) => self.check_closure(*def, expected),
            ExprKind::Cast { expr: operand, ty } => self.check_cast(*operand, *ty, expr.span),
            ExprKind::Error => self.tcx.error(),
        };

        ty
    }

    /// The type `op` produces for two operands that have already been unified into `operand`, and
    /// the check that `op` applies to that type at all.
    ///
    /// Each operator maps onto the `core::ops` trait its lang item names, so unifying the two
    /// sides is required but not sufficient: `foo + bar` also needs an `extend Foo with Add`
    /// block. A built-in operand ([`Typeck::is_builtin_operand`]) short-circuits that -- no
    /// `extend` block backs a primitive, so there would be nothing for the solver to find.
    ///
    /// Shared with [`Typeck::check_assign_op`], which asks the same question of `+=` as this does
    /// of `+`.
    fn check_operator(&mut self, op: BinaryOp, operand: Ty, owner: DefId, span: SrcSpan) -> Ty {
        // `any T` lets a value be passed owned, by `&`, or by `&mut`, used uniformly by the body
        // that holds it (README section 7) -- an operator is exactly such a use, so it is
        // checked, and (for the arithmetic operators, whose result is `Self`) resolved, against
        // the type `any` wraps, the same way `Typeck::peel_receiver` already treats `any` as a
        // layer to see through for a method call.
        let operand = self.peel_any(operand);
        let is_builtin = self.is_builtin_operand(operand);
        let bool_ty = self.tcx.mk_prim(PrimTy::Bool);

        match op {
            BinaryOp::Add | BinaryOp::Sub | BinaryOp::Mul | BinaryOp::Div | BinaryOp::Rem => {
                let item = match op {
                    BinaryOp::Add => LangItem::Add,
                    BinaryOp::Sub => LangItem::Sub,
                    BinaryOp::Mul => LangItem::Mul,
                    BinaryOp::Div => LangItem::Div,
                    BinaryOp::Rem => LangItem::Rem,
                    _ => unreachable!("the outer match admits only the five arithmetic operators"),
                };
                if is_builtin || self.operator_holds(item, operand, owner, span) {
                    // Every `core::ops` trait an operator dispatches to returns `Self`, so the
                    // operand's type is the result.
                    operand
                } else {
                    self.tcx.error()
                }
            }
            BinaryOp::Eq | BinaryOp::Ne => {
                if is_builtin || self.operator_holds(LangItem::Eq, operand, owner, span) {
                    bool_ty
                } else {
                    self.tcx.error()
                }
            }
            BinaryOp::Lt | BinaryOp::Le | BinaryOp::Gt | BinaryOp::Ge => {
                if is_builtin || self.operator_holds(LangItem::Comparable, operand, owner, span) {
                    bool_ty
                } else {
                    self.tcx.error()
                }
            }
            BinaryOp::And | BinaryOp::Or => {
                // Not overloadable -- the core lib has no logic trait, so `&&`/`||` only ever
                // mean the primitive short-circuit operators.
                if let Err(error) = self.unifier.unify(&self.tcx, operand, bool_ty) {
                    report_logic_op_needs_bool_operands(self.cx(), error, operand, span);
                }
                bool_ty
            }
        }
    }

    /// Whether `self_ty` implements the operator trait `item` names -- `Add`, `Neg`, `Eq`, and so
    /// on.
    ///
    /// Refuses outright, with its own diagnostic, when `self_ty` is still a wholly unresolved
    /// variable ([`TyVar::Any`]) -- never a numeric one, which
    /// [`Typeck::is_builtin_operand`] has already let through before this is reached. An
    /// operator's result feeds the expression around it immediately, with no later moment to
    /// retry against once inference has settled further, which is the same reason a method
    /// call's receiver is not deferred either (see [`method`](crate::typeck::traits::method)).
    /// Left to [`Typeck::require_extends`] instead, an unresolved variable reads as
    /// [`Solution::Ambiguous`](crate::typeck::traits::solve::Solution::Ambiguous), which is
    /// deliberately not reported there -- so this would otherwise answer
    /// [`TyKind::Error`] with no diagnostic to explain why, and the mistake would vanish the
    /// moment `Error`'s own absorb-everything rule met the surrounding expression.
    ///
    /// Otherwise a one-line wrapper over [`Typeck::require_extends`] that supplies the two things
    /// every operator has in common: an operator trait takes no generic arguments of its own, and
    /// the label to put on the diagnostic is always "this operator". What it does *not* do is
    /// work out the operator's result type, because that differs per operator -- every
    /// `core::ops` trait returns `Self` except `Eq` and `Comparable`, which return `bool` -- so
    /// the caller keeps that decision.
    fn operator_holds(&mut self, item: LangItem, self_ty: Ty, owner: DefId, span: SrcSpan) -> bool {
        if matches!(self.tcx.kind(self_ty), TyKind::Var(TyVar::Any(_))) {
            report_operand_has_unknown_type(span);
            return false;
        }

        self.require_extends(
            self_ty,
            TraitName::Lang(item),
            Vec::new(),
            owner,
            span,
            "this operator",
        )
    }

    /// Whether `ty` is guaranteed to be a primitive by the time it is fully resolved, so a
    /// built-in operator applies to it without asking the solver anything.
    ///
    /// A numeric inference variable counts alongside an already-concrete primitive:
    /// [`Unifier::decompose`](crate::typeck::unify::Unifier) only ever lets a `{integer}`/
    /// `{float}` variable unify with another numeric variable or the matching family of
    /// primitives, never with an `Adt` -- so there is no operator question here for the solver
    /// to answer, ambiguously or otherwise, whatever the variable eventually resolves to.
    fn is_builtin_operand(&self, ty: Ty) -> bool {
        matches!(
            self.tcx.kind(ty),
            TyKind::Primitive(_) | TyKind::Var(TyVar::Int(_) | TyVar::Float(_))
        )
    }

    /// Strips every `any` layer off `ty`, the way [`Typeck::peel_receiver`] strips `any` (and
    /// `&`/`&mut`) off a method receiver. Unlike that method, this stops at `any` alone: an
    /// operator does not implicitly dereference an ordinary `&`/`&mut` the way `any` is defined
    /// to let a use see through it.
    fn peel_any(&self, mut ty: Ty) -> Ty {
        while let TyKind::Any(base) = *self.tcx.kind(ty) {
            ty = base;
        }
        ty
    }

    /// The type of a literal. Every kind of literal is trivial except an unsuffixed number: `1`
    /// and `1.0` start out as the fallback-carrying [`TyVar::Int`](crate::typeck::ty::TyVar::Int)
    /// and [`TyVar::Float`](crate::typeck::ty::TyVar::Float) inference variables described on
    /// [`TyVar`](crate::typeck::ty::TyVar), narrowed once unification meets a concrete type or
    /// falls back to `i32`/`f64` if it never does. A suffixed number (`1_u8`, `1.0_f32`) instead
    /// lowers straight to the `PrimTy` its suffix names -- there is nothing left to infer.
    ///
    /// Shared with [`Typeck::check_pat`], since a literal in a pattern is the same literal and
    /// takes the same type -- `span` is what lets the one case that reports do so against
    /// whichever of the two it was written in.
    pub(crate) fn check_literal(&mut self, lit: &Literal, span: SrcSpan) -> Ty {
        match lit {
            Literal::Bool(_) => self.tcx.mk_prim(PrimTy::Bool),
            Literal::Char(_) => self.tcx.mk_prim(PrimTy::Char),
            Literal::Int { suffix, .. } => match suffix {
                None => self.tcx.next_int_var(),
                // A whole number written with a float suffix (`5_f64`) is that float, not an
                // error -- only the fractional case below has nowhere for its value to go.
                Some(suffix) => match prim_ty(*suffix) {
                    Some(prim) if is_integer(prim) || is_float(prim) => self.tcx.mk_prim(prim),
                    _ => {
                        report_unknown_literal_suffix(*suffix, span);
                        self.tcx.error()
                    }
                },
            },
            Literal::Float { suffix, .. } => match suffix {
                None => self.tcx.next_float_var(),
                Some(suffix) => match prim_ty(*suffix) {
                    Some(prim) if is_float(prim) => self.tcx.mk_prim(prim),
                    Some(prim) if is_integer(prim) => {
                        report_int_suffix_on_float_literal(*suffix, span);
                        self.tcx.error()
                    }
                    _ => {
                        report_unknown_literal_suffix(*suffix, span);
                        self.tcx.error()
                    }
                },
            },
            // A string literal is a value of some string type, and there is nothing here for it to
            // be: the core library declares no `String`, and `LangItem` names none, so there is no
            // definition to resolve one to. Reported rather than given a stand-in type, which
            // would make every use of it check against something no later pass could lower.
            Literal::Str(_) => {
                report_str_literal_untyped(span);
                self.tcx.error()
            }
        }
    }

    pub fn check_stmt(&mut self, id: HirId) {
        let stmt = self.hir.stmt(id);

        match &stmt.kind {
            StmtKind::Let {
                mutability,
                pat,
                ty,
                init,
                else_block,
            } => {
                let (mutability, pat, ty, init, else_block) =
                    (*mutability, *pat, *ty, *init, *else_block);
                self.check_binding(pat, ty, init, stmt.span);
                self.record_let_mutability(pat, mutability);

                match (self.pat_is_irrefutable(pat), else_block) {
                    (false, None) => {
                        report_refutable_let_without_else(self.hir.pat(pat).span);
                    }
                    (true, Some(block)) => {
                        report_irrefutable_let_with_else(self.hir.block(block).span);
                    }
                    _ => {}
                }

                if let Some(block) = else_block {
                    self.check_block(block);
                }
            }
            StmtKind::With { lends, block } => {
                // Copied out of the node first: every lend is checked with `&mut self`, and the
                // list lives in the arena the borrow above reads.
                let lends: Vec<(HirId, Option<HirId>, HirId, SrcSpan)> = lends
                    .iter()
                    .map(|lend| (lend.pat, lend.ty, lend.init, lend.span))
                    .collect();
                for (pat, ty, init, span) in lends {
                    // A `with` lend is deliberately not entered into `let_mutability`: it has no
                    // `mut` syntax of its own, so it is left unrestricted, same as a `for` or
                    // `match` binding -- see the field's own docs.
                    self.check_binding(pat, ty, init, span);
                }
                self.check_block(*block);
            }
            StmtKind::Return(Some(expr)) => {
                let expr = *expr;
                let ret = self.return_ty(id.owner);

                // The declared return type is what the context demands, so it goes in `expected`
                // and the returned expression in `found` -- otherwise the diagnostic reads
                // backwards ("expected `bool`, found `()`" for a `bool` returned from a function
                // declared to return nothing).
                let expr_ty = self.ty_of_expecting(expr, ret);
                if let Err(err) = self.unify_allowing_any(ret, expr_ty) {
                    report_return_mismatch(self.cx(), err, stmt.span);
                }
            }
            // `return;` with no value produces nothing, which the enclosing definition has to
            // agree to.
            StmtKind::Return(None) => {
                let ret = self.return_ty(id.owner);
                let unit = self.tcx.unit();
                if let Err(err) = self.unifier.unify(&self.tcx, ret, unit) {
                    report_return_mismatch(self.cx(), err, stmt.span);
                }
            }
            StmtKind::Defer(expr) | StmtKind::Expr(expr) => {
                self.ty_of(*expr);
            }
            _ => {}
        }
    }

    /// Checks one binding form -- a `let`, or one lend of a `with` -- and gives the names its
    /// pattern introduces their types.
    ///
    /// The two are the same shape and the same rule: an annotation, if written, is what the
    /// initializer is checked against and what the pattern is bound at; without one the
    /// initializer's own type is both. Checking the initializer *expecting* the annotation is what
    /// makes `let s: Shape = .circle(1.0);` work at all, since `.circle` names no enum of its own.
    fn check_binding(&mut self, pat: HirId, ty: Option<HirId>, init: HirId, span: SrcSpan) {
        let declared = ty.map(|ty| self.lower_ty(ty));
        let init_ty = self.ty_of_expecting_opt(init, declared);

        let bound = match declared {
            Some(declared) => {
                if let Err(err) = self.unifier.unify(&self.tcx, declared, init_ty) {
                    report_binding_type_mismatch(self.cx(), err, span);
                }
                declared
            }
            None => init_ty,
        };
        self.check_pat(pat, bound);
    }

    /// Enters every `Binding` leaf under `pat` into [`Typeck::let_mutability`], recursing through
    /// the shapes a pattern can nest in (a tuple destructure, a variant's payload) so that
    /// `let mut (a, b) = ..` marks both `a` and `b` mutable rather than only the pattern's
    /// outermost node.
    ///
    /// Only [`Typeck::check_stmt`]'s `Let` arm calls this -- a `with` lend is checked through
    /// [`Typeck::check_binding`] the same way a `let` is, but is deliberately never passed
    /// through here (see [`Typeck::let_mutability`]'s own docs).
    fn record_let_mutability(&mut self, pat: HirId, mutability: Mutability) {
        let node = self.hir.pat(pat);
        match &node.kind {
            PatKind::Binding { .. } => {
                self.let_mutability.insert(pat, mutability);
            }
            PatKind::Tuple(elems) => {
                let elems = elems.clone();
                for elem in elems {
                    self.record_let_mutability(elem, mutability);
                }
            }
            PatKind::Variant { payload, .. } => {
                let values: Vec<HirId> = match payload {
                    Payload::None => Vec::new(),
                    Payload::Single(value) => vec![*value],
                    Payload::Record(fields) => fields.iter().map(|field| field.value).collect(),
                };
                for value in values {
                    self.record_let_mutability(value, mutability);
                }
            }
            PatKind::Wildcard | PatKind::Literal(_) | PatKind::Error => {}
        }
    }

    /// Whether `pat_id`'s pattern is guaranteed to match every value of the type
    /// [`Typeck::check_pat`] already checked it against -- the same question
    /// [`Typeck::check_match_exhaustive`] asks of a whole arm list, asked here of one pattern
    /// alone, since a `let` (unlike a `match`) has only the one. A wildcard or a plain binding
    /// always is; a literal never is, even `true` alone doesn't cover `false`; a variant pattern
    /// is exactly when the enum it names declares only that one variant; a tuple is exactly when
    /// every element is.
    ///
    /// Called only after [`Typeck::check_binding`] has already run [`Typeck::check_pat`] over
    /// `pat_id` (and, for a `Variant`, recorded the enum type it was matched against), so this
    /// never has to re-derive a type on its own -- it only reads back what that pass already
    /// settled.
    fn pat_is_irrefutable(&mut self, pat_id: HirId) -> bool {
        let pat = self.hir.pat(pat_id);
        match &pat.kind {
            PatKind::Wildcard | PatKind::Binding { .. } => true,
            PatKind::Literal(_) => false,
            PatKind::Variant { .. } => {
                let ty = self
                    .types
                    .ty(pat_id)
                    .map(|ty| self.resolve_deep(ty))
                    .unwrap_or_else(|| self.tcx.error());
                match self.tcx.kind(ty) {
                    // Already reported (an unresolved variant, or a scrutinee whose type never
                    // settled) -- treat it charitably rather than cascading a second diagnostic.
                    TyKind::Error | TyKind::Var(_) => true,
                    TyKind::Adt { def, .. } => match self.hir.def(*def) {
                        OwnerNode::Enum(enum_) => enum_.variants.len() == 1,
                        _ => false,
                    },
                    _ => false,
                }
            }
            PatKind::Tuple(elems) => elems.iter().all(|&elem| self.pat_is_irrefutable(elem)),
            // Already reported by the parser; don't cascade a second diagnostic onto it.
            PatKind::Error => true,
        }
    }

    /// What a `return` inside `owner` has to produce.
    ///
    /// A definition with no declared return type produces nothing, which is exactly what `Unit`
    /// means. `Never` would be wrong here: it unifies with everything, so `return <anything>` from
    /// a function declared to return nothing would be accepted silently.
    ///
    /// `owner` is a function or a closure. A closure records a signature for itself before its
    /// body is checked ([`Typeck::check_closure`]) precisely so that this reads the same way for
    /// both.
    fn return_ty(&mut self, owner: DefId) -> Ty {
        let sig = self
            .recorded_ty_of_def(owner)
            .expect("a signature is recorded before the body it belongs to is checked");
        let TyKind::Fun { ret, .. } = self.tcx.kind(sig) else {
            unreachable!("a function's or closure's own signature always lowers to TyKind::Fun");
        };
        ret.unwrap_or_else(|| self.tcx.unit())
    }

    /// Display context for rendering this pass's types as the user wrote them. Build one
    /// where a diagnostic is emitted rather than holding on to it -- it borrows `self`.
    fn cx(&self) -> DisplayCx<'_> {
        DisplayCx::new(self.hir, &self.tcx)
    }

    /// Whether something declared `visibility` in `owner_module` is reachable from `from` -- the
    /// definition (a function, method, or closure) an access to it sits inside.
    ///
    /// Mirrors `SymbolTable::is_visible` (`crate::nameres::symbol_table`), the same rule read
    /// here off `Hir`'s own module tree instead of the AST's: `public` reaches everywhere,
    /// `private` only the declaring module and that module's descendants, so `owner_module` must
    /// appear in `from`'s chain of ancestor modules (or be `from`'s own enclosing module).
    fn is_visible_from(&self, owner_module: DefId, from: DefId, visibility: Visibility) -> bool {
        match visibility {
            Visibility::Public => true,
            Visibility::Private => {
                let mut current = Some(self.hir.module_of(from));
                while let Some(module) = current {
                    if module == owner_module {
                        return true;
                    }
                    current = self.hir.parent(module);
                }
                false
            }
        }
    }

    /// Checks every statement in the block, and its trailing expression if it has one, and returns
    /// the block's own type.
    pub fn check_block(&mut self, id: HirId) -> Ty {
        self.check_block_expecting(id, None)
    }

    /// [`Typeck::check_block`], passing an expectation on to the trailing expression.
    ///
    /// A block's type is its trailing expression's, so an expectation on the block is an
    /// expectation on that expression and on nothing else in it. This is what carries a `match`
    /// arm's expected type down to the `.variant` the arm ends with.
    fn check_block_expecting(&mut self, id: HirId, expected: Option<Ty>) -> Ty {
        let block = self.hir.block(id);
        let tail = block.expr;

        let mut diverges = false;
        for &stmt in &block.stmts {
            self.check_stmt(stmt);
            diverges |= match self.hir.stmt(stmt).kind {
                StmtKind::Return(_) | StmtKind::Break | StmtKind::Continue => true,
                // Written as a statement rather than the block's tail (a trailing `;`, or more
                // code after it), an `if`/`match` that diverges on every one of its own arms is
                // otherwise invisible here: it is neither literally a `return`/`break`/`continue`
                // itself, nor read back through `tail_ty` below since it isn't the tail. Its own
                // checked type already folded that down to `Never` (`check_if`/`check_match`
                // unify their arms together, and `Never` is what an arm that itself diverges
                // contributes), so asking for it is enough to catch it here too.
                StmtKind::Expr(expr) => self.ty_of(expr) == self.tcx.never(),
                _ => false,
            };
        }

        // A block's trailing expression is not a statement, so the loop above never reaches it.
        // Checking it here is what types a function body written as a bare expression -- and it
        // happens even for a block that has already diverged, since those nodes still need types.
        let tail_ty = match tail {
            Some(tail) => self.ty_of_expecting_opt(tail, expected),
            // A block that ends in a statement produces nothing.
            None => self.tcx.unit(),
        };

        // A block that leaves through a `return`, `break`, or `continue` -- directly, or by way
        // of a statement whose own checked type already came out `Never` -- never reaches its own
        // end, so it produces no value of any type -- which is what `Never` says, and why it
        // unifies with whatever the context wanted. Without this, `|x| { return x; }` would be
        // read as producing `()`.
        //
        // Only a statement *of* this block is looked at. Divergence hidden inside an expression
        // this language has no syntax to express -- a call to a function that never returns, say
        // -- is not tracked, so this errs towards treating a block as completing normally.
        if diverges {
            self.tcx.never()
        } else {
            tail_ty
        }
    }

    /// Checks `def_id`'s body against the signature stage one collected for it, and bakes the
    /// resulting types into the table.
    ///
    /// The body block is checked *expecting* the declared return type and its resulting type is
    /// then unified against that same type, exactly as [`Typeck::check_closure`] already does for
    /// a closure's body. That single unification is enough to cover a trailing expression of the
    /// wrong type, an empty body, and a body that only returns on some paths: `check_block`
    /// already folds an always-diverging body down to `Never` (every statement-position
    /// expression whose own type came out `Never` counts, not just a literal `return`/`break`/
    /// `continue`, so `if c { return 1; } else { return 2; }` written as the body's tail
    /// expression is `Never` itself), and `Never` unifies with anything -- so only a body that
    /// can actually fall through with the wrong type (or fall through at all) is rejected here. A
    /// `return` inside the body is checked on its own terms regardless (see
    /// [`Typeck::check_stmt`]'s `Return` arm).
    pub fn check_function(&mut self, def_id: DefId) {
        let function = self.hir.function(def_id);

        if let Some(block) = function.block {
            // Bounds raised while checking the body are proved at the end of it and not before:
            // an argument written as `_` early on is only known once the rest of the body has
            // pinned it down, and asking sooner would answer "ambiguous" to a question that has a
            // perfectly good answer a few statements later.
            self.in_body = true;
            let ret = self.return_ty(def_id);
            let body = self.check_block_expecting(block, Some(ret));
            if let Err(err) = self.unify_allowing_any(ret, body) {
                report_body_return_mismatch(self.cx(), err, function.span);
            }
            self.select_body_obligations();
            self.in_body = false;
        }
        self.writeback(def_id);
    }
}

/// Drives stage one over the HIR: the traversal behind [`Typeck::collect_module`].
///
/// A wrapper rather than an `impl Visitor for Typeck` because the two stages are two different
/// traversals of the same tree and a type may implement a trait once. The wrapper holds
/// `&mut Typeck`, so each hook delegates to the `collect_*` method of the same name.
///
/// Three hooks depart from calling the matching `walk_*`, each for a stated reason:
///
/// - `visit_function` does not walk. [`Typeck::collect_function`] reads the same children the
///   walk would, but has to build one `Vec<Ty>` of parameter types in declaration order with the
///   `self` parameter first, which a per-child hook cannot accumulate.
/// - `visit_struct`, `visit_enum` and `visit_trait` interleave: `Self` for a definition is
///   `TyKind::Adt` applied to that definition's own generics, so [`Typeck::self_ty`] must run
///   after the generics are recorded and before any field or variant type that could mention
///   `Self` is lowered. The `collect_*` methods do all three in that order.
/// - `visit_nested_owner` descends, so that a trait's and an `extend` block's methods have their
///   signatures collected. Stage one visits no body, so descending here cannot reach one.
struct Collect<'a, 'hir>(&'a mut Typeck<'hir>);

impl<'hir> Visitor<'hir> for Collect<'_, 'hir> {
    fn hir(&self) -> &'hir Hir {
        self.0.hir
    }

    fn visit_nested_owner(&mut self, def_id: DefId) {
        visit::walk_item(self, def_id);
    }

    fn visit_function(&mut self, def_id: DefId) {
        self.0.collect_function(def_id);
    }

    fn visit_struct(&mut self, def_id: DefId) {
        self.0.collect_struct(def_id);
    }

    fn visit_enum(&mut self, def_id: DefId) {
        self.0.collect_enum(def_id);
    }

    fn visit_trait(&mut self, def_id: DefId) {
        self.0.collect_trait(def_id);
        // Reaches the trait's methods through `visit_nested_owner`; `collect_trait` itself
        // records only the trait's generics and its `Self`.
        visit::walk_trait(self, def_id);
    }

    fn visit_extend(&mut self, def_id: DefId) {
        self.0.collect_extend(def_id);
        visit::walk_extend(self, def_id);
    }

    /// A closure is an owner, so `walk_item` has an arm for it, but no traversal here can reach
    /// one: a closure's `DefId` is stored in an `ExprKind::Closure` inside a body, and stage one
    /// enters no body. Reaching this means a closure was reached as a module item or as a method,
    /// which lowering does not produce.
    fn visit_closure(&mut self, def_id: DefId) {
        unreachable!("stage one reached a closure ({def_id:?}), which owns no signature to collect")
    }
}

/// Drives stage two over the HIR: the traversal behind [`Typeck::check_module`].
///
/// The counterpart to [`Collect`], and the reason both are wrappers. Where stage one records a
/// type for every declaration, this one visits only the definitions that own a body:
///
/// - `visit_struct` and `visit_enum` are overridden to do nothing, replacing the walk's descent
///   into fields and variants. Stage one already recorded those types, and neither declaration
///   contains an expression to check.
/// - `visit_function` calls [`Typeck::check_function`] and does not walk. Checking a body is a
///   traversal with a result type at every step -- [`Typeck::check_expr`] returns the [`Ty`] its
///   caller unifies against -- which the walk's `()`-returning hooks cannot carry.
/// - `visit_nested_owner` descends, reaching the methods of a trait or `extend` block. A closure
///   is also a nested owner, but stage two never reaches one from here: `walk_item` is called
///   only from the hooks below, and `check_function` does not walk into its own body.
struct Check<'a, 'hir>(&'a mut Typeck<'hir>);

impl<'hir> Visitor<'hir> for Check<'_, 'hir> {
    fn hir(&self) -> &'hir Hir {
        self.0.hir
    }

    fn visit_nested_owner(&mut self, def_id: DefId) {
        visit::walk_item(self, def_id);
    }

    fn visit_function(&mut self, def_id: DefId) {
        self.0.check_function(def_id);
    }

    fn visit_struct(&mut self, _def_id: DefId) {}

    fn visit_enum(&mut self, _def_id: DefId) {}

    /// Unreachable for the same reason as [`Collect::visit_closure`]: `check_function` checks a
    /// body without walking it, so no closure's `DefId` is ever handed to this traversal. The
    /// `ExprKind::Closure` arm of [`Typeck::check_expr`] is what will check one, from inside the
    /// body that declares it.
    fn visit_closure(&mut self, def_id: DefId) {
        unreachable!("stage two reached a closure ({def_id:?}) outside the body declaring it")
    }
}

pub struct TypeckOutput {
    pub tcx: TyCtx,
    pub types: TypeResolutions,
}

/// Checks the whole program, as described in the [module docs](self).
pub fn check(hir: &Hir) -> TypeckOutput {
    let mut checker = Typeck::new(hir);
    checker.collect_module(hir.root_id());
    checker.build_impl_index();
    checker.check_coherence();
    checker.check_trait_members();
    checker.check_declared_bounds();
    checker.check_impl_headers();
    checker.select_program_obligations();
    checker.check_module(hir.root_id());
    TypeckOutput {
        tcx: checker.tcx,
        types: checker.types,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::diag::{DiagCtx, Severity};
    use crate::nameres::PrimTy;
    use crate::testing::{
        find_return, first_extend_method, first_function, first_struct, first_trait, resolve_src,
        typeck_accepts as accepts, typeck_rejects as rejects,
    };
    use crate::typeck::unify::UnifyError;

    /// Builds a `Typeck` with every signature collected, ready for `check_stmt` to be called
    /// directly on one of `def_id`'s statements.
    fn checker_with_signatures_collected<'hir>(hir: &'hir Hir) -> Typeck<'hir> {
        let mut checker = Typeck::new(hir);
        checker.collect_module(hir.root_id());
        checker
    }

    #[test]
    fn return_stmt_accepts_a_value_matching_the_return_type() {
        // `0`'s int-inference var unifies fine with the declared `i32` return type.
        let hir = resolve_src("fun f() -> i32 { return 0; }");
        let def_id = first_function(&hir);
        let (stmt_id, _expr_id) = find_return(&hir, def_id);

        let mut checker = checker_with_signatures_collected(&hir);

        DiagCtx::clear();
        checker.check_stmt(stmt_id);
        let diagnostics = DiagCtx::diagnostics();
        assert!(diagnostics.is_empty(), "{diagnostics:?}");
    }

    #[test]
    fn return_stmt_rejects_a_value_not_matching_the_return_type() {
        let hir = resolve_src("fun f() -> i32 { return true; }");
        let def_id = first_function(&hir);
        let (stmt_id, _expr_id) = find_return(&hir, def_id);

        let mut checker = checker_with_signatures_collected(&hir);

        DiagCtx::clear();
        checker.check_stmt(stmt_id);
        let diagnostics = DiagCtx::diagnostics();
        assert_eq!(diagnostics.len(), 1, "{diagnostics:?}");
        assert_eq!(diagnostics[0].severity, Severity::Error);
    }

    #[test]
    fn return_stmt_in_a_function_with_no_declared_return_type_rejects_a_value() {
        // No `-> T` means the function returns `Unit`, so returning a `bool` from it is an
        // error. This used to lower to `Never` instead, which unifies with everything and so
        // accepted any returned value at all.
        let hir = resolve_src("fun f() { return true; }");
        let def_id = first_function(&hir);
        let (stmt_id, _expr_id) = find_return(&hir, def_id);

        let mut checker = checker_with_signatures_collected(&hir);

        DiagCtx::clear();
        checker.check_stmt(stmt_id);
        let diagnostics = DiagCtx::diagnostics();
        assert_eq!(diagnostics.len(), 1, "{diagnostics:?}");
        assert_eq!(diagnostics[0].severity, Severity::Error);
    }

    /// A bare `return;`, with no expression, produces `Unit` -- exactly what a function with no
    /// declared return type itself produces -- so the two agree.
    #[test]
    fn a_bare_return_with_no_declared_return_type_checks() {
        accepts("fun f() { return; }");
    }

    /// A bare `return;` still has to agree with a *declared* return type, the same as `return`
    /// with a value does.
    #[test]
    fn a_bare_return_in_a_function_declaring_a_return_type_is_rejected() {
        rejects("fun f() -> i32 { return; }", "mismatched types");
    }

    /// Defect 2's end-to-end shape: `Never`/`Error`/`Unit` are interned once per pass, so a
    /// merge involving one used to leak across every function checked afterwards. Both of these
    /// functions are individually valid, and checking them together must stay that way.
    #[test]
    fn two_functions_with_no_return_type_do_not_interfere() {
        let hir = resolve_src(
            "fun f() -> bool { return true; }
             fun g() -> i32 { return 1; }",
        );
        let mut checker = checker_with_signatures_collected(&hir);

        DiagCtx::clear();
        checker.check_module(hir.root_id());
        let diagnostics = DiagCtx::diagnostics();
        assert!(diagnostics.is_empty(), "{diagnostics:?}");
    }

    // -----------------------------------------------------------------
    // check_expr
    // -----------------------------------------------------------------

    /// A checker with signatures collected and the impl index built, ready to answer trait
    /// questions -- what [`Typeck::operator_holds`] needs, since it's reached through
    /// [`Typeck::extends`] rather than the plain unifier.
    fn checker_with_impls_built<'hir>(hir: &'hir Hir) -> Typeck<'hir> {
        let mut checker = checker_with_signatures_collected(hir);
        checker.build_impl_index();
        checker
    }

    /// The `DefId` of the first item anywhere in `hir`'s module tree that `pred` accepts,
    /// recursing into submodules.
    ///
    /// [`first_function`]/[`first_struct`] only look at the root module's own items, which is
    /// enough for a fixture with no `module` header of its own. A lang item's own trait has to
    /// live at its real path (`core::ops::Add`, for [`LangItem::path`]) to resolve at all, so
    /// these tests nest their fixture's whole program under `module core::ops;` and need to find
    /// their way back into it.
    fn find_owner(hir: &Hir, from: DefId, pred: &impl Fn(&OwnerNode) -> bool) -> DefId {
        find_owner_opt(hir, from, pred)
            .unwrap_or_else(|| panic!("no item matching the predicate anywhere under {from:?}"))
    }

    fn find_owner_opt(hir: &Hir, from: DefId, pred: &impl Fn(&OwnerNode) -> bool) -> Option<DefId> {
        let module = hir.module(from);

        for &item in &module.items {
            if pred(hir.def(item)) {
                return Some(item);
            }
            if matches!(hir.def(item), OwnerNode::Module(_)) {
                if let Some(found) = find_owner_opt(hir, item, pred) {
                    return Some(found);
                }
            }
        }
        None
    }

    /// `a + b` on a struct with an `extend Foo with Add` block resolves through the solver:
    /// [`Typeck::operator_holds`] asks [`Typeck::extends`] whether `Foo` implements the trait
    /// `LangItem::Add` names, gets back `Solution::Holds`, and the arm returns `Foo` itself as
    /// the result -- every operator trait in `core::ops` returns `Self`, so there's no associated
    /// type to project.
    #[test]
    fn binary_add_on_a_struct_with_an_add_impl_resolves_through_the_solver() {
        let hir = resolve_src(
            "module core::ops;

             public trait Add {
                 fun add(&self, other: &Self) -> Self;
             }

             struct Foo {
                 x: i32,
             }

             extend Foo with Add {
                 fun add(&self, other: &Self) -> Self {
                     return .{ x: self.x };
                 }
             }

             fun f(a: Foo, b: Foo) -> Foo {
                 return a + b;
             }",
        );
        let f = find_owner(&hir, hir.root_id(), &|def| {
            matches!(def, OwnerNode::Function(_))
        });
        let (stmt_id, _expr_id) = find_return(&hir, f);
        let mut checker = checker_with_impls_built(&hir);

        DiagCtx::clear();
        checker.check_stmt(stmt_id);
        let diagnostics = DiagCtx::diagnostics();
        assert!(diagnostics.is_empty(), "{diagnostics:?}");
    }

    /// The same shape, minus the `extend` block: `Solution::DoesNotHold` reports that `Foo`
    /// doesn't implement `Add`, the same way an unsatisfied bound would.
    #[test]
    fn binary_add_on_a_struct_with_no_add_impl_is_rejected() {
        let hir = resolve_src(
            "module core::ops;

             public trait Add {
                 fun add(&self, other: &Self) -> Self;
             }

             struct Foo {
                 x: i32,
             }

             fun f(a: Foo, b: Foo) -> Foo {
                 return a + b;
             }",
        );
        let f = find_owner(&hir, hir.root_id(), &|def| {
            matches!(def, OwnerNode::Function(_))
        });
        let (stmt_id, _expr_id) = find_return(&hir, f);
        let mut checker = checker_with_impls_built(&hir);

        DiagCtx::clear();
        checker.check_stmt(stmt_id);
        let diagnostics = DiagCtx::diagnostics();
        assert_eq!(diagnostics.len(), 1, "{diagnostics:?}");
        assert!(diagnostics[0].message.contains("Add"), "{diagnostics:?}");
    }

    /// `1 + 2` never reaches the solver at all: an operand still typed as a primitive short-
    /// circuits `operator_holds` entirely, so ordinary arithmetic keeps working in a
    /// fixture with no core library -- and so no lang items -- in sight.
    #[test]
    fn binary_add_on_primitives_bypasses_the_solver() {
        let hir = resolve_src("fun f() -> i32 { return 1 + 2; }");
        let def_id = first_function(&hir);
        let (stmt_id, _expr_id) = find_return(&hir, def_id);
        let mut checker = checker_with_impls_built(&hir);

        DiagCtx::clear();
        checker.check_stmt(stmt_id);
        let diagnostics = DiagCtx::diagnostics();
        assert!(diagnostics.is_empty(), "{diagnostics:?}");
    }

    /// Defect: `is_builtin_operand`'s predecessor recognized only an already-concrete
    /// primitive, so two unsuffixed literals -- neither one resolved yet -- fell through to
    /// the solver, which answered `Ambiguous`, which `require_extends` (rightly) does not
    /// report. The whole expression silently checked to `Error` instead of `i32`, with no
    /// diagnostic anywhere: `Error` absorbs into the `return` type's unification and the
    /// mistake vanishes. `binary_add_on_primitives_bypasses_the_solver` above only ever
    /// asserted "no diagnostics", which this defect also satisfied -- so this test checks the
    /// actual resolved type instead.
    #[test]
    fn binary_add_between_two_unresolved_int_literals_resolves_to_the_return_type() {
        let hir = resolve_src("fun f() -> i32 { return 1 + 2; }");
        let def_id = first_function(&hir);
        let (_stmt_id, expr_id) = find_return(&hir, def_id);
        let mut checker = checker_with_signatures_collected(&hir);

        checker.check_function(def_id);

        let ty = checker
            .types
            .ty(expr_id)
            .expect("checking the body records the binary expression's type");
        assert_eq!(
            *checker.tcx.kind(ty),
            TyKind::Primitive(PrimTy::I32),
            "the operator bypassed the solver and the return unified it with i32"
        );
    }

    /// The genuinely ambiguous case `is_builtin_operand` does not, and should not, swallow: two
    /// operands that stay wholly unconstrained variables all the way to the operator, with
    /// nothing anywhere pinning either one down. Reported immediately rather than silently
    /// becoming `Error`, the same way an unknown method receiver is -- there is no later pass
    /// this could be deferred to that would ever know more.
    #[test]
    fn an_operator_on_two_still_unresolved_operands_needs_an_annotation() {
        use crate::testing::typeck_rejects;

        typeck_rejects(
            "fun make<T>() -> T { return make(); }
             fun f() {
                 let a = make();
                 let b = make();
                 let c = a - b;
             }",
            "type annotations needed",
        );
    }

    /// `ty_of` records on first use and reads through the unifier afterwards, so a type read
    /// back after it has been unified with something concrete comes back as the concrete type
    /// rather than the variable that was originally recorded.
    #[test]
    fn a_type_read_back_after_unification_is_the_unified_type() {
        let hir = resolve_src("fun f() -> i32 { return 1; }");
        let def_id = first_function(&hir);
        let (_stmt_id, expr_id) = find_return(&hir, def_id);
        let mut checker = checker_with_signatures_collected(&hir);

        // An unsuffixed literal starts out as an integer inference variable.
        let recorded = checker.ty_of(expr_id);
        assert!(matches!(checker.tcx.kind(recorded), TyKind::Var(_)));

        let i32_ty = checker.tcx.mk_prim(PrimTy::I32);
        checker
            .unifier
            .unify(&checker.tcx, recorded, i32_ty)
            .expect("an int var unifies with i32");

        // The table still holds the variable, but nothing reads it directly.
        assert_eq!(checker.types.ty(expr_id), Some(recorded));
        assert_eq!(checker.ty_of(expr_id), i32_ty);
    }

    /// Reading through the unifier only helps while the unifier is still around. `writeback`
    /// bakes the resolution into the table so that everything downstream of the pass -- which
    /// gets the table and the `TyCtx`, but no union-find -- reads settled types.
    #[test]
    fn writeback_leaves_no_unresolved_variables_behind() {
        let hir = resolve_src("fun f() -> i32 { return 1; }");
        let def_id = first_function(&hir);
        let (_stmt_id, expr_id) = find_return(&hir, def_id);
        let mut checker = checker_with_signatures_collected(&hir);

        checker.check_function(def_id);

        let recorded = checker
            .types
            .ty(expr_id)
            .expect("checking the body records the returned expression's type");
        assert_eq!(
            *checker.tcx.kind(recorded),
            TyKind::Primitive(PrimTy::I32),
            "the return unified the literal with i32, and writeback stored that"
        );
    }

    /// A local whose initializer is never unified against anything else -- no annotation, no
    /// later use pinning its type down -- still leaves `writeback` with a bare `TyVar` to
    /// resolve. `default_unconstrained` is what turns that into `i32`, the same fallback an
    /// unsuffixed integer literal gets in Rust.
    #[test]
    fn an_unconstrained_int_literal_defaults_to_i32() {
        let hir = resolve_src("fun f() { let x = 5; }");
        let def_id = first_function(&hir);
        let mut checker = checker_with_signatures_collected(&hir);
        checker.check_function(def_id);

        let function = hir.function(def_id);
        let block = hir.block(function.block.unwrap());
        let stmt = hir.stmt(block.stmts[0]);
        let StmtKind::Let { init, .. } = stmt.kind else {
            panic!("expected a let statement");
        };

        let ty = checker
            .types
            .ty(init)
            .expect("writeback records the initializer's type");
        assert_eq!(*checker.tcx.kind(ty), TyKind::Primitive(PrimTy::I32));
    }

    /// The float counterpart of the test above.
    #[test]
    fn an_unconstrained_float_literal_defaults_to_f64() {
        let hir = resolve_src("fun f() { let x = 5.0; }");
        let def_id = first_function(&hir);
        let mut checker = checker_with_signatures_collected(&hir);
        checker.check_function(def_id);

        let function = hir.function(def_id);
        let block = hir.block(function.block.unwrap());
        let stmt = hir.stmt(block.stmts[0]);
        let StmtKind::Let { init, .. } = stmt.kind else {
            panic!("expected a let statement");
        };

        let ty = checker
            .types
            .ty(init)
            .expect("writeback records the initializer's type");
        assert_eq!(*checker.tcx.kind(ty), TyKind::Primitive(PrimTy::F64));
    }

    #[test]
    fn bool_literal_checks_to_the_bool_primitive() {
        let hir = resolve_src("fun f() -> bool { return true; }");
        let def_id = first_function(&hir);
        let (_stmt_id, expr_id) = find_return(&hir, def_id);
        let mut checker = checker_with_signatures_collected(&hir);

        let ty = checker.ty_of(expr_id);
        assert_eq!(*checker.tcx.kind(ty), TyKind::Primitive(PrimTy::Bool));
        assert_eq!(checker.types.ty(expr_id), Some(ty));
    }

    #[test]
    fn char_literal_checks_to_the_char_primitive() {
        let hir = resolve_src("fun f() -> char { return 'a'; }");
        let def_id = first_function(&hir);
        let (_stmt_id, expr_id) = find_return(&hir, def_id);
        let mut checker = checker_with_signatures_collected(&hir);

        let ty = checker.ty_of(expr_id);
        assert_eq!(*checker.tcx.kind(ty), TyKind::Primitive(PrimTy::Char));
    }

    #[test]
    fn unsuffixed_int_literal_checks_to_an_int_inference_var() {
        let hir = resolve_src("fun f() -> i32 { return 0; }");
        let def_id = first_function(&hir);
        let (_stmt_id, expr_id) = find_return(&hir, def_id);
        let mut checker = checker_with_signatures_collected(&hir);

        let ty = checker.ty_of(expr_id);
        assert!(matches!(
            checker.tcx.kind(ty),
            TyKind::Var(crate::typeck::ty::TyVar::Int(_))
        ));
    }

    #[test]
    fn unsuffixed_float_literal_checks_to_a_float_inference_var() {
        let hir = resolve_src("fun f() -> f64 { return 0.0; }");
        let def_id = first_function(&hir);
        let (_stmt_id, expr_id) = find_return(&hir, def_id);
        let mut checker = checker_with_signatures_collected(&hir);

        let ty = checker.ty_of(expr_id);
        assert!(matches!(
            checker.tcx.kind(ty),
            TyKind::Var(crate::typeck::ty::TyVar::Float(_))
        ));
    }

    #[test]
    fn suffixed_int_literal_checks_directly_to_its_primitive() {
        let hir = resolve_src("fun f() -> u8 { return 5_u8; }");
        let def_id = first_function(&hir);
        let (_stmt_id, expr_id) = find_return(&hir, def_id);
        let mut checker = checker_with_signatures_collected(&hir);

        let ty = checker.ty_of(expr_id);
        assert_eq!(*checker.tcx.kind(ty), TyKind::Primitive(PrimTy::U8));
    }

    #[test]
    fn suffixed_float_literal_checks_directly_to_its_primitive() {
        let hir = resolve_src("fun f() -> f32 { return 3.14_f32; }");
        let def_id = first_function(&hir);
        let (_stmt_id, expr_id) = find_return(&hir, def_id);
        let mut checker = checker_with_signatures_collected(&hir);

        let ty = checker.ty_of(expr_id);
        assert_eq!(*checker.tcx.kind(ty), TyKind::Primitive(PrimTy::F32));
    }

    /// A whole number written with a float suffix is that float, not an error: `5_f64` is `5.0`.
    #[test]
    fn whole_number_with_a_float_suffix_checks_to_the_float_primitive() {
        let hir = resolve_src("fun f() -> f64 { return 5_f64; }");
        let def_id = first_function(&hir);
        let (_stmt_id, expr_id) = find_return(&hir, def_id);
        let mut checker = checker_with_signatures_collected(&hir);

        let ty = checker.ty_of(expr_id);
        assert_eq!(*checker.tcx.kind(ty), TyKind::Primitive(PrimTy::F64));
    }

    #[test]
    fn digit_separators_do_not_interfere_with_a_suffix() {
        let hir = resolve_src("fun f() -> i64 { return 1_000_000_i64; }");
        let def_id = first_function(&hir);
        let (_stmt_id, expr_id) = find_return(&hir, def_id);
        let mut checker = checker_with_signatures_collected(&hir);

        let ty = checker.ty_of(expr_id);
        assert_eq!(*checker.tcx.kind(ty), TyKind::Primitive(PrimTy::I64));
    }

    #[test]
    fn suffix_disagreeing_with_the_expected_type_is_a_mismatch() {
        use crate::testing::typeck_rejects;

        typeck_rejects("fun f() -> i32 { return 5_u8; }", "found");
    }

    #[test]
    fn fractional_literal_with_an_integer_suffix_is_rejected() {
        use crate::testing::typeck_rejects;

        typeck_rejects("fun f() -> i32 { return 3.14_i32; }", "fractional part");
    }

    #[test]
    fn unknown_literal_suffix_is_rejected() {
        use crate::testing::typeck_rejects;

        typeck_rejects(
            "fun f() -> i32 { return 1_bogus; }",
            "invalid literal suffix",
        );
    }

    #[test]
    fn tuple_expr_checks_to_a_tuple_of_its_elements_types() {
        let hir = resolve_src("fun f() -> (bool, char) { return (true, 'a'); }");
        let def_id = first_function(&hir);
        let (_stmt_id, expr_id) = find_return(&hir, def_id);
        let mut checker = checker_with_signatures_collected(&hir);

        let ty = checker.ty_of(expr_id);
        let TyKind::Tuple(elems) = checker.tcx.kind(ty) else {
            panic!("a tuple expression checks to TyKind::Tuple, got {ty:?}");
        };
        let elem_kinds: Vec<TyKind> = elems
            .iter()
            .map(|&elem| checker.tcx.kind(elem).clone())
            .collect();
        assert_eq!(
            elem_kinds,
            vec![
                TyKind::Primitive(PrimTy::Bool),
                TyKind::Primitive(PrimTy::Char),
            ]
        );
    }

    // -----------------------------------------------------------------
    // check_expr: Path
    // -----------------------------------------------------------------

    #[test]
    fn path_to_a_parameter_checks_to_the_parameters_type() {
        let hir = resolve_src("fun f(x: i32) -> i32 { return x; }");
        let def_id = first_function(&hir);
        let (_stmt_id, expr_id) = find_return(&hir, def_id);
        let mut checker = checker_with_signatures_collected(&hir);

        let ty = checker.ty_of(expr_id);
        assert_eq!(*checker.tcx.kind(ty), TyKind::Primitive(PrimTy::I32));
    }

    #[test]
    fn path_to_a_function_checks_to_its_signature() {
        let hir = resolve_src(
            "fun g() -> bool { return true; }
             fun f() -> bool { return g; }",
        );
        // `first_function` finds `g`; `f` is the second top-level function.
        let module = hir.root();
        let g_def = first_function(&hir);
        let f_def = module
            .items
            .iter()
            .copied()
            .find(|&item| matches!(hir.def(item), OwnerNode::Function(_)) && item != g_def)
            .expect("fixture declares a second top-level function");
        let (_stmt_id, expr_id) = find_return(&hir, f_def);
        let mut checker = checker_with_signatures_collected(&hir);

        let ty = checker.ty_of(expr_id);
        let TyKind::Fun { params, ret } = checker.tcx.kind(ty) else {
            panic!("a function path checks to TyKind::Fun, got {ty:?}");
        };
        assert!(params.is_empty());
        assert_eq!(
            ret.map(|ret| checker.tcx.kind(ret).clone()),
            Some(TyKind::Primitive(PrimTy::Bool))
        );
    }

    #[test]
    fn path_to_self_checks_to_the_self_parameters_type() {
        let hir = resolve_src(
            "struct S {}
             extend S { fun m(&self) -> i32 { return self; } }",
        );
        let method_def = first_extend_method(&hir);
        let (_stmt_id, expr_id) = find_return(&hir, method_def);
        let mut checker = checker_with_signatures_collected(&hir);

        let ty = checker.ty_of(expr_id);
        assert_eq!(checker.cx().show(ty).to_string(), "&S");
    }

    // -----------------------------------------------------------------
    // Diagnostic rendering
    // -----------------------------------------------------------------

    #[test]
    fn primitive_displays_as_its_keyword() {
        let hir = resolve_src("fun f() {}");
        let mut checker = checker_with_signatures_collected(&hir);
        let ty = checker.tcx.mk_prim(PrimTy::I32);
        assert_eq!(checker.cx().show(ty).to_string(), "i32");
    }

    #[test]
    fn any_ty_var_displays_as_underscore() {
        let hir = resolve_src("fun f() {}");
        let mut checker = checker_with_signatures_collected(&hir);
        let ty = checker.tcx.next_ty_var();
        assert_eq!(checker.cx().show(ty).to_string(), "_");
    }

    #[test]
    fn int_var_displays_as_integer_placeholder() {
        let hir = resolve_src("fun f() {}");
        let mut checker = checker_with_signatures_collected(&hir);
        let ty = checker.tcx.next_int_var();
        assert_eq!(checker.cx().show(ty).to_string(), "{integer}");
    }

    #[test]
    fn float_var_displays_as_float_placeholder() {
        let hir = resolve_src("fun f() {}");
        let mut checker = checker_with_signatures_collected(&hir);
        let ty = checker.tcx.next_float_var();
        assert_eq!(checker.cx().show(ty).to_string(), "{float}");
    }

    #[test]
    fn never_displays_as_bang() {
        let hir = resolve_src("fun f() {}");
        let checker = checker_with_signatures_collected(&hir);
        assert_eq!(checker.cx().show(checker.tcx.never()).to_string(), "!");
    }

    #[test]
    fn unit_displays_as_empty_parens() {
        let hir = resolve_src("fun f() {}");
        let checker = checker_with_signatures_collected(&hir);
        assert_eq!(checker.cx().show(checker.tcx.unit()).to_string(), "()");
    }

    #[test]
    fn unit_is_a_singleton_distinct_from_the_empty_tuple() {
        let hir = resolve_src("fun f() {}");
        let mut checker = checker_with_signatures_collected(&hir);
        let unit = checker.tcx.unit();
        let empty_tuple = checker.tcx.mk_tuple(vec![]);
        assert_ne!(unit, empty_tuple);
        assert_eq!(
            checker.tcx.unit(),
            unit,
            "unit() always returns the same handle"
        );
    }

    #[test]
    fn error_displays_as_placeholder() {
        let hir = resolve_src("fun f() {}");
        let checker = checker_with_signatures_collected(&hir);
        assert_eq!(
            checker.cx().show(checker.tcx.error()).to_string(),
            "{error}"
        );
    }

    #[test]
    fn immutable_ref_displays_with_ampersand() {
        let hir = resolve_src("fun f() {}");
        let mut checker = checker_with_signatures_collected(&hir);
        let bool_ty = checker.tcx.mk_prim(PrimTy::Bool);
        let ty = checker.tcx.mk_ref(bool_ty, Mutability::Immutable);
        assert_eq!(checker.cx().show(ty).to_string(), "&bool");
    }

    #[test]
    fn mutable_ref_displays_with_ampersand_mut() {
        let hir = resolve_src("fun f() {}");
        let mut checker = checker_with_signatures_collected(&hir);
        let bool_ty = checker.tcx.mk_prim(PrimTy::Bool);
        let ty = checker.tcx.mk_ref(bool_ty, Mutability::Mutable);
        assert_eq!(checker.cx().show(ty).to_string(), "&mut bool");
    }

    #[test]
    fn any_ty_displays_with_any_keyword() {
        let hir = resolve_src("fun f() {}");
        let mut checker = checker_with_signatures_collected(&hir);
        let bool_ty = checker.tcx.mk_prim(PrimTy::Bool);
        let ty = checker.tcx.mk_any(bool_ty);
        assert_eq!(checker.cx().show(ty).to_string(), "any bool");
    }

    #[test]
    fn empty_tuple_displays_as_empty_parens() {
        let hir = resolve_src("fun f() {}");
        let mut checker = checker_with_signatures_collected(&hir);
        let ty = checker.tcx.mk_tuple(vec![]);
        assert_eq!(checker.cx().show(ty).to_string(), "()");
    }

    #[test]
    fn one_element_tuple_displays_with_a_trailing_comma() {
        let hir = resolve_src("fun f() {}");
        let mut checker = checker_with_signatures_collected(&hir);
        let bool_ty = checker.tcx.mk_prim(PrimTy::Bool);
        let ty = checker.tcx.mk_tuple(vec![bool_ty]);
        assert_eq!(checker.cx().show(ty).to_string(), "(bool,)");
    }

    #[test]
    fn multi_element_tuple_displays_comma_separated() {
        let hir = resolve_src("fun f() {}");
        let mut checker = checker_with_signatures_collected(&hir);
        let bool_ty = checker.tcx.mk_prim(PrimTy::Bool);
        let char_ty = checker.tcx.mk_prim(PrimTy::Char);
        let ty = checker.tcx.mk_tuple(vec![bool_ty, char_ty]);
        assert_eq!(checker.cx().show(ty).to_string(), "(bool, char)");
    }

    #[test]
    fn array_displays_with_brackets_and_a_placeholder_length() {
        let hir = resolve_src("fun f() {}");
        let mut checker = checker_with_signatures_collected(&hir);
        let i32_ty = checker.tcx.mk_prim(PrimTy::I32);
        let ty = checker.tcx.mk_array(i32_ty, None);
        assert_eq!(checker.cx().show(ty).to_string(), "[i32; _]");
    }

    #[test]
    fn fun_with_no_params_or_ret_displays_as_bare_fun() {
        let hir = resolve_src("fun f() {}");
        let mut checker = checker_with_signatures_collected(&hir);
        let ty = checker.tcx.mk_fun(vec![], None);
        assert_eq!(checker.cx().show(ty).to_string(), "fun()");
    }

    #[test]
    fn fun_with_params_and_ret_displays_with_arrow() {
        let hir = resolve_src("fun f() {}");
        let mut checker = checker_with_signatures_collected(&hir);
        let i32_ty = checker.tcx.mk_prim(PrimTy::I32);
        let bool_ty = checker.tcx.mk_prim(PrimTy::Bool);
        let ty = checker.tcx.mk_fun(vec![i32_ty, i32_ty], Some(bool_ty));
        assert_eq!(checker.cx().show(ty).to_string(), "fun(i32, i32) -> bool");
    }

    #[test]
    fn generic_displays_with_its_declared_name() {
        let hir = resolve_src("struct Wrap<T> { inner: T }");
        let def_id = first_struct(&hir);
        let s = hir.struct_(def_id);
        let generic_id = s.generics[0];
        let mut checker = checker_with_signatures_collected(&hir);
        let ty = checker.tcx.mk_generic(generic_id);
        assert_eq!(checker.cx().show(ty).to_string(), "T");
    }

    #[test]
    fn adt_displays_with_its_name_and_generic_args() {
        let hir = resolve_src("struct Wrap<T> { inner: T }");
        let def_id = first_struct(&hir);
        let checker = checker_with_signatures_collected(&hir);

        let ty = checker
            .types
            .ty_of_def(def_id)
            .expect("collect_struct records the struct's own type under its owner node");
        assert_eq!(checker.cx().show(ty).to_string(), "Wrap<T>");
    }

    #[test]
    fn adt_with_no_generics_displays_with_just_its_name() {
        let hir = resolve_src("struct Unit {}");
        let def_id = first_struct(&hir);
        let checker = checker_with_signatures_collected(&hir);

        let ty = checker
            .types
            .ty_of_def(def_id)
            .expect("collect_struct records the struct's own type under its owner node");
        assert_eq!(checker.cx().show(ty).to_string(), "Unit");
    }

    #[test]
    fn self_param_displays_as_self() {
        let hir = resolve_src("trait Greet { fun hello(); }");
        let def_id = first_trait(&hir);
        let checker = checker_with_signatures_collected(&hir);

        let ty = checker
            .types
            .ty_of_def(def_id)
            .expect("collect_trait records the trait's own Self type under its owner node");
        assert_eq!(checker.cx().show(ty).to_string(), "Self");
    }

    #[test]
    fn dyn_displays_with_dyn_keyword_and_trait_name() {
        let hir = resolve_src("trait Greet { fun hello(); }");
        let def_id = first_trait(&hir);
        let mut checker = checker_with_signatures_collected(&hir);
        let ty = checker.tcx.mk_dyn(def_id, vec![]);
        assert_eq!(checker.cx().show(ty).to_string(), "dyn Greet");
    }

    /// A `UnifyError` renders itself, so the wording lives next to the variant it explains
    /// rather than in a `match` somewhere in the checker.
    #[test]
    fn a_mismatch_names_both_types_as_the_user_wrote_them() {
        let hir = resolve_src("fun f() {}");
        let mut tcx = TyCtx::new();
        let (expected, found) = (tcx.mk_prim(PrimTy::I32), tcx.mk_prim(PrimTy::Bool));
        let cx = DisplayCx::new(&hir, &tcx);

        assert_eq!(
            cx.show(UnifyError::Mismatch { expected, found })
                .to_string(),
            "mismatched types: expected `i32`, found `bool`"
        );
    }

    #[test]
    fn an_int_var_mismatch_says_an_integer_type_was_expected() {
        let hir = resolve_src("fun f() {}");
        let mut tcx = TyCtx::new();
        let var = tcx.next_int_var();
        let found = tcx.mk_prim(PrimTy::Bool);
        let cx = DisplayCx::new(&hir, &tcx);

        assert_eq!(
            cx.show(UnifyError::ExpectedInteger { var, found })
                .to_string(),
            "mismatched types: expected an integer type, found `bool`"
        );
    }

    // -----------------------------------------------------------------
    // A function's body is checked against its declared return type: see `check_function`'s doc
    // comment for how one `unify` against the body's own `check_block_expecting` result covers
    // all three shapes below at once.
    // -----------------------------------------------------------------

    /// A function's trailing expression is checked against its own declared return type, the
    /// same as the equivalent closure already was (see
    /// `a_closure_checks_to_a_function_type_of_its_parameters_and_body` in `expr.rs`).
    #[test]
    fn a_functions_trailing_expression_is_checked_against_its_return_type() {
        rejects("fun f() -> i32 { true }", "mismatched types");
    }

    /// An empty body produces `()`, which is rejected for a function declaring any other return
    /// type.
    #[test]
    fn an_empty_body_is_checked_against_a_declared_return_type() {
        rejects("fun f() -> i32 {}", "mismatched types");
    }

    /// Only the `if` branch returns; falling through the missing `else` produces `()`, not the
    /// declared `i32`.
    #[test]
    fn a_partial_return_does_not_guarantee_every_path_produces_the_declared_type() {
        rejects(
            "fun f(c: bool) -> i32 { if c { return 1; } }",
            "mismatched types",
        );
    }

    /// An `if`/`else` that returns on every branch is accepted as the body's tail expression --
    /// `check_if` unifies the two branches together, and a branch whose own block diverged is
    /// `Never`, which is what lets this differ from the previous test's missing `else`.
    #[test]
    fn a_function_body_ending_in_an_if_else_that_always_returns_checks() {
        accepts("fun f(c: bool) -> i32 { if c { return 1; } else { return 2; } }");
    }

    /// The same `if`/`else`, but written as a statement (note the trailing `;`) with unreachable
    /// code after it rather than as the block's tail expression -- the case `check_block_expecting`
    /// has to recognize by the `if`'s own checked type coming out `Never`, since it is not
    /// literally a `return`/`break`/`continue` at the statement level.
    #[test]
    fn a_statement_position_if_else_that_always_returns_still_lets_later_code_check() {
        accepts(
            "fun f(c: bool) -> i32 {
                 if c { return 1; } else { return 2; };
                 return 0;
             }",
        );
    }

    // -----------------------------------------------------------------
    // Primitives, broadly
    // -----------------------------------------------------------------

    /// Every integer primitive is usable as a parameter and return type, and round-trips through
    /// a bare `return` unchanged.
    #[test]
    fn every_integer_primitive_round_trips_through_a_function_signature() {
        for name in ["i8", "i16", "i32", "i64", "u8", "u16", "u32", "u64"] {
            accepts(&format!("fun f(x: {name}) -> {name} {{ return x; }}"));
        }
    }

    #[test]
    fn every_float_primitive_round_trips_through_a_function_signature() {
        for name in ["f32", "f64"] {
            accepts(&format!("fun f(x: {name}) -> {name} {{ return x; }}"));
        }
    }

    #[test]
    fn a_negative_int_literal_still_checks_as_an_integer() {
        accepts("fun f() -> i32 { return -1; }");
    }

    #[test]
    fn a_negative_float_literal_still_checks_as_a_float() {
        accepts("fun f() -> f64 { return -1.5; }");
    }

    #[test]
    fn an_int_literal_and_a_float_literal_do_not_unify() {
        rejects("fun f() { let x = 1 + 1.0; }", "mismatched types");
    }

    #[test]
    fn a_bool_and_a_char_do_not_unify() {
        rejects(
            "fun f() -> bool { return 'a' == true; }",
            "mismatched types",
        );
    }

    // -----------------------------------------------------------------
    // `&&` and `||`: not overloadable, and require `bool` on both sides
    // -----------------------------------------------------------------

    #[test]
    fn and_and_or_accept_two_bools() {
        accepts("fun f(a: bool, b: bool) -> bool { return a && b; }");
        accepts("fun f(a: bool, b: bool) -> bool { return a || b; }");
    }

    /// `1 && true` is one mistake, one diagnostic (the stated design principle behind
    /// `TyKind::Error`/`Never` absorbing everywhere else in this pass): the `Binary` arm's own
    /// `unify(lhs, rhs)` fails, reports it, and returns `Error` immediately rather than letting
    /// `check_operator`'s `And`/`Or` branch unify the same still-unresolved operand against
    /// `bool` and report the identical mismatch a second time.
    #[test]
    fn and_rejects_operands_of_different_types_exactly_once() {
        rejects("fun f() { let x = 1 && true; }", "mismatched types");
    }

    /// Two operands that agree with each other but are not `bool` are still rejected: `&&`/`||`
    /// are hardcoded to `bool` and never reach the solver at all. The specific "need bool
    /// operands" wording lives in the diagnostic's label, not its top-level message, so it is
    /// checked against `UnifyError`'s own rendering instead -- see
    /// `an_int_var_mismatch_says_an_integer_type_was_expected` above for that wording's source.
    #[test]
    fn and_rejects_two_operands_of_the_same_non_bool_type() {
        rejects("fun f() { let x = 1 && 2; }", "expected an integer type");
    }

    // -----------------------------------------------------------------
    // Every operator lang item on one struct
    // -----------------------------------------------------------------

    /// One `extend` block providing every operator trait `core::ops` declares, exercising each
    /// operator once. `Comparable` alone backs all four of `<`/`<=`/`>`/`>=`, and `Eq` backs both
    /// `==` and `!=` -- see `Typeck::check_operator`.
    #[test]
    fn a_struct_implementing_every_operator_trait_supports_every_operator() {
        accepts(
            "module core::ops;

             public trait Add { fun add(&self, other: &Self) -> Self; }
             public trait Sub { fun sub(&self, other: &Self) -> Self; }
             public trait Mul { fun mul(&self, other: &Self) -> Self; }
             public trait Div { fun div(&self, other: &Self) -> Self; }
             public trait Rem { fun rem(&self, other: &Self) -> Self; }
             public trait Neg { fun neg(&self) -> Self; }
             public trait Not { fun not(&self) -> Self; }
             public trait Eq { fun eq(&self, other: &Self) -> bool; }
             public trait Comparable { fun compare(&self, other: &Self) -> i32; }

             struct N { v: i32 }

             extend N with Add { fun add(&self, other: &Self) -> Self { return .{ v: self.v }; } }
             extend N with Sub { fun sub(&self, other: &Self) -> Self { return .{ v: self.v }; } }
             extend N with Mul { fun mul(&self, other: &Self) -> Self { return .{ v: self.v }; } }
             extend N with Div { fun div(&self, other: &Self) -> Self { return .{ v: self.v }; } }
             extend N with Rem { fun rem(&self, other: &Self) -> Self { return .{ v: self.v }; } }
             extend N with Neg { fun neg(&self) -> Self { return .{ v: self.v }; } }
             extend N with Not { fun not(&self) -> Self { return .{ v: self.v }; } }
             extend N with Eq { fun eq(&self, other: &Self) -> bool { return true; } }
             extend N with Comparable { fun compare(&self, other: &Self) -> i32 { return 0; } }

             fun f(a: N, b: N) -> N {
                 let sum = a + b;
                 let diff = a - b;
                 let prod = a * b;
                 let quot = a / b;
                 let rem = a % b;
                 let negated = -a;
                 let inverted = !a;
                 let is_eq = a == b;
                 let is_ne = a != b;
                 let lt = a < b;
                 let le = a <= b;
                 let gt = a > b;
                 let ge = a >= b;
                 return sum;
             }",
        );
    }

    /// Each operator names its own trait in the diagnostic when the impl is missing, not a
    /// generic "operator" message -- `Sub`, `Neg`, and `Comparable` here, matching the
    /// already-covered `Add` case.
    #[test]
    fn sub_neg_and_comparable_each_report_their_own_missing_trait() {
        rejects(
            "module core::ops;
             public trait Sub { fun sub(&self, other: &Self) -> Self; }
             struct N { v: i32 }
             fun f(a: N, b: N) -> N { return a - b; }",
            "does not implement `Sub`",
        );
        rejects(
            "module core::ops;
             public trait Neg { fun neg(&self) -> Self; }
             struct N { v: i32 }
             fun f(a: N) -> N { return -a; }",
            "does not implement `Neg`",
        );
        rejects(
            "module core::ops;
             public trait Comparable { fun compare(&self, other: &Self) -> i32; }
             struct N { v: i32 }
             fun f(a: N, b: N) -> bool { return a < b; }",
            "does not implement `Comparable`",
        );
    }

    // -----------------------------------------------------------------
    // Shadowing and recursion, checked (not just resolved)
    // -----------------------------------------------------------------

    /// A `let` may rebind a name at a different type; the later binding is what a subsequent use
    /// sees, and its type is unaffected by the type the same name had before.
    #[test]
    fn a_let_may_rebind_a_name_at_a_different_type() {
        accepts(
            "fun f() -> bool {
                 let x = 1;
                 let x = true;
                 return x;
             }",
        );
    }

    /// A block-scoped shadow does not affect the outer binding once the inner block ends.
    #[test]
    fn a_block_scoped_shadow_does_not_leak_out() {
        accepts(
            "fun f() -> i32 {
                 let x = 1;
                 { let x = true; }
                 return x;
             }",
        );
        rejects(
            "fun f() -> bool {
                 let x = 1;
                 { let x = true; }
                 return x;
             }",
            "mismatched types",
        );
    }

    #[test]
    fn a_directly_recursive_function_checks() {
        accepts("fun fact(n: i32) -> i32 { return fact(n); }");
    }

    #[test]
    fn two_mutually_recursive_functions_check_regardless_of_order() {
        accepts(
            "fun is_even(n: i32) -> bool { return is_odd(n); }
             fun is_odd(n: i32) -> bool { return is_even(n); }",
        );
    }

    #[test]
    fn a_self_referencing_struct_field_behind_a_reference_checks() {
        accepts("struct Node { next: &Node }");
    }

    #[test]
    fn a_self_referencing_enum_variant_behind_a_reference_checks() {
        accepts("enum List { cons: &List, nil }");
    }

    // -----------------------------------------------------------------
    // `let`: refutability against `else`
    // -----------------------------------------------------------------

    #[test]
    fn a_plain_binding_pattern_needs_no_else() {
        accepts("fun f() { let x = 1; }");
    }

    #[test]
    fn a_wildcard_pattern_needs_no_else() {
        accepts("fun f() { let _ = 1; }");
    }

    #[test]
    fn a_tuple_of_irrefutable_patterns_needs_no_else() {
        accepts("fun f(p: (i32, bool)) { let (x, y) = p; }");
    }

    #[test]
    fn a_refutable_variant_pattern_without_an_else_is_rejected() {
        rejects(
            "enum Option<T> { some: T, none }
             fun f(o: Option<i32>) { let .some(x) = o; }",
            "with no `else`",
        );
    }

    #[test]
    fn a_refutable_variant_pattern_with_an_else_is_accepted() {
        accepts(
            "enum Option<T> { some: T, none }
             fun f(o: Option<i32>) -> i32 {
                 let .some(x) = o else { return 0; };
                 return x;
             }",
        );
    }

    #[test]
    fn a_tuple_containing_a_refutable_element_needs_an_else() {
        rejects(
            "enum Option<T> { some: T, none }
             fun f(p: (i32, Option<i32>)) { let (x, .some(y)) = p; }",
            "with no `else`",
        );
    }

    /// A single-variant enum's own pattern is irrefutable -- the same rule
    /// `check_match_exhaustive` already applies to a `match` with one unguarded arm naming that
    /// variant and nothing else.
    #[test]
    fn a_single_variant_enums_pattern_is_irrefutable() {
        accepts(
            "enum Only { one: i32 }
             fun f(o: Only) -> i32 { let .one(x) = o; return x; }",
        );
        rejects(
            "enum Only { one: i32 }
             fun f(o: Only) -> i32 { let .one(x) = o else { return 0; }; return x; }",
            "irrefutable pattern",
        );
    }

    #[test]
    fn an_irrefutable_binding_pattern_with_an_else_is_rejected() {
        rejects(
            "fun f() -> i32 { let x = 1 else { return 0; }; return x; }",
            "irrefutable pattern",
        );
    }

    #[test]
    fn an_irrefutable_tuple_pattern_with_an_else_is_rejected() {
        rejects(
            "fun f(p: (i32, bool)) -> i32 { let (x, y) = p else { return 0; }; return x; }",
            "irrefutable pattern",
        );
    }

    /// A `with` lend is never checked for refutability at all -- its own pattern is always a
    /// plain binding (`mir::lower::block::lower_with_lend` panics on anything else), and it has
    /// no `else` to reject one against in the first place.
    #[test]
    fn a_with_lends_pattern_is_never_checked_for_refutability() {
        accepts("fun f(x: i32) { with y = &x { let _ = y; } }");
    }
}
