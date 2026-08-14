//! Method resolution: turning `x.foo()` into one particular function, and `x.foo` into one
//! particular field.
//!
//! [`ExprKind::Access`](crate::hir::ExprKind::Access) is the node the parser produces for
//! everything written with a `.`, deliberately undisambiguated: a field, a method call, and an
//! enum variant named through its type all land in it, because telling them apart needs types.
//! This module is where the types finally exist. [`ExprKind::Call`](crate::hir::ExprKind::Call)
//! is checked here too, for the one thing the two share -- an argument list measured against a
//! signature -- and because a call to a generic callee is the registration site
//! [`bounds`](crate::typeck::traits::bounds) left open.
//!
//! ## Why a method call is not a lookup
//!
//! A name written after a `.` may be reachable in four different ways, and three of them are
//! questions for the solver rather than for a table:
//!
//! - an **inherent** `extend Foo { .. }` block defines it outright;
//! - an **impl** of a trait for `Foo` provides it, or inherits it from the trait's default body;
//! - a **bound in scope** promises it, which is the only thing that can answer for a receiver
//!   whose type is a bare type parameter -- `fun f<T: Show>(x: T) { x.show() }` has no `Foo` for
//!   the index to look up at all;
//! - a **`dyn Show`** receiver carries exactly the methods `Show` declares.
//!
//! Each of the four also answers with a different *vocabulary* for the signature it found, which
//! is what [`Typeck::instantiate_method`] exists to reconcile: a block's own method is written in
//! the block's terms, a trait's declaration in the trait's, and only substituting each through
//! what made it apply puts the signature in the caller's terms.
//!
//! ## Picking
//!
//! An inherent method (defined in an `extend Foo { .. }` block without a trait) takes precedence
//! over any trait method: because the block targets a specific type, it is more specific than any
//! trait's method that it shadows. More than one surviving *trait* candidate indicates an
//! ambiguity: the receiver could dispatch to multiple methods with no clear priority. Coherence
//! rules out two impls for the same type providing the same method, so multiple candidates only
//! arise across independent trait bounds in a [`ParamEnv`](crate::typeck::traits::solve::ParamEnv)
//! -- for example, `fun f<T: A + B>` where both trait `A` and trait `B` declare a `size` method.
//! Coherence cannot detect this overlap because neither `A` nor `B` is implemented for a concrete
//! type; the conflict is only visible when both are bounds on the same parameter.
//!
//! ## Receivers
//!
//! The receiver's type is peeled down to the type the candidates were collected for, and how many
//! layers of `&`/`any` came off *is* the receiver adjustment: the call has to reach the form the
//! method's [`SelfMode`] asks for, by dereferencing what was peeled or by taking a reference to a
//! place. Nothing downstream consumes the adjustment yet -- there is no lowering to consume it --
//! so what the depth is used for here is deciding whether the call is legal at all.
//!
//! ## Not deferred
//!
//! Unlike a bound, an unresolved receiver is reported on the spot rather than retried later. A
//! bound is a side condition, so postponing it costs nothing; a method call's *result type* feeds
//! everything around it, and there is no answer to give the surrounding expression while the
//! receiver is unknown.

use std::collections::{HashMap, HashSet};

use crate::ast::interner::Interner;
use crate::ast::{Ident, Mutability, SelfMode, Symbol};
use crate::diagnostics::typeck::traits::method::{
    function_name_span, report_ambiguous_method, report_call_arg_count, report_call_arg_mismatch,
    report_field_is_a_method, report_no_field, report_no_method, report_no_receiver,
    report_not_callable, report_receiver_mode, report_receiver_not_a_place,
    report_receiver_unknown,
};
use crate::diagnostics::typeck::traits::trait_name;
use crate::driver::source::SrcSpan;
use crate::hir::{AccessArgs, DefId, ExprKind, HirId, Node, OwnerNode, Res};
use crate::typeck::Typeck;
use crate::typeck::traits::index::ImplId;
use crate::typeck::traits::solve::match_ty;
use crate::typeck::ty::{Ty, TyKind};

/// One way the member being called could be reached, and everything it takes to read that
/// method's signature in the caller's terms.
#[derive(Clone, Debug)]
pub(crate) struct Candidate {
    /// The function that would run. For a trait method the block wrote out, this is the block's
    /// method; for one it inherited, the trait's own declaration and its default body.
    method: DefId,

    source: CandidateSource,

    /// What `Self` stands for in `method`'s signature. Always the peeled receiver type: a trait
    /// declaration is written in terms of `Self`, and the type that reached it is what `Self` is.
    self_ty: Ty,

    /// What the parameters `method`'s signature is written in terms of stand for -- an impl's own
    /// `<T>` group, or a trait's. The method's *own* parameters are not in here; those are
    /// instantiated fresh per call site, in [`Typeck::instantiate_method`].
    subst: HashMap<HirId, Ty>,
}

/// Where a candidate came from. Only two cases, because only two things are decided by it:
/// whether the candidate outranks the others, and which trait to name if it does not.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum CandidateSource {
    /// An `extend Foo { .. }` block with no trait.
    Inherent,
    /// A trait, however the receiver reaches it -- through an impl, a bound in scope, or a `dyn`.
    /// The three are one case here: they differ in how the method was *found*, and not at all in
    /// how it is picked between or reported.
    Trait(DefId),
}

/// One layer peeled off a receiver on the way to the type its methods live on.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) enum Layer {
    Ref(Mutability),
    Any,
}

impl<'hir> Typeck<'hir> {
    // -----------------------------------------------------------------
    // The two expression forms
    // -----------------------------------------------------------------

    /// Checks `base.member`, in whichever of its readings applies.
    ///
    /// `base.member` where `base` names an enum (`Shape.circle`, an enum variant reached through
    /// its type) is **not** disambiguated here, nor by name resolution: the design deliberately
    /// gives `Res` no `Variant` arm, since which enum a variant belongs to is only knowable once
    /// the expected type is (see `hir::path::Res`'s docs and `hir::path::Local`'s). Nothing in
    /// this pass currently fills that gap either -- `base` is checked as an ordinary expression
    /// below, which for `Shape.circle` means a value-position lookup on `Shape` that name
    /// resolution cannot satisfy (`Res::Err`), so `base`'s type comes out `Error` and every arm
    /// below short-circuits on it silently. Recognizing an enum-named base and dispatching to
    /// variant construction is real, missing functionality, not something the AST-level resolver
    /// took away: the old HIR-based resolver's `resolve_access` used to settle
    /// this before typeck ran, keyed by this very `id`, but nothing downstream of it actually
    /// built a variant expression from the answer -- the fast path below just routed straight to
    /// `todo!()`.
    pub(crate) fn check_access(
        &mut self,
        _id: HirId,
        base: HirId,
        member: Ident,
        args: &AccessArgs,
        span: SrcSpan,
    ) -> Ty {
        match args {
            AccessArgs::Call(args) => self.check_method_call(base, member, args, span),
            AccessArgs::None => self.check_field(base, member),
            // A record payload is only ever written on a variant, and every access name
            // resolution could read as one was taken above. What is left is a record payload on
            // something that is not an enum, which is the variant checking this phase does not
            // do.
            AccessArgs::Record(_) => todo!("check_expr: Access (variant with a record payload)"),
        }
    }

    /// Checks `callee(args)`.
    ///
    /// The callee is an ordinary expression, so what makes this a call rather than a lookup is
    /// only that its type has to be a function type. A path naming a generic function is the
    /// exception, and the reason [`Typeck::callee_sig`] exists: its declared parameters have to be
    /// instantiated before the arguments can be checked against anything.
    pub(crate) fn check_call(&mut self, callee: HirId, args: &[HirId], span: SrcSpan) -> Ty {
        let (sig, instantiation) = self.callee_sig(callee);

        let TyKind::Fun { params, ret } = self.tcx.kind(sig).clone() else {
            // Every argument is still checked, so that a mistake in one of them is reported
            // alongside this rather than only after it has been fixed.
            for &arg in args {
                self.ty_of(arg);
            }
            if !matches!(self.tcx.kind(sig), TyKind::Error) {
                report_not_callable(self.cx(), sig, span);
            }
            return self.tcx.error();
        };

        self.check_args(&params, args, "this call", span);
        if let Some((def, generic_args)) = instantiation {
            self.register_instantiation(def, &generic_args, span, callee.owner);
        }
        ret.unwrap_or_else(|| self.tcx.unit())
    }

    /// The signature `callee` is being called at, with a generic callee's own parameters replaced
    /// by fresh inference variables, and -- for a generic callee -- what it was instantiated with.
    ///
    /// The instantiation is handed back rather than registered here, because the bound it raises
    /// reads far better once the argument list has had its say; see
    /// [`register_instantiation`](Typeck::register_instantiation).
    fn callee_sig(&mut self, callee: HirId) -> (Ty, Option<(DefId, Vec<Ty>)>) {
        let Node::Expr(expr) = self.hir.node(callee) else {
            unreachable!("a call's callee always names an expression");
        };

        // Anything but a path -- a closure, a field holding a function, a parenthesized
        // expression -- has no declaration behind it to instantiate, so its type is already the
        // signature.
        let ExprKind::Path(path) = &expr.kind else {
            return (self.ty_of(callee), None);
        };
        let Res::Function(def) = path.res else {
            return (self.ty_of(callee), None);
        };
        let OwnerNode::Function(function) = self.hir.def(def) else {
            return (self.ty_of(callee), None);
        };
        let generics = function.generics.clone();
        if generics.is_empty() {
            return (self.ty_of(callee), None);
        }

        let sig = self
            .recorded_ty_of_def(def)
            .expect("collect_function records every function's own signature");
        let mut subst = HashMap::new();
        let mut args = Vec::with_capacity(generics.len());
        for param in generics {
            let var = self.tcx.next_ty_var();
            subst.insert(param, var);
            args.push(var);
        }
        let sig = self.subst_ty(sig, &subst);

        // Recorded rather than left to `ty_of`, so that the type the table holds for this node is
        // the one the call was actually checked against.
        self.types.record(callee, sig);
        (sig, Some((def, args)))
    }

    /// Registers what `def`'s declared bounds demand of the arguments it was instantiated at.
    ///
    /// This is the fourth bound registration site, the one
    /// [`register_bound_obligations`](Typeck::register_bound_obligations) was written for and left
    /// open: instantiating `fun sort<T: Comparable>(..)` at a call site is as much an
    /// instantiation as writing `Sorted<Foo>` in an annotation, and the bound has to hold of
    /// whatever the argument turned out to be.
    ///
    /// Which is why the arguments are resolved first. A call site's type parameters are chosen by
    /// the argument list, so by the time the list has been checked they are usually settled, and
    /// registering the resolved form is what lets the failure say `Bare: Show` instead of
    /// `_: Show`. One that is *not* settled stays a variable and so stays
    /// [`Ambiguous`](crate::typeck::traits::solve::Solution::Ambiguous), which is the deferral
    /// this design already has an answer for.
    fn register_instantiation(&mut self, def: DefId, args: &[Ty], cause: SrcSpan, owner: DefId) {
        let args: Vec<Ty> = args.iter().map(|&arg| self.resolve_deep(arg)).collect();
        self.register_bound_obligations(def, &args, cause, owner);
    }

    // -----------------------------------------------------------------
    // Method calls
    // -----------------------------------------------------------------

    /// Checks `receiver.member(args)`.
    pub(crate) fn check_method_call(
        &mut self,
        receiver: HirId,
        member: Ident,
        args: &[HirId],
        span: SrcSpan,
    ) -> Ty {
        let owner = receiver.owner;
        let receiver_ty = self.ty_of(receiver);
        let receiver_ty = self.resolve_deep(receiver_ty);

        // Step 1. Not deferred; see the [module docs](self).
        if matches!(self.tcx.kind(receiver_ty), TyKind::Var(_)) {
            report_receiver_unknown(member, self.hir.expr(receiver).span);
            return self.check_args_only(args);
        }
        if matches!(self.tcx.kind(receiver_ty), TyKind::Error) {
            return self.check_args_only(args);
        }

        // Step 2.
        let (base, layers) = self.peel_receiver(receiver_ty);

        // Step 3.
        let candidates = self.method_candidates(base, member.text, owner);
        if candidates.is_empty() {
            report_no_method(self.cx(), member, base);
            return self.check_args_only(args);
        }

        // Step 4.
        let Some(chosen) = self.pick(&candidates, member) else {
            return self.check_args_only(args);
        };

        // Step 5.
        self.check_chosen_method(&chosen, receiver, &layers, member, args, span)
    }

    /// Instantiates the chosen method and checks the call against it.
    fn check_chosen_method(
        &mut self,
        chosen: &Candidate,
        receiver: HirId,
        layers: &[Layer],
        member: Ident,
        args: &[HirId],
        span: SrcSpan,
    ) -> Ty {
        let mode = self.receiver_mode(chosen.method);
        let (params, ret, fresh) =
            self.instantiate_method(chosen.method, &chosen.subst, chosen.self_ty);

        match mode {
            Some(mode) => self.check_receiver(mode, receiver, layers, member, chosen.method),
            None => report_no_receiver(self.hir, member, chosen.method),
        }

        // A method's `self` counts as its first parameter -- see
        // [`collect_function`](Typeck::collect_function) -- and it was just checked separately, so
        // the written arguments start after it.
        let expected = params[usize::from(mode.is_some())..].to_vec();
        let name = format!("`{}`", Interner::resolve(member.text));
        self.check_args(&expected, args, &name, span);

        // A method may declare parameters of its own, and calling it instantiates them exactly as
        // calling a free function does -- after the arguments, for the same reason.
        self.register_instantiation(chosen.method, &fresh, span, receiver.owner);

        ret.unwrap_or_else(|| self.tcx.unit())
    }

    /// Every way `member` could be reached on `base`, in the order the design lists them.
    ///
    /// Deduplicated by the function each one would call, which is what keeps `T: Show + Show` --
    /// or a bound that also happens to be provable from an impl -- from reading as an ambiguity
    /// between a candidate and itself.
    pub(crate) fn method_candidates(
        &mut self,
        base: Ty,
        member: Symbol,
        owner: DefId,
    ) -> Vec<Candidate> {
        let mut candidates = Vec::new();

        // Inherent blocks and impls, both keyed on the head of the self type.
        if let TyKind::Adt { def, .. } = *self.tcx.kind(base) {
            for impl_id in self.impls.for_self(def).to_vec() {
                if let Some(candidate) = self.impl_candidate(impl_id, base, member) {
                    candidates.push(candidate);
                }
            }
        }

        // A `dyn Show` value implements exactly `Show`, so it offers exactly what `Show`
        // declares. There is no impl behind it -- impls are nominal -- so this is a rule here,
        // the same way it is a rule in the query.
        if let TyKind::Dyn { trait_, args } = self.tcx.kind(base).clone()
            && let Some(method) = self.trait_method(trait_, member)
        {
            let subst = self.trait_subst(trait_, &args);
            candidates.push(Candidate {
                method,
                source: CandidateSource::Trait(trait_),
                self_ty: base,
                subst,
            });
        }

        // The environment. This is the only step that can answer for a receiver whose type is a
        // bare parameter, since nothing about `T` is in the index.
        for bound in self.param_env(owner).bounds {
            if bound.self_ty != base {
                continue;
            }
            let Some(method) = self.trait_method(bound.trait_ref.def, member) else {
                continue;
            };
            let subst = self.trait_subst(bound.trait_ref.def, &bound.trait_ref.args);
            candidates.push(Candidate {
                method,
                source: CandidateSource::Trait(bound.trait_ref.def),
                self_ty: base,
                subst,
            });
        }

        let mut seen = HashSet::new();
        candidates.retain(|candidate| seen.insert(candidate.method));
        candidates
    }

    /// Whether one `extend` block offers `member` for `base`, and under what substitution.
    ///
    /// The header is an open term, so applying to `base` is a match rather than a comparison --
    /// the same one-way [`match_ty`] the query uses, and for the same reason: it must not bind
    /// anything in the receiver's type while merely considering a candidate.
    fn impl_candidate(&mut self, impl_id: ImplId, base: Ty, member: Symbol) -> Option<Candidate> {
        let header = self.impls.header(impl_id);
        let (generics, self_ty, trait_ref) = (
            header.generics.clone(),
            header.self_ty,
            header.trait_ref.clone(),
        );
        let provided = header.methods.get(&member).copied();

        let mut subst = HashMap::new();
        if !match_ty(&self.tcx, &generics, self_ty, base, &mut subst) {
            return None;
        }

        let Some(trait_ref) = trait_ref else {
            // An inherent block's methods are its own list: there is no declaration elsewhere for
            // one to be missing from, and its signature is already written in the block's terms.
            return provided.map(|method| Candidate {
                method,
                source: CandidateSource::Inherent,
                self_ty: base,
                subst,
            });
        };

        // What the *trait* declares is what the type ends up with, which is not the same as what
        // the block wrote out: a defaulted method is available without appearing in the block, and
        // a method the trait never declared is not available at all -- `check_trait_members` has
        // already reported that one, and leaving it unreachable is what keeps a call to it from
        // being checked against a signature no trait promised.
        let declared = self.trait_method(trait_ref.def, member)?;
        let source = CandidateSource::Trait(trait_ref.def);

        match provided {
            // Written out by the block, so its signature is already phrased in the block's own
            // terms and the impl substitution is the whole of what it needs.
            Some(method) => Some(Candidate {
                method,
                source,
                self_ty: base,
                subst,
            }),
            // Inherited: the trait's declaration, phrased in the trait's vocabulary. The block's
            // arguments to the trait are what its parameters stand for -- carried through the
            // impl substitution first, since they may mention the block's own parameters.
            None => {
                let args: Vec<Ty> = trait_ref
                    .args
                    .iter()
                    .map(|&arg| self.subst_ty(arg, &subst))
                    .collect();
                let subst = self.trait_subst(trait_ref.def, &args);
                Some(Candidate {
                    method: declared,
                    source,
                    self_ty: base,
                    subst,
                })
            }
        }
    }

    /// The one candidate the call resolves to, or `None` after reporting why there isn't one.
    ///
    /// Never called with an empty list: "no method at all" is a different diagnostic, which names
    /// the type rather than the candidates.
    fn pick(&self, candidates: &[Candidate], member: Ident) -> Option<Candidate> {
        if let Some(inherent) = candidates
            .iter()
            .find(|candidate| candidate.source == CandidateSource::Inherent)
        {
            return Some(inherent.clone());
        }

        match candidates {
            [only] => Some(only.clone()),
            [] => unreachable!("pick is only asked about a non-empty candidate list"),
            many => {
                let candidates: Vec<(&str, SrcSpan)> = many
                    .iter()
                    .map(|candidate| {
                        let CandidateSource::Trait(def) = candidate.source else {
                            unreachable!(
                                "an inherent candidate wins outright, so it is never ambiguous"
                            );
                        };
                        (
                            trait_name(self.hir, def),
                            function_name_span(self.hir, candidate.method),
                        )
                    })
                    .collect();
                report_ambiguous_method(member, &candidates);
                None
            }
        }
    }

    // -----------------------------------------------------------------
    // Signatures
    // -----------------------------------------------------------------

    /// `method`'s signature in the caller's terms: parameter types, return type, and the fresh
    /// variables its own type parameters were instantiated at.
    ///
    /// Three substitutions happen at once, because a signature may mention all three at the same
    /// time. `subst` carries whatever declared the method -- an impl's `<T>` group or a trait's --
    /// `self_ty` carries `Self`, and the method's own `<U>` group becomes fresh inference
    /// variables, since each call site chooses them independently.
    fn instantiate_method(
        &mut self,
        method: DefId,
        subst: &HashMap<HirId, Ty>,
        self_ty: Ty,
    ) -> (Vec<Ty>, Option<Ty>, Vec<Ty>) {
        let OwnerNode::Function(function) = self.hir.def(method) else {
            unreachable!("a candidate's method is always a function");
        };
        let generics = function.generics.clone();

        let mut subst = subst.clone();
        let mut fresh = Vec::with_capacity(generics.len());
        for param in generics {
            let var = self.tcx.next_ty_var();
            subst.insert(param, var);
            fresh.push(var);
        }

        let sig = self
            .types
            .ty_of_def(method)
            .expect("collect_function records every method's own signature");
        let TyKind::Fun { params, ret } = self.tcx.kind(sig).clone() else {
            unreachable!("a function's own signature always lowers to TyKind::Fun");
        };

        let params = params
            .into_iter()
            .map(|ty| self.subst_sig_ty(ty, &subst, self_ty))
            .collect();
        let ret = ret.map(|ty| self.subst_sig_ty(ty, &subst, self_ty));
        (params, ret, fresh)
    }

    /// Rebuilds `ty` with every parameter in `subst` and every `Self` replaced at once.
    ///
    /// One walk rather than [`subst_ty`](Typeck::subst_ty) followed by a second pass for `Self`,
    /// because instantiating a signature always means both: a trait declares `fun get(&self, key:
    /// K) -> Self`, and reading that at a call site substitutes `K` and `Self` in the same breath.
    /// [`subst_ty`](Typeck::subst_ty) deliberately leaves `SelfTy` alone, which is right
    /// everywhere it is used -- inside an `extend` block `Self` is already concrete -- and wrong
    /// only here, where the declaration being read is the trait's own.
    fn subst_sig_ty(&mut self, ty: Ty, subst: &HashMap<HirId, Ty>, self_ty: Ty) -> Ty {
        match self.tcx.kind(ty).clone() {
            TyKind::Generic(param) => subst.get(&param).copied().unwrap_or(ty),
            TyKind::SelfTy(_) => self_ty,
            TyKind::Adt { def, args } => {
                let args = self.subst_sig_tys(&args, subst, self_ty);
                self.tcx.mk_adt(def, args)
            }
            TyKind::Dyn { trait_, args } => {
                let args = self.subst_sig_tys(&args, subst, self_ty);
                self.tcx.mk_dyn(trait_, args)
            }
            TyKind::Tuple(elems) => {
                let elems = self.subst_sig_tys(&elems, subst, self_ty);
                self.tcx.mk_tuple(elems)
            }
            TyKind::Ref { base, mutability } => {
                let base = self.subst_sig_ty(base, subst, self_ty);
                self.tcx.mk_ref(base, mutability)
            }
            TyKind::Any(base) => {
                let base = self.subst_sig_ty(base, subst, self_ty);
                self.tcx.mk_any(base)
            }
            TyKind::Array { elem, len } => {
                let elem = self.subst_sig_ty(elem, subst, self_ty);
                self.tcx.mk_array(elem, len)
            }
            TyKind::Fun { params, ret } => {
                let params = self.subst_sig_tys(&params, subst, self_ty);
                let ret = ret.map(|ret| self.subst_sig_ty(ret, subst, self_ty));
                self.tcx.mk_fun(params, ret)
            }
            // Nothing to substitute into.
            TyKind::Var(_)
            | TyKind::Primitive(_)
            | TyKind::Unit
            | TyKind::Never
            | TyKind::Error => ty,
        }
    }

    fn subst_sig_tys(&mut self, tys: &[Ty], subst: &HashMap<HirId, Ty>, self_ty: Ty) -> Vec<Ty> {
        tys.iter()
            .map(|&ty| self.subst_sig_ty(ty, subst, self_ty))
            .collect()
    }

    /// What a trait's own parameters stand for, given the arguments it was applied to.
    ///
    /// A wrong argument count is reported where the arguments were written -- by
    /// [`check_impl_headers`](Typeck::check_impl_headers) or by
    /// [`lower_ty`](Typeck::lower_ty) -- so the pairs that do line up are taken and the rest left
    /// out, rather than reporting the same count twice.
    fn trait_subst(&self, trait_def: DefId, args: &[Ty]) -> HashMap<HirId, Ty> {
        let OwnerNode::Trait(trait_) = self.hir.def(trait_def) else {
            unreachable!("a TraitRef's def always names a trait; the index is what enforces it");
        };
        trait_
            .generics
            .iter()
            .copied()
            .zip(args.iter().copied())
            .collect()
    }

    /// The method `trait_def` declares under `name`, if it declares one.
    fn trait_method(&self, trait_def: DefId, name: Symbol) -> Option<DefId> {
        let OwnerNode::Trait(trait_) = self.hir.def(trait_def) else {
            unreachable!("a TraitRef's def always names a trait; the index is what enforces it");
        };
        trait_.functions.iter().copied().find(|&function| {
            let OwnerNode::Function(function) = self.hir.def(function) else {
                unreachable!("a trait's `functions` holds only functions");
            };
            function.name.text == name
        })
    }

    /// How `method` takes its receiver, or `None` for an associated function that takes none.
    fn receiver_mode(&self, method: DefId) -> Option<SelfMode> {
        let OwnerNode::Function(function) = self.hir.def(method) else {
            unreachable!("a candidate's method is always a function");
        };
        let Node::SelfParam(self_param) = self.hir.node(function.self_param?) else {
            unreachable!("a function's self param slot always holds a Node::SelfParam");
        };
        Some(self_param.mode)
    }

    // -----------------------------------------------------------------
    // Receivers
    // -----------------------------------------------------------------

    /// Strips the `&`, `&mut` and `any` layers off a receiver to reach the type whose methods are
    /// being looked for, keeping what came off.
    ///
    /// The layers are the receiver adjustment: their count is how many dereferences the call
    /// performs, and the outermost one is what decides whether a `&mut self` method can be reached
    /// through what the caller has.
    pub(crate) fn peel_receiver(&self, ty: Ty) -> (Ty, Vec<Layer>) {
        let mut layers = Vec::new();
        let mut current = ty;
        loop {
            match *self.tcx.kind(current) {
                TyKind::Ref { base, mutability } => {
                    layers.push(Layer::Ref(mutability));
                    current = base;
                }
                TyKind::Any(base) => {
                    layers.push(Layer::Any);
                    current = base;
                }
                _ => return (current, layers),
            }
        }
    }

    /// Checks that what the caller holds can be turned into the receiver `mode` asks for.
    ///
    /// Two adjustments are available, and each mode uses at most one of them. Dereferencing is
    /// what the peeled layers already describe. Taking a reference -- autoref -- is only possible
    /// where there is something to take a reference *to*, which is why a place expression is
    /// required: `make_foo().show()` would have to borrow a temporary that outlives nothing.
    fn check_receiver(
        &mut self,
        mode: SelfMode,
        receiver: HirId,
        layers: &[Layer],
        member: Ident,
        method: DefId,
    ) {
        let span = self.hir.expr(receiver).span;
        match mode {
            // `any self` is exactly the mode that accepts every form of receiver, which is what
            // it was added to the language to say.
            SelfMode::Any => {}

            // Taking `self` by value has to have the value. Dereferencing to get one would move
            // out of a reference, which is the caller's to do explicitly if it is theirs to do at
            // all.
            SelfMode::Move => {
                if !layers.is_empty() {
                    report_receiver_mode(self.hir, member, mode, span, method);
                }
            }

            // Any depth of reference reaches the base, and both `&` and `&mut` yield the shared
            // borrow a `&self` method wants. What is left is the unreferenced case, which needs a
            // place to borrow.
            SelfMode::Immutable => {
                if layers.is_empty() && !self.is_place_expr(receiver) {
                    report_receiver_not_a_place(self.hir, member, mode, span, method);
                }
            }

            // The one mode that cares *which* reference it was handed: a shared borrow cannot
            // become a mutable one.
            SelfMode::Mutable => match layers.first() {
                None => {
                    if !self.is_place_expr(receiver) {
                        report_receiver_not_a_place(self.hir, member, mode, span, method);
                    }
                }
                Some(Layer::Ref(Mutability::Immutable)) => {
                    report_receiver_mode(self.hir, member, mode, span, method);
                }
                Some(Layer::Ref(Mutability::Mutable) | Layer::Any) => {}
            },
        }
    }

    /// Whether `id` names a place -- somewhere a value lives -- rather than a value produced on
    /// the spot.
    ///
    /// Only the forms that certainly do. A call, a literal or an arithmetic expression produces a
    /// temporary; anything not listed here is treated as one, which errs towards reporting rather
    /// than towards silently borrowing something with nowhere to live.
    pub(crate) fn is_place_expr(&self, id: HirId) -> bool {
        match &self.hir.expr(id).kind {
            // A path names a local, a parameter, or `self`.
            ExprKind::Path(_) => true,
            // A field of a place is a place; a method call on one is not.
            ExprKind::Access {
                args: AccessArgs::None,
                ..
            } => true,
            ExprKind::Index { .. } => true,
            _ => false,
        }
    }

    // -----------------------------------------------------------------
    // Fields
    // -----------------------------------------------------------------

    /// Checks `base.member` where no argument list follows: a field access.
    ///
    /// A method of the same name is searched for only to say so. `x.foo` where `foo` is a method
    /// is an error rather than a function value -- see the design's scope -- and the search is
    /// what turns "no field `foo`" into a sentence that says what to do about it.
    fn check_field(&mut self, base: HirId, member: Ident) -> Ty {
        let owner = base.owner;
        let base_ty = self.ty_of(base);
        let base_ty = self.resolve_deep(base_ty);

        if matches!(self.tcx.kind(base_ty), TyKind::Var(_)) {
            report_receiver_unknown(member, self.hir.expr(base).span);
            return self.tcx.error();
        }
        if matches!(self.tcx.kind(base_ty), TyKind::Error) {
            return self.tcx.error();
        }

        // A field is reached through references exactly as a method is.
        let (base_ty, _adjustment) = self.peel_receiver(base_ty);
        if let Some(ty) = self.field_ty(base_ty, member.text) {
            return ty;
        }

        if self
            .method_candidates(base_ty, member.text, owner)
            .is_empty()
        {
            report_no_field(self.cx(), member, base_ty);
        } else {
            report_field_is_a_method(self.cx(), member, base_ty);
        }
        self.tcx.error()
    }

    /// The type of `base`'s field named `member`, if `base` is a struct that has one.
    ///
    /// A field's declared type is written in the struct's own terms, so it is read through the
    /// arguments the receiver's type applied: `inner` of `Wrap<i32>` is `i32`, not `T`.
    fn field_ty(&mut self, base: Ty, member: Symbol) -> Option<Ty> {
        let TyKind::Adt { def, args } = self.tcx.kind(base).clone() else {
            return None;
        };
        // An enum's fields belong to its variants rather than to the enum, so there is nothing
        // here to reach through a `.` on a value of one.
        let OwnerNode::Struct(struct_) = self.hir.def(def) else {
            return None;
        };
        let (fields, generics) = (struct_.fields.clone(), struct_.generics.clone());

        let field = fields
            .into_iter()
            .find(|&id| self.hir.field(id).name.text == member)?;
        let declared = self
            .types
            .ty(field)
            .expect("collect_fields records every field's declared type");

        let subst: HashMap<HirId, Ty> = generics.into_iter().zip(args).collect();
        Some(self.subst_ty(declared, &subst))
    }

    // -----------------------------------------------------------------
    // Argument lists
    // -----------------------------------------------------------------

    /// Checks `args` against the types `expected`, reporting a count mismatch and each argument
    /// that does not fit.
    ///
    /// Every argument is checked whatever the count is, so that the types recorded for them are
    /// complete and a mistake inside one is reported alongside the count rather than only once the
    /// count is fixed. Only the pairs that line up are unified: past the shorter of the two lists
    /// there is nothing to compare against, and pairing them off anyway would report positions the
    /// caller never wrote.
    fn check_args(&mut self, expected: &[Ty], args: &[HirId], name: &str, span: SrcSpan) {
        let found: Vec<Ty> = args.iter().map(|&arg| self.ty_of(arg)).collect();

        if found.len() != expected.len() {
            report_call_arg_count(name, found.len(), expected.len(), span);
        }

        for (index, (&want, &got)) in expected.iter().zip(found.iter()).enumerate() {
            if let Err(err) = self.unifier.unify(&self.tcx, want, got) {
                let span = self.hir.expr(args[index]).span;
                report_call_arg_mismatch(self.cx(), err, span);
            }
        }
    }

    /// Checks the arguments of a call that has already gone wrong, and answers
    /// [`TyKind::Error`](crate::typeck::ty::TyKind::Error).
    ///
    /// The arguments are still expressions with types of their own, and leaving them unchecked
    /// would hide a second, unrelated mistake until the first one is fixed.
    fn check_args_only(&mut self, args: &[HirId]) -> Ty {
        for &arg in args {
            self.ty_of(arg);
        }
        self.tcx.error()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::diag::DiagCtx;
    use crate::hir::Hir;
    use crate::testing::{resolve_src, typeck_src as check};

    /// Everything up to body checking, which is what candidate collection reads.
    fn collected<'hir>(hir: &'hir Hir) -> Typeck<'hir> {
        let mut checker = Typeck::new(hir);
        checker.collect_module(hir.root_id());
        checker.build_impl_index();
        DiagCtx::clear();
        checker
    }

    /// The `DefId` of the top-level definition named `name`.
    fn named(checker: &Typeck<'_>, name: &str) -> DefId {
        checker
            .hir
            .root()
            .items
            .iter()
            .copied()
            .find(|&id| {
                let text = match checker.hir.def(id) {
                    OwnerNode::Struct(s) => s.name.text,
                    OwnerNode::Enum(e) => e.name.text,
                    OwnerNode::Trait(t) => t.name.text,
                    OwnerNode::Function(f) => f.name.text,
                    _ => return false,
                };
                Interner::resolve(text) == name
            })
            .unwrap_or_else(|| panic!("no definition named {name:?}"))
    }

    fn trait_of(candidate: &Candidate) -> DefId {
        match candidate.source {
            CandidateSource::Trait(def) => def,
            CandidateSource::Inherent => panic!("this candidate is inherent"),
        }
    }

    // -----------------------------------------------------------------
    // Candidate collection
    // -----------------------------------------------------------------

    #[test]
    fn an_inherent_block_offers_the_methods_it_defines() {
        let hir = resolve_src(
            "struct Foo {}
             extend Foo { fun show(&self) {} }",
        );
        let mut checker = collected(&hir);
        let (foo, root) = (named(&checker, "Foo"), hir.root_id());
        let foo_ty = checker.tcx.mk_adt(foo, vec![]);

        let candidates = checker.method_candidates(foo_ty, Interner::intern("show"), root);
        assert_eq!(candidates.len(), 1);
        assert_eq!(candidates[0].source, CandidateSource::Inherent);

        assert!(
            checker
                .method_candidates(foo_ty, Interner::intern("other"), root)
                .is_empty(),
            "a name the block does not define is not offered"
        );
    }

    #[test]
    fn a_trait_impl_offers_what_the_trait_declares() {
        let hir = resolve_src(
            "trait Show { fun show(&self); }
             struct Foo {}
             extend Foo with Show { fun show(&self) {} }",
        );
        let mut checker = collected(&hir);
        let (foo, show, root) = (
            named(&checker, "Foo"),
            named(&checker, "Show"),
            hir.root_id(),
        );
        let foo_ty = checker.tcx.mk_adt(foo, vec![]);

        let candidates = checker.method_candidates(foo_ty, Interner::intern("show"), root);
        assert_eq!(candidates.len(), 1);
        assert_eq!(trait_of(&candidates[0]), show);
    }

    /// A defaulted method is available without appearing in the block, so the candidate is the
    /// trait's own declaration.
    #[test]
    fn a_defaulted_method_is_offered_by_an_impl_that_does_not_write_it() {
        let hir = resolve_src(
            "trait Show { fun show(&self) {} }
             struct Foo {}
             extend Foo with Show {}",
        );
        let mut checker = collected(&hir);
        let (foo, show, root) = (
            named(&checker, "Foo"),
            named(&checker, "Show"),
            hir.root_id(),
        );
        let foo_ty = checker.tcx.mk_adt(foo, vec![]);

        let candidates = checker.method_candidates(foo_ty, Interner::intern("show"), root);
        assert_eq!(candidates.len(), 1);
        let OwnerNode::Trait(declared) = hir.def(show) else {
            unreachable!("`Show` is a trait");
        };
        assert_eq!(candidates[0].method, declared.functions[0]);
    }

    /// The index has nothing to say about a bare parameter, so the environment is the only thing
    /// that can answer.
    #[test]
    fn a_bound_in_scope_offers_its_traits_methods_on_a_parameter() {
        let hir = resolve_src(
            "trait Show { fun show(&self); }
             fun f<T: Show>(x: T) {}",
        );
        let mut checker = collected(&hir);
        let (f, show) = (named(&checker, "f"), named(&checker, "Show"));
        let OwnerNode::Function(function) = hir.def(f) else {
            unreachable!("`f` is a function");
        };
        let t = checker.tcx.mk_generic(function.generics[0]);

        let candidates = checker.method_candidates(t, Interner::intern("show"), f);
        assert_eq!(candidates.len(), 1);
        assert_eq!(trait_of(&candidates[0]), show);
    }

    /// A bound on some *other* parameter says nothing about this one.
    #[test]
    fn a_bound_on_another_parameter_is_not_offered() {
        let hir = resolve_src(
            "trait Show { fun show(&self); }
             fun f<T, U: Show>(x: T) {}",
        );
        let mut checker = collected(&hir);
        let f = named(&checker, "f");
        let OwnerNode::Function(function) = hir.def(f) else {
            unreachable!("`f` is a function");
        };
        let t = checker.tcx.mk_generic(function.generics[0]);

        assert!(
            checker
                .method_candidates(t, Interner::intern("show"), f)
                .is_empty()
        );
    }

    #[test]
    fn a_dyn_receiver_offers_exactly_its_traits_methods() {
        let hir = resolve_src(
            "trait Show { fun show(&self); }
             trait Other { fun other(&self); }",
        );
        let mut checker = collected(&hir);
        let (show, root) = (named(&checker, "Show"), hir.root_id());
        let dyn_show = checker.tcx.mk_dyn(show, vec![]);

        let candidates = checker.method_candidates(dyn_show, Interner::intern("show"), root);
        assert_eq!(candidates.len(), 1);
        assert_eq!(trait_of(&candidates[0]), show);

        assert!(
            checker
                .method_candidates(dyn_show, Interner::intern("other"), root)
                .is_empty(),
            "a `dyn` offers the methods of the trait it names and no others"
        );
    }

    /// An impl whose header does not apply to this receiver is not a candidate, however the name
    /// lines up.
    #[test]
    fn an_impl_whose_header_does_not_match_is_not_a_candidate() {
        let hir = resolve_src(
            "struct Wrap<T> { inner: T }
             struct Foo {}
             struct Bar {}
             extend Wrap<Foo> { fun show(&self) {} }",
        );
        let mut checker = collected(&hir);
        let (wrap, foo, bar, root) = (
            named(&checker, "Wrap"),
            named(&checker, "Foo"),
            named(&checker, "Bar"),
            hir.root_id(),
        );
        let (foo_ty, bar_ty) = (
            checker.tcx.mk_adt(foo, vec![]),
            checker.tcx.mk_adt(bar, vec![]),
        );
        let (wrap_foo, wrap_bar) = (
            checker.tcx.mk_adt(wrap, vec![foo_ty]),
            checker.tcx.mk_adt(wrap, vec![bar_ty]),
        );

        assert_eq!(
            checker
                .method_candidates(wrap_foo, Interner::intern("show"), root)
                .len(),
            1
        );
        assert!(
            checker
                .method_candidates(wrap_bar, Interner::intern("show"), root)
                .is_empty()
        );
    }

    /// Two bounds naming the same trait are one candidate, not an ambiguity between a candidate
    /// and itself.
    #[test]
    fn the_same_method_reached_twice_is_one_candidate() {
        let hir = resolve_src(
            "trait Show { fun show(&self); }
             fun f<T: Show + Show>(x: T) {}",
        );
        let mut checker = collected(&hir);
        let f = named(&checker, "f");
        let OwnerNode::Function(function) = hir.def(f) else {
            unreachable!("`f` is a function");
        };
        let t = checker.tcx.mk_generic(function.generics[0]);

        assert_eq!(
            checker
                .method_candidates(t, Interner::intern("show"), f)
                .len(),
            1
        );
    }

    // -----------------------------------------------------------------
    // Picking
    // -----------------------------------------------------------------

    /// The function named `name` that `owner` -- a trait or an `extend` block -- declares.
    ///
    /// A [`Candidate`]'s `method` is always a real function, and the diagnostics read it to point
    /// at where that function was declared, so a fixture that hands over the trait's own `DefId`
    /// instead is not a shortcut but a fake that the reporting path sees through.
    fn method_of(checker: &Typeck<'_>, owner: DefId, name: &str) -> DefId {
        checker
            .hir
            .def_ids()
            .find(|&id| {
                checker.hir.parent(id) == Some(owner)
                    && matches!(
                        checker.hir.def(id),
                        OwnerNode::Function(f) if Interner::resolve(f.name.text) == name
                    )
            })
            .unwrap_or_else(|| panic!("no method named {name:?}"))
    }

    /// The function named `name` declared by the fixture's one `extend` block. An `extend` block
    /// has no name of its own to look it up by, and a fixture that needs this only ever writes
    /// one.
    fn extend_method(checker: &Typeck<'_>, name: &str) -> DefId {
        let block = checker
            .hir
            .def_ids()
            .find(|&id| matches!(checker.hir.def(id), OwnerNode::Extend(_)))
            .expect("the fixture writes an extend block");
        method_of(checker, block, name)
    }

    fn candidate(source: CandidateSource, method: DefId, self_ty: Ty) -> Candidate {
        Candidate {
            method,
            source,
            self_ty,
            subst: HashMap::new(),
        }
    }

    #[test]
    fn an_inherent_candidate_wins_over_a_trait_one() {
        let hir = resolve_src(
            "trait Show { fun show(&self); }
             struct Foo {}
             extend Foo { fun show(&self) {} }",
        );
        let mut checker = collected(&hir);
        let (foo, show) = (named(&checker, "Foo"), named(&checker, "Show"));
        let inherent = extend_method(&checker, "show");
        let foo_ty = checker.tcx.mk_adt(foo, vec![]);
        let member = Ident {
            text: Interner::intern("show"),
            span: SrcSpan::new(0, 0),
        };

        // Trait first, so that "the inherent one" is not merely "the first one".
        let candidates = [
            candidate(
                CandidateSource::Trait(show),
                method_of(&checker, show, "show"),
                foo_ty,
            ),
            candidate(CandidateSource::Inherent, inherent, foo_ty),
        ];
        let picked = checker
            .pick(&candidates, member)
            .expect("one candidate wins");
        assert_eq!(picked.source, CandidateSource::Inherent);
        assert!(DiagCtx::diagnostics().is_empty());
    }

    #[test]
    fn a_single_trait_candidate_is_picked() {
        let hir = resolve_src(
            "trait Show { fun show(&self); }
             struct Foo {}",
        );
        let mut checker = collected(&hir);
        let (foo, show) = (named(&checker, "Foo"), named(&checker, "Show"));
        let foo_ty = checker.tcx.mk_adt(foo, vec![]);
        let member = Ident {
            text: Interner::intern("show"),
            span: SrcSpan::new(0, 0),
        };

        let candidates = [candidate(
            CandidateSource::Trait(show),
            method_of(&checker, show, "show"),
            foo_ty,
        )];
        assert!(checker.pick(&candidates, member).is_some());
        assert!(DiagCtx::diagnostics().is_empty());
    }

    #[test]
    fn two_trait_candidates_are_an_ambiguity_naming_both() {
        let hir = resolve_src(
            "trait A { fun size(&self); }
             trait B { fun size(&self); }
             struct Foo {}",
        );
        let mut checker = collected(&hir);
        let (foo, a, b) = (
            named(&checker, "Foo"),
            named(&checker, "A"),
            named(&checker, "B"),
        );
        let foo_ty = checker.tcx.mk_adt(foo, vec![]);
        let member = Ident {
            text: Interner::intern("size"),
            span: SrcSpan::new(0, 0),
        };

        let candidates = [
            candidate(
                CandidateSource::Trait(a),
                method_of(&checker, a, "size"),
                foo_ty,
            ),
            candidate(
                CandidateSource::Trait(b),
                method_of(&checker, b, "size"),
                foo_ty,
            ),
        ];
        assert!(checker.pick(&candidates, member).is_none());
        assert_eq!(
            DiagCtx::diagnostics()
                .into_iter()
                .map(|diagnostic| diagnostic.message)
                .collect::<Vec<_>>(),
            ["ambiguous method call: `size` is declared by more than one trait in scope: `A`, `B`"]
        );
    }

    // -----------------------------------------------------------------
    // Source-level
    // -----------------------------------------------------------------

    #[test]
    fn an_inherent_method_call_checks() {
        assert!(
            check(
                "struct Foo {}
                 extend Foo { fun show(&self) {} }
                 fun f(x: Foo) { x.show(); }"
            )
            .is_empty()
        );
    }

    #[test]
    fn a_trait_method_call_checks() {
        assert!(
            check(
                "trait Show { fun show(&self); }
                 struct Foo {}
                 extend Foo with Show { fun show(&self) {} }
                 fun f(x: Foo) { x.show(); }"
            )
            .is_empty()
        );
    }

    /// The case the environment exists for: nothing is known about `T` but the bound.
    #[test]
    fn a_method_reached_through_a_bound_checks() {
        assert!(
            check(
                "trait Show { fun show(&self); }
                 fun f<T: Show>(x: T) { x.show(); }"
            )
            .is_empty()
        );
    }

    #[test]
    fn a_method_on_a_parameter_with_no_bound_is_not_found() {
        assert_eq!(
            check(
                "trait Show { fun show(&self); }
                 fun f<T>(x: T) { x.show(); }"
            ),
            ["no method `show` on `T`"]
        );
    }

    /// Inside a trait, `Self` implements that trait by definition, which is what lets one default
    /// body call another method of the same trait.
    #[test]
    fn a_default_body_may_call_another_method_of_its_own_trait() {
        assert!(
            check("trait Show { fun show(&self); fun show_twice(&self) { self.show(); } }")
                .is_empty()
        );
    }

    #[test]
    fn a_method_on_a_dyn_receiver_checks() {
        assert!(
            check(
                "trait Show { fun show(&self); }
                 fun f(x: &dyn Show) { x.show(); }"
            )
            .is_empty()
        );
    }

    /// An `extend Foo` block is about `Foo` specifically, so its method is the one meant.
    ///
    /// Coherence has its own opinion about this program -- two `extend` blocks for one type may
    /// not offer one name, whatever picks between them afterwards -- so the conflict it reports is
    /// expected here and is the *only* thing expected. What it rules out is a second diagnostic:
    /// the call is checked against a signature returning `i32`, which is the inherent block's, so
    /// picking the trait's `bool` instead would show up as a mismatch on the `return`.
    #[test]
    fn an_inherent_method_beats_a_trait_method_of_the_same_name() {
        assert_eq!(
            check(
                "trait Show { fun show(&self) -> bool; }
                 struct Foo {}
                 extend Foo with Show { fun show(&self) -> bool { return true; } }
                 extend Foo { fun show(&self) -> i32 { return 0; } }
                 fun f(x: Foo) -> i32 { return x.show(); }"
            ),
            ["the method `show` is defined more than once for type `Foo`"]
        );
    }

    /// Coherence cannot see this one: neither trait is implemented for anything in particular
    /// here, so the collision only exists at the call site.
    #[test]
    fn a_method_declared_by_two_bounds_is_ambiguous() {
        assert_eq!(
            check(
                "trait A { fun size(&self); }
                 trait B { fun size(&self); }
                 fun f<T: A + B>(x: T) { x.size(); }"
            ),
            ["ambiguous method call: `size` is declared by more than one trait in scope: `A`, `B`"]
        );
    }

    #[test]
    fn an_unknown_method_is_reported() {
        assert_eq!(
            check(
                "struct Foo {}
                 extend Foo { fun show(&self) {} }
                 fun f(x: Foo) { x.nope(); }"
            ),
            ["no method `nope` on `Foo`"]
        );
    }

    #[test]
    fn a_field_access_checks_to_the_fields_type() {
        assert!(
            check(
                "struct Foo { count: i32 }
                 fun f(x: Foo) -> i32 { return x.count; }"
            )
            .is_empty()
        );
    }

    /// A field's declared type is read through the arguments the receiver applied.
    #[test]
    fn a_generic_structs_field_is_read_through_its_arguments() {
        assert_eq!(
            check(
                "struct Wrap<T> { inner: T }
                 fun f(x: Wrap<i32>) -> bool { return x.inner; }"
            ),
            ["mismatched types: expected `bool`, found `i32`"]
        );
    }

    #[test]
    fn an_unknown_field_is_reported() {
        assert_eq!(
            check(
                "struct Foo { count: i32 }
                 fun f(x: Foo) -> i32 { return x.nope; }"
            ),
            ["no field `nope` on `Foo`"]
        );
    }

    /// `x.show` without a call is an error, and the diagnostic says what to do about it rather
    /// than only that no field of that name exists.
    #[test]
    fn naming_a_method_without_calling_it_says_so() {
        assert_eq!(
            check(
                "struct Foo {}
                 extend Foo { fun show(&self) {} }
                 fun f(x: Foo) { x.show; }"
            ),
            ["no field `show` on `Foo`; there is a method `show`"]
        );
    }

    // -----------------------------------------------------------------
    // Receivers
    // -----------------------------------------------------------------

    #[test]
    fn a_reference_receiver_reaches_a_ref_self_method() {
        assert!(
            check(
                "struct Foo {}
                 extend Foo { fun show(&self) {} }
                 fun f(x: &Foo) { x.show(); }"
            )
            .is_empty()
        );
    }

    /// Autoref: the method wants a reference and the receiver is a place, so one is taken.
    #[test]
    fn a_value_receiver_is_autoreffed_for_a_ref_self_method() {
        assert!(
            check(
                "struct Foo {}
                 extend Foo { fun show(&self) {} }
                 fun f(x: Foo) { x.show(); }"
            )
            .is_empty()
        );
    }

    #[test]
    fn a_mut_ref_receiver_reaches_a_mut_self_method() {
        assert!(
            check(
                "struct Foo {}
                 extend Foo { fun bump(&mut self) {} }
                 fun f(x: &mut Foo) { x.bump(); }"
            )
            .is_empty()
        );
    }

    #[test]
    fn a_shared_reference_cannot_reach_a_mut_self_method() {
        assert_eq!(
            check(
                "struct Foo {}
                 extend Foo { fun bump(&mut self) {} }
                 fun f(x: &Foo) { x.bump(); }"
            ),
            ["`bump` takes `&mut self`, which this receiver cannot provide"]
        );
    }

    #[test]
    fn a_value_receiver_reaches_a_by_value_self_method() {
        assert!(
            check(
                "struct Foo {}
                 extend Foo { fun consume(self) {} }
                 fun f(x: Foo) { x.consume(); }"
            )
            .is_empty()
        );
    }

    #[test]
    fn a_reference_cannot_reach_a_by_value_self_method() {
        assert_eq!(
            check(
                "struct Foo {}
                 extend Foo { fun consume(self) {} }
                 fun f(x: &Foo) { x.consume(); }"
            ),
            ["`consume` takes `self`, which this receiver cannot provide"]
        );
    }

    /// `any self` is the mode that accepts every form, which is what it exists to say.
    #[test]
    fn an_any_self_method_accepts_every_receiver() {
        assert!(
            check(
                "struct Foo {}
                 extend Foo { fun peek(any self) {} }
                 fun f(a: Foo, b: &Foo, c: &mut Foo) { a.peek(); b.peek(); c.peek(); }"
            )
            .is_empty()
        );
    }

    /// A method reached on a value produced by another call has nothing to take a reference to.
    #[test]
    fn a_temporary_receiver_cannot_be_autoreffed() {
        assert_eq!(
            check(
                "struct Foo {}
                 extend Foo { fun show(&self) {} fun make() -> Foo { return Foo {}; } }
                 fun make() -> Foo { return Foo {}; }
                 fun f() { make().show(); }"
            ),
            ["`show` takes `&self`, and this receiver is a temporary"]
        );
    }

    // -----------------------------------------------------------------
    // Arguments
    // -----------------------------------------------------------------

    #[test]
    fn too_many_arguments_are_reported() {
        assert_eq!(
            check(
                "struct Foo {}
                 extend Foo { fun take(&self, n: i32) {} }
                 fun f(x: Foo) { x.take(1, 2); }"
            ),
            ["`take` takes 1 argument but 2 were supplied"]
        );
    }

    #[test]
    fn an_argument_of_the_wrong_type_is_reported() {
        assert_eq!(
            check(
                "struct Foo {}
                 extend Foo { fun take(&self, n: i32) {} }
                 fun f(x: Foo) { x.take(true); }"
            ),
            ["mismatched types: expected `i32`, found `bool`"]
        );
    }

    #[test]
    fn a_methods_return_type_is_read_through_the_impls_arguments() {
        assert_eq!(
            check(
                "struct Wrap<T> { inner: T }
                 extend<T> Wrap<T> { fun get(&self) -> T { return self.inner; } }
                 fun f(x: Wrap<i32>) -> bool { return x.get(); }"
            ),
            ["mismatched types: expected `bool`, found `i32`"]
        );
    }

    // -----------------------------------------------------------------
    // Calls
    // -----------------------------------------------------------------

    #[test]
    fn a_call_to_a_free_function_checks_its_arguments() {
        assert_eq!(
            check(
                "fun g(n: i32) {}
                 fun f() { g(true); }"
            ),
            ["mismatched types: expected `i32`, found `bool`"]
        );
    }

    #[test]
    fn a_call_with_the_wrong_number_of_arguments_is_reported() {
        assert_eq!(
            check(
                "fun g(n: i32) {}
                 fun f() { g(); }"
            ),
            ["this call takes 1 argument but 0 were supplied"]
        );
    }

    /// The registration site this phase wires up: instantiating `g`'s parameter at `Bare` is what
    /// raises `Bare: Show`, and the per-body drain is what answers it.
    #[test]
    fn a_call_to_a_generic_callee_checks_the_callees_bounds() {
        assert_eq!(
            check(
                "trait Show { fun show(&self); }
                 struct Bare {}
                 fun g<T: Show>(x: T) {}
                 fun f(y: Bare) { g(y); }"
            ),
            ["the trait bound `Bare: Show` is not satisfied"]
        );
    }

    #[test]
    fn a_call_to_a_generic_callee_whose_bound_holds_checks() {
        assert!(
            check(
                "trait Show { fun show(&self); }
                 struct Foo {}
                 extend Foo with Show { fun show(&self) {} }
                 fun g<T: Show>(x: T) {}
                 fun f(y: Foo) { g(y); }"
            )
            .is_empty()
        );
    }

    /// A receiver whose type nothing pins down cannot be resolved, and unlike a bound it is not
    /// something a later pass could answer.
    #[test]
    fn an_unresolved_receiver_is_reported() {
        assert_eq!(
            check(
                "struct Foo {}
                 extend Foo { fun show(&self) {} }
                 fun f() { let x = 1; x.show(); }"
            ),
            [
                "type annotations needed: the type of the value `show` is reached on is still \
                 unknown"
            ]
        );
    }

    // -----------------------------------------------------------------
    // `any self`, generic methods, and index-through-a-generic-trait
    // -----------------------------------------------------------------

    /// `any self` accepts every receiver shape: by value, by reference, and by `any`.
    #[test]
    fn any_self_accepts_every_receiver_shape() {
        assert!(
            check(
                "struct Foo {}
                 extend Foo { fun show(any self) {} }
                 fun f(a: Foo, b: &Foo, c: &mut Foo, d: any Foo) {
                     a.show();
                     b.show();
                     c.show();
                     d.show();
                 }"
            )
            .is_empty()
        );
    }

    /// A method's own generic parameters are instantiated fresh per call, independent of the
    /// receiver's type.
    #[test]
    fn a_methods_own_generic_parameter_is_inferred_from_its_argument() {
        assert!(
            check(
                "struct Box_ {}
                 extend Box_ { fun identity<T>(&self, x: T) -> T { return x; } }
                 fun f(b: Box_, n: i32) -> i32 { return b.identity(n); }"
            )
            .is_empty()
        );
        assert_eq!(
            check(
                "struct Box_ {}
                 extend Box_ { fun identity<T>(&self, x: T) -> T { return x; } }
                 fun f(b: Box_, n: i32) -> bool { return b.identity(n); }"
            ),
            ["mismatched types: expected `bool`, found `i32`"]
        );
    }

    /// `Index<K, V>` read through a generic `extend` block: `V` is recovered from the receiver's
    /// own type arguments, not left as the block's bare parameter.
    #[test]
    fn indexing_a_generic_type_reads_v_through_the_receivers_own_arguments() {
        assert!(
            check(
                "module core::ops;

                 public trait Index<K, V> { fun index(&self, key: K) -> &V; }

                 struct Map<V> { value: V }

                 extend<V> Map<V> with Index<i32, V> {
                     fun index(&self, key: i32) -> &V { return &self.value; }
                 }

                 fun f(m: Map<bool>) -> &bool { return m[0]; }"
            )
            .is_empty()
        );
        assert_eq!(
            check(
                "module core::ops;

                 public trait Index<K, V> { fun index(&self, key: K) -> &V; }

                 struct Map<V> { value: V }

                 extend<V> Map<V> with Index<i32, V> {
                     fun index(&self, key: i32) -> &V { return &self.value; }
                 }

                 fun f(m: Map<bool>) -> &i32 { return m[0]; }"
            ),
            ["mismatched types: expected `&i32`, found `&bool`"]
        );
    }
}
