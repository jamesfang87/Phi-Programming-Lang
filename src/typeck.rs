//! Type checking runs in two stages, for the same reason name resolution runs before it: a
//! definition's body can refer to any other definition in the program, including ones declared
//! later, so nothing about a body can be checked until every *signature* is known.
//!
//! [`check`] runs both stages in order. The first is [`Typeck::collect_module`], which walks
//! every declaration in the program -- a struct's fields, an enum's variants, a function's
//! parameters and return type -- and converts each type annotation the user wrote into the
//! [`Ty`] the checker reasons about, recording it in [`TypeResolutions`]. It never looks inside
//! a function body. The second is [`Typeck::check_module`], which checks those bodies against
//! the signatures the first stage collected. Every `collect_*` therefore runs before any
//! `check_*`.
//!
//! A third stage sits between them: [`Typeck::build_impl_index`], [`Typeck::check_coherence`],
//! [`Typeck::check_trait_members`], [`Typeck::check_declared_bounds`],
//! [`Typeck::check_impl_headers`] and [`Typeck::select_program_obligations`], which collect every
//! `extend` block in the program into an index the trait solver can look up in, check that no two
//! of them can both apply to one type, check each of them against the trait it implements, and
//! then prove the bounds collection raised while that index did not yet exist. Its position is
//! exact. Coherence needs every `extend` header lowered to a [`Ty`], so it cannot run before
//! collection; bodies ask the solver questions, so they cannot be checked before coherence has
//! made the answer to those questions unique. See [`traits`].

use std::collections::{HashMap, HashSet};

use crate::ast::{BinaryOp, Literal, Mutability, SelfMode, UnaryOp};
use crate::diag::{DiagCtx, Diagnostic};
use crate::driver::source::SrcSpan;
use crate::hir::visit::{self, Visitor};
use crate::hir::{
    DefId, ExprKind, Hir, HirId, Local, Node, OwnerNode, Res, StmtKind, VariantPayload,
};
use crate::langitems::LangItem;
use crate::nameres::PrimTy;
use crate::typeck::display::DisplayCx;
use crate::typeck::results::TypeResolutions;
use crate::typeck::traits::bounds::ObligationCx;
use crate::typeck::traits::index::ImplIndex;
use crate::typeck::traits::solve::{Obligation, ParamEnv, TraitName};
use crate::typeck::ty::{Ty, TyKind};
use crate::typeck::tyctx::TyCtx;
use crate::typeck::unify::{Unifier, UnifyError};

pub mod display;
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

    /// Owns every type this pass builds. Handed back by [`check`] along with the results,
    /// since a [`Ty`] means nothing without it.
    tcx: TyCtx,

    /// The type of every node this pass has worked out. Reached through [`Typeck::ty_of`] and
    /// [`Typeck::recorded_ty`] rather than directly, which is what keeps the table and the
    /// unifier from drifting apart.
    types: TypeResolutions,

    /// The union-find over every [`Ty`] the checker has unified so far. Lives for the whole
    /// pass, alongside `tcx`, rather than being created fresh per call, so that equivalences
    /// established while checking one expression are still known while checking the next.
    unifier: Unifier,

    /// Every `extend` block in the program, keyed for lookup. Empty until
    /// [`Typeck::build_impl_index`] runs, which is why nothing may ask the solver a question
    /// before then.
    impls: ImplIndex,

    /// What each definition may assume about its own type parameters, worked out on first use.
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
}

impl<'hir> Typeck<'hir> {
    /// A checker that has looked at nothing yet. Every stage below is driven from
    /// [`check`], which is what puts them in the right order; this exists so that a test can
    /// stop after any one of them.
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
        }
    }

    /// Stage one: records the type of every declaration under `module_id`, without checking a
    /// body. See [`Collect`] for how the traversal is driven.
    pub fn collect_module(&mut self, module_id: DefId) {
        Collect(self).visit_module(module_id);
    }

    /// Collects a function's signature: its type parameters, the type of `self` if it is a
    /// method, each parameter's type, and its return type.
    ///
    /// The body is deliberately skipped. Checking it needs every other signature in the program
    /// to be collected first, which is exactly what this pass is producing.
    pub fn collect_function(&mut self, function: DefId) {
        // Reborrow the HIR at its declaration lifetime rather than through `self`. Since `&'hir Hir`
        // is `Copy` and has a longer lifetime than `self`'s mutable borrow, field reads on this
        // reference remain valid across all `&mut self` method calls below. This allows the
        // signature components to be extracted without cloning, because they are directly borrowed
        // from the arena.
        let hir: &'hir Hir = self.hir;
        let OwnerNode::Function(function_node) = hir.def(function) else {
            unreachable!("root of a Function owner is always OwnerNode::Function");
        };
        let (generics, self_param, params, ret) = (
            &function_node.generics,
            function_node.self_param,
            &function_node.params,
            function_node.ret,
        );

        self.collect_generics(generics);

        // A method's `self` counts as its first parameter, so that a signature says everything
        // a call has to be checked against without the caller having to look the receiver up
        // separately.
        let mut param_tys = Vec::with_capacity(params.len() + usize::from(self_param.is_some()));
        if let Some(id) = self_param {
            param_tys.push(self.collect_self_param(id));
        }
        for &id in params {
            let Node::Param(param) = hir.node(id) else {
                unreachable!("Node that is not a parameter found in a function's parameter list");
            };

            let ty = self.lower_ty(param.ty);
            self.types.record(id, ty);
            param_tys.push(ty);
        }
        let ret = ret.map(|ret| self.lower_ty(ret));

        let sig = self.tcx.mk_fun(param_tys, ret);
        self.types.record_def(function, sig);
    }

    /// Gives the `self` parameter of a method its type: the enclosing `Self`, wrapped according
    /// to how the method takes it. `any self` accepts every one of the other three forms at
    /// once, which no single type describes, so it keeps the bare `Self` type and leaves the
    /// distinction to be enforced at the call site.
    fn collect_self_param(&mut self, id: HirId) -> Ty {
        let Node::SelfParam(self_param) = self.hir.node(id) else {
            unreachable!("Node that is not a self param found in a function's self param slot");
        };
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
        let OwnerNode::Struct(struct_node) = hir.def(r#struct) else {
            unreachable!("root of a Struct owner is always OwnerNode::Struct");
        };
        let (generics, fields, span) =
            (&struct_node.generics, &struct_node.fields, struct_node.span);

        // The generics have to be recorded first: the struct's own type is itself applied to
        // them.
        self.collect_generics(generics);
        let self_ty = self.self_ty(r#struct, span);
        self.types.record_def(r#struct, self_ty);

        self.collect_fields(fields);
    }

    pub fn collect_enum(&mut self, r#enum: DefId) {
        let hir: &'hir Hir = self.hir;
        let OwnerNode::Enum(enum_node) = hir.def(r#enum) else {
            unreachable!("root of an Enum owner is always OwnerNode::Enum");
        };
        let (generics, variants, span) = (&enum_node.generics, &enum_node.variants, enum_node.span);

        self.collect_generics(generics);
        let self_ty = self.self_ty(r#enum, span);
        self.types.record_def(r#enum, self_ty);

        for &id in variants {
            let Node::Variant(variant) = hir.node(id) else {
                unreachable!("Node that is not a variant found in an enum's variant list");
            };

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
        let OwnerNode::Trait(trait_node) = hir.def(r#trait) else {
            unreachable!("root of a Trait owner is always OwnerNode::Trait");
        };
        let (generics, span) = (&trait_node.generics, trait_node.span);

        self.collect_generics(generics);
        // A trait names no type of its own, so what it gets recorded as is the `Self` it stands
        // for: the placeholder every implementing type substitutes.
        let self_ty = self.self_ty(r#trait, span);
        self.types.record_def(r#trait, self_ty);
    }

    /// Collects an `extend` block's three bracket groups and the signature of each method it
    /// holds.
    ///
    /// The three groups scope differently -- see [`Extend`](crate::hir::Extend) -- but all three
    /// lower the same way here. The block's own `<T>` list is written as types even though it
    /// declares parameters, and name resolution has already bound each entry to itself, so
    /// lowering one yields the [`Generic`](crate::typeck::ty::TyKind::Generic) it declares.
    pub fn collect_extend(&mut self, extend: DefId) {
        let hir: &'hir Hir = self.hir;
        let OwnerNode::Extend(extend_node) = hir.def(extend) else {
            unreachable!("root of an Extend owner is always OwnerNode::Extend");
        };
        let (extend_generics, adt_generics, trait_generics, span) = (
            &extend_node.extend_generics,
            &extend_node.adt_generics,
            &extend_node.trait_generics,
            extend_node.span,
        );

        // The first group declares parameters, the other two apply arguments -- so the first is
        // collected like any other generics list and the others are lowered as types.
        self.collect_generics(extend_generics);
        self.lower_tys(adt_generics);
        self.lower_tys(trait_generics);

        // Which is the extended type applied to `adt_generics`, so this is also what `Self`
        // means inside each method of the block.
        let self_ty = self.self_ty(extend, span);
        self.types.record_def(extend, self_ty);
    }

    /// Records the type each of `def_id`'s own type parameters stands for: itself.
    fn collect_generics(&mut self, generics: &[HirId]) {
        for &id in generics {
            debug_assert!(
                matches!(self.hir.node(id), Node::Generic(_)),
                "Node that is not a generic found in a generics list"
            );

            let ty = self.tcx.mk_generic(id);
            self.types.record(id, ty);
        }
    }

    /// Records the declared type of each field in a struct or a record variant.
    fn collect_fields(&mut self, fields: &[HirId]) {
        for &id in fields {
            let Node::Field(field) = self.hir.node(id) else {
                unreachable!("Node that is not a field found in a field list");
            };

            let ty = self.lower_ty(field.ty);
            self.types.record(id, ty);
        }
    }

    /// Stage two: checks every body under `module`, against the signatures
    /// [`Typeck::collect_module`] recorded. See [`Check`] for how the traversal is driven.
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
    fn ty_of_maybe_expecting(&mut self, id: HirId, expected: Option<Ty>) -> Ty {
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

    /// [`Typeck::recorded_ty`] for a definition's own type: a `struct`'s type, a `fun`'s
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
            self.types.record(id, resolved);
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
            // with, so it stays a variable -- fallback to `i32`/`f64` for an unconstrained
            // literal is a separate step, not yet written.
            TyKind::Var(_)
            | TyKind::Primitive(_)
            | TyKind::Generic(_)
            | TyKind::SelfTy(_)
            | TyKind::Unit
            | TyKind::Never
            | TyKind::Error => ty,
        }
    }

    fn resolve_deep_all(&mut self, tys: &[Ty]) -> Vec<Ty> {
        tys.iter().map(|&ty| self.resolve_deep(ty)).collect()
    }

    /// Works out the type of the expression `id` names, without recording it. Private, and
    /// `#[must_use]`, because [`Typeck::ty_of`] is what puts a type in the table -- reaching
    /// this directly is how a computed type gets dropped on the floor.
    #[must_use]
    fn check_expr(&mut self, id: HirId) -> Ty {
        let Node::Expr(expr) = self.hir.node(id) else {
            unreachable!("Node that is not an expr passed to check_expr");
        };

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
                let is_primitive = matches!(self.tcx.kind(resolved), TyKind::Primitive(_));

                let item = match op {
                    UnaryOp::Neg => LangItem::Neg,
                    UnaryOp::Not => LangItem::Not,
                };

                if is_primitive {
                    // `-`/`!` on `i32`/`bool` and friends are built in -- no `extend` block
                    // backs a primitive, so there is nothing for the solver to find.
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
                    DiagCtx::emit(
                        Diagnostic::error(self.cx().show(error).to_string(), expr.span).with_label(
                            format!(
                                "cannot use incompatible types {} and {} in binary operation",
                                self.cx().show(lhs),
                                self.cx().show(rhs)
                            ),
                        ),
                    );
                }
                let resolved = self.unifier.root(lhs);
                self.check_operator(*op, resolved, id.owner, expr.span)
            }
            ExprKind::Assign { lhs, rhs } => self.check_assign(*lhs, *rhs, expr.span),
            ExprKind::AssignOp { op, lhs, rhs } => {
                self.check_assign_op(*op, *lhs, *rhs, expr.span)
            }
            ExprKind::Borrow {
                mutability,
                operand,
            } => self.check_borrow(*mutability, *operand, expected),
            ExprKind::Call { callee, args } => self.check_call(*callee, args, expr.span),
            ExprKind::Access { base, member, args } => {
                self.check_access(id, *base, *member, args, expr.span)
            }
            ExprKind::Index { base, index } => self.check_index(*base, *index, expr.span),
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
                self.check_match(*scrutinee, arms, expected)
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
            ExprKind::Error => self.tcx.error(),
        };

        ty
    }

    /// The type `op` produces for two operands that have already been unified into `operand`, and
    /// the check that `op` applies to that type at all.
    ///
    /// Each operator maps onto the `core::ops` trait its lang item names, so unifying the two
    /// sides is required but not sufficient: `foo + bar` also needs an `extend Foo with Add`
    /// block. A primitive short-circuits that -- no `extend` block backs `i32`, so there would be
    /// nothing for the solver to find.
    ///
    /// Shared with [`Typeck::check_assign_op`], which asks the same question of `+=` as this does
    /// of `+`.
    fn check_operator(&mut self, op: BinaryOp, operand: Ty, owner: DefId, span: SrcSpan) -> Ty {
        let is_primitive = matches!(self.tcx.kind(operand), TyKind::Primitive(_));
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
                if is_primitive || self.operator_holds(item, operand, owner, span) {
                    // Every `core::ops` trait an operator dispatches to returns `Self`, so the
                    // operand's own type is the result.
                    operand
                } else {
                    self.tcx.error()
                }
            }
            BinaryOp::Eq | BinaryOp::Ne => {
                if is_primitive || self.operator_holds(LangItem::Eq, operand, owner, span) {
                    bool_ty
                } else {
                    self.tcx.error()
                }
            }
            BinaryOp::Lt | BinaryOp::Le | BinaryOp::Gt | BinaryOp::Ge => {
                if is_primitive || self.operator_holds(LangItem::Comparable, operand, owner, span) {
                    bool_ty
                } else {
                    self.tcx.error()
                }
            }
            BinaryOp::And | BinaryOp::Or => {
                // Not overloadable -- the core lib has no logic trait, so `&&`/`||` only ever
                // mean the primitive short-circuit operators.
                if let Err(error) = self.unifier.unify(&self.tcx, operand, bool_ty) {
                    DiagCtx::emit(
                        Diagnostic::error(self.cx().show(error).to_string(), span).with_label(
                            format!(
                                "`&&`/`||` need bool operands, found {}",
                                self.cx().show(operand)
                            ),
                        ),
                    );
                }
                bool_ty
            }
        }
    }

    /// Whether `self_ty` implements the operator trait `item` names -- `Add`, `Neg`, `Eq`, and so
    /// on.
    ///
    /// A one-line wrapper over [`Typeck::require_extends`] that supplies the two things every
    /// operator has in common: an operator trait takes no generic arguments of its own, and the
    /// label to put on the diagnostic is always "this operator". What it does *not* do is work
    /// out the operator's result type, because that differs per operator -- every `core::ops`
    /// trait returns `Self` except `Eq` and `Comparable`, which return `bool` -- so the caller
    /// keeps that decision.
    fn operator_holds(&mut self, item: LangItem, self_ty: Ty, owner: DefId, span: SrcSpan) -> bool {
        self.require_extends(
            self_ty,
            TraitName::Lang(item),
            Vec::new(),
            owner,
            span,
            "this operator",
        )
    }

    /// The type of a literal. Every kind of literal is trivial except an unsuffixed number: `1`
    /// and `1.0` start out as the fallback-carrying [`TyVar::Int`](crate::typeck::ty::TyVar::Int)
    /// and [`TyVar::Float`](crate::typeck::ty::TyVar::Float) inference variables described on
    /// [`TyVar`](crate::typeck::ty::TyVar), narrowed once unification meets a concrete type or
    /// falls back to `i32`/`f64` if it never does.
    ///
    /// Shared with [`Typeck::check_pat`], since a literal in a pattern is the same literal and
    /// takes the same type -- `span` is what lets the one case that reports do so against
    /// whichever of the two it was written in.
    pub(crate) fn check_literal(&mut self, lit: &Literal, span: SrcSpan) -> Ty {
        match lit {
            Literal::Bool(_) => self.tcx.mk_prim(PrimTy::Bool),
            Literal::Char(_) => self.tcx.mk_prim(PrimTy::Char),
            // TODO: read `suffix` (`i32`, `u8`, ...) once literal suffixes are interpreted, and
            // lower straight to that `PrimTy` instead of an inference variable.
            Literal::Int { .. } => self.tcx.next_int_var(),
            Literal::Float { .. } => self.tcx.next_float_var(),
            // A string literal is a value of some string type, and there is nothing here for it to
            // be: the core library declares no `String`, and `LangItem` names none, so there is no
            // definition to resolve one to. Reported rather than given a stand-in type, which
            // would make every use of it check against something no later pass could lower.
            Literal::Str(_) => {
                DiagCtx::emit(
                    Diagnostic::error("a string literal has no type yet", span)
                        .with_label("`str` is not a type the core library declares")
                        .with_help(
                            "the core library declares no string type and no lang item names one, \
                             so there is nothing for this literal to be",
                        ),
                );
                self.tcx.error()
            }
        }
    }

    pub fn check_stmt(&mut self, id: HirId) {
        let Node::Stmt(stmt) = self.hir.node(id) else {
            unreachable!("Node which is not a stmt passed to check_stmt");
        };

        match &stmt.kind {
            StmtKind::Let {
                pat,
                ty,
                init,
                else_block,
                ..
            } => {
                let (pat, ty, init, else_block) = (*pat, *ty, *init, *else_block);
                self.check_binding(pat, ty, init, stmt.span);

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
                if let Err(err) = self.unifier.unify(&self.tcx, ret, expr_ty) {
                    self.report_return_mismatch(err, stmt.span);
                }
            }
            // `return;` with no value produces nothing, which the enclosing definition has to
            // agree to.
            StmtKind::Return(None) => {
                let ret = self.return_ty(id.owner);
                let unit = self.tcx.unit();
                if let Err(err) = self.unifier.unify(&self.tcx, ret, unit) {
                    self.report_return_mismatch(err, stmt.span);
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
        let init_ty = self.ty_of_maybe_expecting(init, declared);

        let bound = match declared {
            Some(declared) => {
                if let Err(err) = self.unifier.unify(&self.tcx, declared, init_ty) {
                    DiagCtx::emit(
                        Diagnostic::error(self.cx().show(err).to_string(), span).with_label(
                            "the value this binding is given does not match its declared type",
                        ),
                    );
                }
                declared
            }
            None => init_ty,
        };
        self.check_pat(pat, bound);
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

    /// Everything needed to render this pass's types the way the user wrote them. Build one
    /// where a diagnostic is emitted rather than holding on to it -- it borrows `self`.
    fn cx(&self) -> DisplayCx<'_> {
        DisplayCx::new(self.hir, &self.tcx)
    }

    /// Reports why a `return`'s expression didn't unify with the enclosing function's return
    /// type, at the `return` statement's span.
    fn report_return_mismatch(&self, err: UnifyError, span: SrcSpan) {
        DiagCtx::emit(
            Diagnostic::error(self.cx().show(err).to_string(), span).with_label(
                "returned value does not match this \
                function's return type",
            ),
        );
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
        let Node::Block(block) = self.hir.node(id) else {
            unreachable!("Node which is not a block passed to check_block");
        };
        let tail = block.expr;

        let mut diverges = false;
        for &stmt in &block.stmts {
            self.check_stmt(stmt);
            diverges |= matches!(
                self.hir.stmt(stmt).kind,
                StmtKind::Return(_) | StmtKind::Break | StmtKind::Continue
            );
        }

        // A block's trailing expression is not a statement, so the loop above never reaches it.
        // Checking it here is what types a function body written as a bare expression -- and it
        // happens even for a block that has already diverged, since those nodes still need types.
        let tail_ty = match tail {
            Some(tail) => self.ty_of_maybe_expecting(tail, expected),
            // A block that ends in a statement produces nothing.
            None => self.tcx.unit(),
        };

        // A block that leaves through a `return`, `break`, or `continue` never reaches its own
        // end, so it produces no value of any type -- which is what `Never` says, and why it
        // unifies with whatever the context wanted. Without this, `|x| { return x; }` would be
        // read as producing `()`.
        //
        // Only a statement *of* this block is looked at. Divergence hidden inside an expression
        // -- a call to a function that never returns -- is not tracked, so this errs towards
        // treating a block as completing normally.
        if diverges {
            self.tcx.never()
        } else {
            tail_ty
        }
    }

    /// Checks `def_id`'s body against the signature stage one collected for it, and bakes the
    /// resulting types into the table.
    ///
    /// The body's *trailing expression* is deliberately not checked against the declared return
    /// type, though [`Typeck::check_block`] now hands one back. Doing so needs divergence to be
    /// tracked further than it is: `fun f() -> i32 { if c { return 1; } else { return 2; } }` ends
    /// in a block that produces no value and reaches no `return` statement of its own, so a check
    /// here would reject it. A `return` inside the body is checked, which is what a body that ends
    /// in one is relying on; a closure's body, which cannot use a bare `return` to stand in for
    /// its value the same way, is checked -- see [`Typeck::check_closure`].
    pub fn check_function(&mut self, def_id: DefId) {
        let OwnerNode::Function(function) = self.hir.def(def_id) else {
            unreachable!("root of a Function owner is always OwnerNode::Function");
        };

        if let Some(block) = function.block {
            // Bounds raised while checking the body are proved at the end of it and not before:
            // an argument written as `_` early on is only known once the rest of the body has
            // pinned it down, and asking sooner would answer "ambiguous" to a question that has a
            // perfectly good answer a few statements later.
            self.in_body = true;
            self.check_block(block);
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

/// What type checking produces: the type of every node it worked out, and the arena those types
/// live in.
///
/// The two travel together because a [`Ty`] is an index into a [`TyCtx`] and means nothing
/// without it.
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
    use crate::diag::Severity;
    use crate::nameres::PrimTy;
    use crate::testing::{
        find_return, first_extend_method, first_function, first_struct, first_trait, resolve_src,
    };

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
        let OwnerNode::Module(module) = hir.def(from) else {
            unreachable!("find_owner only recurses into Module owners");
        };

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
        let OwnerNode::Module(module) = hir.def(hir.root_id()) else {
            unreachable!("root of a Module owner is always OwnerNode::Module");
        };
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
        let OwnerNode::Struct(s) = hir.def(def_id) else {
            unreachable!("first_struct only returns a struct's DefId");
        };
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
}
