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

use std::collections::{HashMap, HashSet};

use crate::ast::{Literal, Mutability, SelfMode};
use crate::diag::{DiagCtx, Diagnostic};
use crate::hir::{
    DefId, ExprKind, Hir, HirId, NameResolutions, Node, OwnerNode, StmtKind, VariantPayload,
};
use crate::lexer::src_span::SrcSpan;
use crate::nameres::results::{PrimTy, ValueRes};
use crate::typeck::display::DisplayCx;
use crate::typeck::results::TypeResolutions;
use crate::typeck::ty::{Ty, TyKind};
use crate::typeck::tyctx::TyCtx;
use crate::typeck::unify::{Unifier, UnifyError};

pub mod display;
pub mod lower_ty;
pub mod results;
pub mod ty;
pub mod tyctx;
pub mod unify;

pub struct Typeck<'hir> {
    hir: &'hir Hir,

    /// What every path in the program resolved to. Type lowering reads this instead of doing its
    /// own name lookups -- see [`lower_ty`].
    nameres: &'hir NameResolutions,

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

    /// What `Self` lowers to inside each definition that introduces one, cached because `Self` is
    /// typically written many times in one body. Filled in on demand by
    /// [`Typeck::self_ty`](crate::typeck::Typeck).
    self_tys: HashMap<DefId, Ty>,

    /// The definitions whose `Self` is being computed right now, used to cut off a `Self` that
    /// is defined in terms of itself instead of recursing forever.
    computing_self_tys: HashSet<DefId>,
}

impl<'hir> Typeck<'hir> {
    pub fn collect_module(&mut self, module_id: DefId) {
        let OwnerNode::Module(module) = self.hir.def(module_id) else {
            unreachable!("root of a Module owner is always OwnerNode::Module");
        };

        for &item in &module.items {
            match self.hir.def(item) {
                OwnerNode::Module(_) => self.collect_module(item),
                OwnerNode::Function(_) => self.collect_function(item),
                OwnerNode::Struct(_) => self.collect_struct(item),
                OwnerNode::Enum(_) => self.collect_enum(item),
                OwnerNode::Trait(_) => self.collect_trait(item),
                OwnerNode::Extend(_) => self.collect_extend(item),
                _ => {
                    unreachable!(
                        "A module should not contain fields, variants, type params, and closures in the top level"
                    )
                }
            }
        }
    }

    /// Collects a function's signature: its type parameters, the type of `self` if it is a
    /// method, each parameter's type, and its return type.
    ///
    /// The body is deliberately skipped. Checking it needs every other signature in the program
    /// to be collected first, which is exactly what this pass is producing.
    pub fn collect_function(&mut self, function: DefId) {
        // Read the HIR at its own lifetime rather than through `self`. `&'hir Hir` is `Copy` and
        // outlives the borrow of `self`, so the nodes below stay readable across the `&mut self`
        // calls that follow -- which is what the signature is being copied out of, and why none
        // of it has to be cloned first.
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
        let (generics, functions, span) =
            (&trait_node.generics, &trait_node.functions, trait_node.span);

        self.collect_generics(generics);
        // A trait names no type of its own, so what it gets recorded as is the `Self` it stands
        // for: the placeholder every implementing type substitutes.
        let self_ty = self.self_ty(r#trait, span);
        self.types.record_def(r#trait, self_ty);

        for &function in functions {
            self.collect_function(function);
        }
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
        let (extend_generics, adt_generics, trait_generics, methods, span) = (
            &extend_node.extend_generics,
            &extend_node.adt_generics,
            &extend_node.trait_generics,
            &extend_node.methods,
            extend_node.span,
        );

        // The first group declares parameters, the other two apply arguments -- so the first is
        // collected like any other generics list and the others are lowered as types.
        self.collect_generics(extend_generics);
        self.lower_tys(adt_generics);
        self.lower_tys(trait_generics);

        // Which is the extended type applied to `adt_generics`, so this is also what `Self`
        // means inside each method below.
        let self_ty = self.self_ty(extend, span);
        self.types.record_def(extend, self_ty);

        for &method in methods {
            self.collect_function(method);
        }
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

    pub fn check_module(&mut self, module: DefId) {
        let OwnerNode::Module(module_node) = self.hir.def(module) else {
            unreachable!("root of a Module owner is always OwnerNode::Module");
        };

        for &item in &module_node.items {
            match self.hir.def(item) {
                OwnerNode::Module(_) => self.check_module(item),
                OwnerNode::Function(_) => self.check_function(item),
                OwnerNode::Trait(_) => self.check_trait(item),
                OwnerNode::Extend(_) => self.check_extend(item),
                // A struct or enum has no body of its own to check -- `collect_module` already
                // recorded the types its fields and variants declare, and it introduces no
                // executable code beyond that.
                OwnerNode::Struct(_) | OwnerNode::Enum(_) => {}
                _ => {
                    unreachable!(
                        "A module should not contain fields, variants, type params, and closures in the top level"
                    )
                }
            }
        }
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

        let ty = match &expr.kind {
            ExprKind::Literal(lit) => self.check_literal(lit),
            ExprKind::Tuple(elems) => {
                let tys = elems.iter().map(|&elem| self.ty_of(elem)).collect();
                self.tcx.mk_tuple(tys)
            }
            ExprKind::Path(_) => {
                let Some(res) = self.nameres.value(id) else {
                    unreachable!(
                        "every Path expr is resolved by name resolution before typeck runs"
                    );
                };

                match res {
                    // A local's type was already recorded if it names a parameter
                    // (`collect_function`).
                    //
                    // A `let`/`with` binding's is not: inferring one from the initializer and
                    // the annotation is still unwritten (see `check_stmt`'s `StmtKind::Let`
                    // arm, which checks nothing but the `else` block). Until it is, the binding
                    // gets one inference variable recorded against the pattern that introduced
                    // it, so at least every use of the same local agrees with every other and
                    // unifying one use constrains the rest. Nothing ever *binds* that variable,
                    // so the local's type stays unknown -- this stands in for the missing
                    // inference rather than doing any of it.
                    ValueRes::Local(local) => self.recorded_ty(local).unwrap_or_else(|| {
                        let ty = self.tcx.next_ty_var();
                        self.types.record(local, ty);
                        ty
                    }),
                    ValueRes::SelfVal(self_param) => self
                        .recorded_ty(self_param)
                        .expect("collect_self_param always records the self parameter's type"),
                    ValueRes::Def(def) => self
                        .recorded_ty_of_def(def)
                        .expect("collect_function always records a function's own signature"),
                    // Already reported by name resolution; staying quiet here keeps one mistake
                    // from producing a second diagnostic.
                    ValueRes::Err => self.tcx.error(),

                    // A variant is reached through `.v`, which lowers to `ExprKind::Access`, not
                    // through a path. Everything the type namespace can answer with is not in
                    // `ValueRes` at all, so there is nothing further to rule out here.
                    ValueRes::Variant(_) => {
                        unreachable!("a variant is named through an Access, not a Path")
                    }
                }
            }
            ExprKind::Unary { .. } => todo!("check_expr: Unary"),
            ExprKind::Binary { lhs, rhs, .. } => {
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
                // What the operator itself demands of its operands -- that `+` takes numbers,
                // that `&&` takes bools -- and what it produces is still unwritten; all that
                // happens above is that the two sides are required to agree with each other.
                todo!("check_expr: Binary")
            }
            ExprKind::Assign { .. } => todo!("check_expr: Assign"),
            ExprKind::AssignOp { .. } => todo!("check_expr: AssignOp"),
            ExprKind::Borrow { .. } => todo!("check_expr: Borrow"),
            ExprKind::Call { .. } => todo!("check_expr: Call"),
            ExprKind::Access { .. } => todo!("check_expr: Access"),
            ExprKind::Index { .. } => todo!("check_expr: Index"),
            ExprKind::Ctor { .. } => todo!("check_expr: Ctor"),
            ExprKind::Variant { .. } => todo!("check_expr: Variant"),
            ExprKind::Range { .. } => todo!("check_expr: Range"),
            ExprKind::Try(_) => todo!("check_expr: Try"),
            ExprKind::If { .. } => todo!("check_expr: If"),
            ExprKind::Match { .. } => todo!("check_expr: Match"),
            ExprKind::Loop { block, .. } => {
                self.check_block(*block);
                // A `loop`/`while`/`for` expression produces no value of its own.
                self.tcx.unit()
            }
            ExprKind::Spawn(_) => todo!("check_expr: Spawn"),
            ExprKind::Concurrent(_) => todo!("check_expr: Concurrent"),
            ExprKind::Block(block_id) => {
                let block_id = *block_id;
                self.check_block(block_id);

                let Node::Block(block) = self.hir.node(block_id) else {
                    unreachable!("Node which is not Node::Block found for a block expr's id")
                };

                match block.expr {
                    Some(tail) => self.ty_of(tail),
                    None => self.tcx.unit(),
                }
            }
            ExprKind::Closure(_) => todo!("check_expr: Closure"),
            ExprKind::Error => self.tcx.error(),
        };

        ty
    }

    /// The type of a literal. Every kind of literal is trivial except an unsuffixed number: `1`
    /// and `1.0` start out as the fallback-carrying [`TyVar::Int`](crate::typeck::ty::TyVar::Int)
    /// and [`TyVar::Float`](crate::typeck::ty::TyVar::Float) inference variables described on
    /// [`TyVar`](crate::typeck::ty::TyVar), narrowed once unification meets a concrete type or
    /// falls back to `i32`/`f64` if it never does.
    fn check_literal(&mut self, lit: &Literal) -> Ty {
        match lit {
            Literal::Bool(_) => self.tcx.mk_prim(PrimTy::Bool),
            Literal::Char(_) => self.tcx.mk_prim(PrimTy::Char),
            // TODO: read `suffix` (`i32`, `u8`, ...) once literal suffixes are interpreted, and
            // lower straight to that `PrimTy` instead of an inference variable.
            Literal::Int { .. } => self.tcx.next_int_var(),
            Literal::Float { .. } => self.tcx.next_float_var(),
            Literal::Str(_) => todo!("check_literal: Str (needs the `String` lang item)"),
        }
    }

    pub fn check_stmt(&mut self, id: HirId) {
        let Node::Stmt(stmt) = self.hir.node(id) else {
            unreachable!("Node which is not a stmt passed to check_stmt");
        };

        match &stmt.kind {
            StmtKind::Let { else_block, .. } => {
                // pat, ty, init

                if let Some(&block) = else_block.as_ref() {
                    self.check_block(block);
                }
            }
            StmtKind::With { block, .. } => {
                self.check_block(*block);
            }
            StmtKind::Return(Some(expr)) => {
                let expr = *expr;
                let expr_ty = self.ty_of(expr);

                let OwnerNode::Function(_) = self.hir.def(id.owner) else {
                    unreachable!("Return statement found in non-function");
                };

                let sig = self
                    .recorded_ty_of_def(id.owner)
                    .expect("a function's signature is collected before its body is checked");
                let TyKind::Fun { ret, .. } = self.tcx.kind(sig) else {
                    unreachable!("a function's own signature always lowers to TyKind::Fun");
                };
                // A function with no declared return type produces nothing, which is exactly
                // what `Unit` means. `Never` would be wrong here: it unifies with everything,
                // so `return <anything>` from a function that returns nothing would be accepted
                // silently.
                let ret = ret.unwrap_or_else(|| self.tcx.unit());

                // The declared return type is what the context demands, so it goes in `expected`
                // and the returned expression in `found` -- otherwise the diagnostic reads
                // backwards ("expected `bool`, found `()`" for a `bool` returned from a function
                // declared to return nothing).
                if let Err(err) = self.unifier.unify(&self.tcx, ret, expr_ty) {
                    self.report_return_mismatch(err, stmt.span);
                }
            }
            StmtKind::Defer(expr) | StmtKind::Expr(expr) => {
                self.ty_of(*expr);
            }
            _ => {}
        }
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

    pub fn check_block(&mut self, id: HirId) {
        let Node::Block(block) = self.hir.node(id) else {
            unreachable!("Node which is not a block passed to check_block");
        };
        let tail = block.expr;

        for &stmt in &block.stmts {
            self.check_stmt(stmt);
        }

        // A block's trailing expression is not a statement, so the loop above never reaches it.
        // Checking it here is what types a function body written as a bare expression.
        if let Some(tail) = tail {
            self.ty_of(tail);
        }
    }

    pub fn check_function(&mut self, def_id: DefId) {
        let OwnerNode::Function(function) = self.hir.def(def_id) else {
            unreachable!("root of a Function owner is always OwnerNode::Function");
        };

        if let Some(block) = function.block {
            self.check_block(block);
        }
        self.writeback(def_id);
    }

    pub fn check_trait(&mut self, def_id: DefId) {
        let OwnerNode::Trait(trait_) = self.hir.def(def_id) else {
            unreachable!("root of a Trait owner is always OwnerNode::Trait");
        };

        for &function in &trait_.functions {
            self.check_function(function);
        }
    }

    pub fn check_extend(&mut self, def_id: DefId) {
        let OwnerNode::Extend(extend) = self.hir.def(def_id) else {
            unreachable!("root of an Extend owner is always OwnerNode::Extend");
        };

        for &method in &extend.methods {
            self.check_function(method);
        }
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
pub fn check(hir: &Hir, nameres: &NameResolutions) -> TypeckOutput {
    let mut checker = Typeck {
        hir,
        nameres,
        tcx: TyCtx::new(),
        types: TypeResolutions::new(),
        unifier: Unifier::new(),
        self_tys: HashMap::new(),
        computing_self_tys: HashSet::new(),
    };
    checker.collect_module(hir.root_id());
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
    use crate::nameres::results::PrimTy;
    use crate::testing::{
        find_return, first_extend_method, first_function, first_struct, first_trait, resolve_src,
    };

    /// Builds a `Typeck` with every signature collected, ready for `check_stmt` to be called
    /// directly on one of `def_id`'s statements.
    fn checker_with_signatures_collected<'hir>(
        hir: &'hir Hir,
        nameres: &'hir NameResolutions,
    ) -> Typeck<'hir> {
        let mut checker = Typeck {
            hir,
            nameres,
            tcx: TyCtx::new(),
            types: TypeResolutions::new(),
            unifier: Unifier::new(),
            self_tys: HashMap::new(),
            computing_self_tys: HashSet::new(),
        };
        checker.collect_module(hir.root_id());
        checker
    }

    #[test]
    fn return_stmt_accepts_a_value_matching_the_return_type() {
        // `0`'s int-inference var unifies fine with the declared `i32` return type.
        let (hir, nameres) = resolve_src("fun f() -> i32 { return 0; }");
        let def_id = first_function(&hir);
        let (stmt_id, _expr_id) = find_return(&hir, def_id);

        let mut checker = checker_with_signatures_collected(&hir, &nameres);

        DiagCtx::clear();
        checker.check_stmt(stmt_id);
        let diagnostics = DiagCtx::diagnostics();
        assert!(diagnostics.is_empty(), "{diagnostics:?}");
    }

    #[test]
    fn return_stmt_rejects_a_value_not_matching_the_return_type() {
        let (hir, nameres) = resolve_src("fun f() -> i32 { return true; }");
        let def_id = first_function(&hir);
        let (stmt_id, _expr_id) = find_return(&hir, def_id);

        let mut checker = checker_with_signatures_collected(&hir, &nameres);

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
        let (hir, nameres) = resolve_src("fun f() { return true; }");
        let def_id = first_function(&hir);
        let (stmt_id, _expr_id) = find_return(&hir, def_id);

        let mut checker = checker_with_signatures_collected(&hir, &nameres);

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
        let (hir, nameres) = resolve_src(
            "fun f() -> bool { return true; }
             fun g() -> i32 { return 1; }",
        );
        let mut checker = checker_with_signatures_collected(&hir, &nameres);

        DiagCtx::clear();
        checker.check_module(hir.root_id());
        let diagnostics = DiagCtx::diagnostics();
        assert!(diagnostics.is_empty(), "{diagnostics:?}");
    }

    // -----------------------------------------------------------------
    // check_expr
    // -----------------------------------------------------------------

    /// `ty_of` records on first use and reads through the unifier afterwards, so a type read
    /// back after it has been unified with something concrete comes back as the concrete type
    /// rather than the variable that was originally recorded.
    #[test]
    fn a_type_read_back_after_unification_is_the_unified_type() {
        let (hir, nameres) = resolve_src("fun f() -> i32 { return 1; }");
        let def_id = first_function(&hir);
        let (_stmt_id, expr_id) = find_return(&hir, def_id);
        let mut checker = checker_with_signatures_collected(&hir, &nameres);

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
        let (hir, nameres) = resolve_src("fun f() -> i32 { return 1; }");
        let def_id = first_function(&hir);
        let (_stmt_id, expr_id) = find_return(&hir, def_id);
        let mut checker = checker_with_signatures_collected(&hir, &nameres);

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
        let (hir, nameres) = resolve_src("fun f() -> bool { return true; }");
        let def_id = first_function(&hir);
        let (_stmt_id, expr_id) = find_return(&hir, def_id);
        let mut checker = checker_with_signatures_collected(&hir, &nameres);

        let ty = checker.ty_of(expr_id);
        assert_eq!(*checker.tcx.kind(ty), TyKind::Primitive(PrimTy::Bool));
        assert_eq!(checker.types.ty(expr_id), Some(ty));
    }

    #[test]
    fn char_literal_checks_to_the_char_primitive() {
        let (hir, nameres) = resolve_src("fun f() -> char { return 'a'; }");
        let def_id = first_function(&hir);
        let (_stmt_id, expr_id) = find_return(&hir, def_id);
        let mut checker = checker_with_signatures_collected(&hir, &nameres);

        let ty = checker.ty_of(expr_id);
        assert_eq!(*checker.tcx.kind(ty), TyKind::Primitive(PrimTy::Char));
    }

    #[test]
    fn unsuffixed_int_literal_checks_to_an_int_inference_var() {
        let (hir, nameres) = resolve_src("fun f() -> i32 { return 0; }");
        let def_id = first_function(&hir);
        let (_stmt_id, expr_id) = find_return(&hir, def_id);
        let mut checker = checker_with_signatures_collected(&hir, &nameres);

        let ty = checker.ty_of(expr_id);
        assert!(matches!(
            checker.tcx.kind(ty),
            TyKind::Var(crate::typeck::ty::TyVar::Int(_))
        ));
    }

    #[test]
    fn unsuffixed_float_literal_checks_to_a_float_inference_var() {
        let (hir, nameres) = resolve_src("fun f() -> f64 { return 0.0; }");
        let def_id = first_function(&hir);
        let (_stmt_id, expr_id) = find_return(&hir, def_id);
        let mut checker = checker_with_signatures_collected(&hir, &nameres);

        let ty = checker.ty_of(expr_id);
        assert!(matches!(
            checker.tcx.kind(ty),
            TyKind::Var(crate::typeck::ty::TyVar::Float(_))
        ));
    }

    #[test]
    fn tuple_expr_checks_to_a_tuple_of_its_elements_types() {
        let (hir, nameres) = resolve_src("fun f() -> (bool, char) { return (true, 'a'); }");
        let def_id = first_function(&hir);
        let (_stmt_id, expr_id) = find_return(&hir, def_id);
        let mut checker = checker_with_signatures_collected(&hir, &nameres);

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
        let (hir, nameres) = resolve_src("fun f(x: i32) -> i32 { return x; }");
        let def_id = first_function(&hir);
        let (_stmt_id, expr_id) = find_return(&hir, def_id);
        let mut checker = checker_with_signatures_collected(&hir, &nameres);

        let ty = checker.ty_of(expr_id);
        assert_eq!(*checker.tcx.kind(ty), TyKind::Primitive(PrimTy::I32));
    }

    #[test]
    fn path_to_a_function_checks_to_its_signature() {
        let (hir, nameres) = resolve_src(
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
        let mut checker = checker_with_signatures_collected(&hir, &nameres);

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
        let (hir, nameres) = resolve_src(
            "struct S {}
             extend S { fun m(&self) -> i32 { return self; } }",
        );
        let method_def = first_extend_method(&hir);
        let (_stmt_id, expr_id) = find_return(&hir, method_def);
        let mut checker = checker_with_signatures_collected(&hir, &nameres);

        let ty = checker.ty_of(expr_id);
        assert_eq!(checker.cx().show(ty).to_string(), "&S");
    }

    // -----------------------------------------------------------------
    // Diagnostic rendering
    // -----------------------------------------------------------------

    #[test]
    fn primitive_displays_as_its_keyword() {
        let (hir, nameres) = resolve_src("fun f() {}");
        let mut checker = checker_with_signatures_collected(&hir, &nameres);
        let ty = checker.tcx.mk_prim(PrimTy::I32);
        assert_eq!(checker.cx().show(ty).to_string(), "i32");
    }

    #[test]
    fn any_ty_var_displays_as_underscore() {
        let (hir, nameres) = resolve_src("fun f() {}");
        let mut checker = checker_with_signatures_collected(&hir, &nameres);
        let ty = checker.tcx.next_ty_var();
        assert_eq!(checker.cx().show(ty).to_string(), "_");
    }

    #[test]
    fn int_var_displays_as_integer_placeholder() {
        let (hir, nameres) = resolve_src("fun f() {}");
        let mut checker = checker_with_signatures_collected(&hir, &nameres);
        let ty = checker.tcx.next_int_var();
        assert_eq!(checker.cx().show(ty).to_string(), "{integer}");
    }

    #[test]
    fn float_var_displays_as_float_placeholder() {
        let (hir, nameres) = resolve_src("fun f() {}");
        let mut checker = checker_with_signatures_collected(&hir, &nameres);
        let ty = checker.tcx.next_float_var();
        assert_eq!(checker.cx().show(ty).to_string(), "{float}");
    }

    #[test]
    fn never_displays_as_bang() {
        let (hir, nameres) = resolve_src("fun f() {}");
        let checker = checker_with_signatures_collected(&hir, &nameres);
        assert_eq!(checker.cx().show(checker.tcx.never()).to_string(), "!");
    }

    #[test]
    fn unit_displays_as_empty_parens() {
        let (hir, nameres) = resolve_src("fun f() {}");
        let checker = checker_with_signatures_collected(&hir, &nameres);
        assert_eq!(checker.cx().show(checker.tcx.unit()).to_string(), "()");
    }

    #[test]
    fn unit_is_a_singleton_distinct_from_the_empty_tuple() {
        let (hir, nameres) = resolve_src("fun f() {}");
        let mut checker = checker_with_signatures_collected(&hir, &nameres);
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
        let (hir, nameres) = resolve_src("fun f() {}");
        let checker = checker_with_signatures_collected(&hir, &nameres);
        assert_eq!(
            checker.cx().show(checker.tcx.error()).to_string(),
            "{error}"
        );
    }

    #[test]
    fn immutable_ref_displays_with_ampersand() {
        let (hir, nameres) = resolve_src("fun f() {}");
        let mut checker = checker_with_signatures_collected(&hir, &nameres);
        let bool_ty = checker.tcx.mk_prim(PrimTy::Bool);
        let ty = checker.tcx.mk_ref(bool_ty, Mutability::Immutable);
        assert_eq!(checker.cx().show(ty).to_string(), "&bool");
    }

    #[test]
    fn mutable_ref_displays_with_ampersand_mut() {
        let (hir, nameres) = resolve_src("fun f() {}");
        let mut checker = checker_with_signatures_collected(&hir, &nameres);
        let bool_ty = checker.tcx.mk_prim(PrimTy::Bool);
        let ty = checker.tcx.mk_ref(bool_ty, Mutability::Mutable);
        assert_eq!(checker.cx().show(ty).to_string(), "&mut bool");
    }

    #[test]
    fn any_ty_displays_with_any_keyword() {
        let (hir, nameres) = resolve_src("fun f() {}");
        let mut checker = checker_with_signatures_collected(&hir, &nameres);
        let bool_ty = checker.tcx.mk_prim(PrimTy::Bool);
        let ty = checker.tcx.mk_any(bool_ty);
        assert_eq!(checker.cx().show(ty).to_string(), "any bool");
    }

    #[test]
    fn empty_tuple_displays_as_empty_parens() {
        let (hir, nameres) = resolve_src("fun f() {}");
        let mut checker = checker_with_signatures_collected(&hir, &nameres);
        let ty = checker.tcx.mk_tuple(vec![]);
        assert_eq!(checker.cx().show(ty).to_string(), "()");
    }

    #[test]
    fn one_element_tuple_displays_with_a_trailing_comma() {
        let (hir, nameres) = resolve_src("fun f() {}");
        let mut checker = checker_with_signatures_collected(&hir, &nameres);
        let bool_ty = checker.tcx.mk_prim(PrimTy::Bool);
        let ty = checker.tcx.mk_tuple(vec![bool_ty]);
        assert_eq!(checker.cx().show(ty).to_string(), "(bool,)");
    }

    #[test]
    fn multi_element_tuple_displays_comma_separated() {
        let (hir, nameres) = resolve_src("fun f() {}");
        let mut checker = checker_with_signatures_collected(&hir, &nameres);
        let bool_ty = checker.tcx.mk_prim(PrimTy::Bool);
        let char_ty = checker.tcx.mk_prim(PrimTy::Char);
        let ty = checker.tcx.mk_tuple(vec![bool_ty, char_ty]);
        assert_eq!(checker.cx().show(ty).to_string(), "(bool, char)");
    }

    #[test]
    fn array_displays_with_brackets_and_a_placeholder_length() {
        let (hir, nameres) = resolve_src("fun f() {}");
        let mut checker = checker_with_signatures_collected(&hir, &nameres);
        let i32_ty = checker.tcx.mk_prim(PrimTy::I32);
        let ty = checker.tcx.mk_array(i32_ty, None);
        assert_eq!(checker.cx().show(ty).to_string(), "[i32; _]");
    }

    #[test]
    fn fun_with_no_params_or_ret_displays_as_bare_fun() {
        let (hir, nameres) = resolve_src("fun f() {}");
        let mut checker = checker_with_signatures_collected(&hir, &nameres);
        let ty = checker.tcx.mk_fun(vec![], None);
        assert_eq!(checker.cx().show(ty).to_string(), "fun()");
    }

    #[test]
    fn fun_with_params_and_ret_displays_with_arrow() {
        let (hir, nameres) = resolve_src("fun f() {}");
        let mut checker = checker_with_signatures_collected(&hir, &nameres);
        let i32_ty = checker.tcx.mk_prim(PrimTy::I32);
        let bool_ty = checker.tcx.mk_prim(PrimTy::Bool);
        let ty = checker.tcx.mk_fun(vec![i32_ty, i32_ty], Some(bool_ty));
        assert_eq!(checker.cx().show(ty).to_string(), "fun(i32, i32) -> bool");
    }

    #[test]
    fn generic_displays_with_its_declared_name() {
        let (hir, nameres) = resolve_src("struct Wrap<T> { inner: T }");
        let def_id = first_struct(&hir);
        let OwnerNode::Struct(s) = hir.def(def_id) else {
            unreachable!("first_struct only returns a struct's DefId");
        };
        let generic_id = s.generics[0];
        let mut checker = checker_with_signatures_collected(&hir, &nameres);
        let ty = checker.tcx.mk_generic(generic_id);
        assert_eq!(checker.cx().show(ty).to_string(), "T");
    }

    #[test]
    fn adt_displays_with_its_name_and_generic_args() {
        let (hir, nameres) = resolve_src("struct Wrap<T> { inner: T }");
        let def_id = first_struct(&hir);
        let checker = checker_with_signatures_collected(&hir, &nameres);

        let ty = checker
            .types
            .ty_of_def(def_id)
            .expect("collect_struct records the struct's own type under its owner node");
        assert_eq!(checker.cx().show(ty).to_string(), "Wrap<T>");
    }

    #[test]
    fn adt_with_no_generics_displays_with_just_its_name() {
        let (hir, nameres) = resolve_src("struct Unit {}");
        let def_id = first_struct(&hir);
        let checker = checker_with_signatures_collected(&hir, &nameres);

        let ty = checker
            .types
            .ty_of_def(def_id)
            .expect("collect_struct records the struct's own type under its owner node");
        assert_eq!(checker.cx().show(ty).to_string(), "Unit");
    }

    #[test]
    fn self_param_displays_as_self() {
        let (hir, nameres) = resolve_src("trait Greet { fun hello(); }");
        let def_id = first_trait(&hir);
        let checker = checker_with_signatures_collected(&hir, &nameres);

        let ty = checker
            .types
            .ty_of_def(def_id)
            .expect("collect_trait records the trait's own Self type under its owner node");
        assert_eq!(checker.cx().show(ty).to_string(), "Self");
    }

    #[test]
    fn dyn_displays_with_dyn_keyword_and_trait_name() {
        let (hir, nameres) = resolve_src("trait Greet { fun hello(); }");
        let def_id = first_trait(&hir);
        let mut checker = checker_with_signatures_collected(&hir, &nameres);
        let ty = checker.tcx.mk_dyn(def_id, vec![]);
        assert_eq!(checker.cx().show(ty).to_string(), "dyn Greet");
    }

    /// A `UnifyError` renders itself, so the wording lives next to the variant it explains
    /// rather than in a `match` somewhere in the checker.
    #[test]
    fn a_mismatch_names_both_types_as_the_user_wrote_them() {
        let (hir, _nameres) = resolve_src("fun f() {}");
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
        let (hir, _nameres) = resolve_src("fun f() {}");
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
