//! Bound checking: enforcing `T: Show` at every place a `T` is chosen.
//!
//! A declaration writes its requirements once -- `struct Sorted<T: Comparable>`,
//! `fun sort<T: Comparable>(..)` -- and every instantiation of it has to meet them. This module
//! is the half of that sentence the solver does not cover: [`solve`](crate::typeck::traits::solve)
//! can answer whether one type implements one trait, and what is left is to ask that question at
//! every point where arguments are applied to something carrying declared bounds, and to ask it at
//! a moment when the answer is knowable.
//!
//! ## Why deferral
//!
//! A direct implementation -- prove the bound where the instantiation is written -- fails,
//! because at that moment the arguments are usually still inference variables. `let x = f(v)`
//! fixes `f`'s parameters from `v`'s type, which may itself be settled several statements later.
//! Asking early gets [`Solution::Ambiguous`], which is neither a pass nor a failure.
//!
//! So an obligation is *registered* rather than proved, and an [`ObligationCx`] holds the ones not
//! yet answered. Draining one -- [`Typeck::select_program_obligations`],
//! [`Typeck::select_body_obligations`] -- loops to a fixpoint: each pass proves what it can,
//! reports what it cannot, and keeps what is still ambiguous for the next one. A pass that
//! discharges nothing is the fixpoint -- nothing between two identical passes changed, so nothing
//! ever will -- and the goals still standing are reported as needing a type annotation.
//!
//! ## Two contexts
//!
//! Obligations are raised in two eras, which want two drain points:
//!
//! - during collection, before [`build_impl_index`](Typeck::build_impl_index) has run. There is no
//!   index yet to prove anything against, so these wait in the **program-level** context and are
//!   drained once the index exists.
//! - while checking one function body, where the arguments only settle as the body is checked. The
//!   **per-body** context is drained at the end of that body, which is the earliest moment its
//!   inference is finished and the latest one at which the diagnostic still has a body to point
//!   into.
//!
//! ## Validity, before satisfaction
//!
//! Two of the checks here are not about whether a bound *holds* but whether it is a bound at all,
//! and they are what makes the rest trustworthy:
//!
//! - a bound naming something that is not a trait (`fun f<T: SomeStruct>()`) contributes nothing to
//!   a [`ParamEnv`](crate::typeck::traits::solve::ParamEnv), which silently makes `T` unbounded.
//!   [`Typeck::check_declared_bounds`] reports it, so that a promise nothing can keep is not read
//!   as no promise at all.
//! - an argument list of the wrong length has nothing to substitute the missing parameters with.
//!   `lower_ty` already checks the count for a written type; [`Typeck::check_impl_headers`] is the
//!   same check for the two lists an `extend` block applies -- to the type it extends, and to the
//!   trait it implements. The second is what
//!   [`check_trait_members`](Typeck::check_trait_members) leaves uncompared and defers to here.

use std::collections::HashMap;
use std::mem;

use crate::ast::interner::Interner;
use crate::diag::{DiagCtx, Diagnostic};
use crate::driver::source::SrcSpan;
use crate::hir::{DefId, HirId, OwnerNode, Path, Res, TyDef, Type};
use crate::typeck::Typeck;
use crate::typeck::traits::TraitRef;
use crate::typeck::traits::index::ImplId;
use crate::typeck::traits::solve::{Obligation, Solution};
use crate::typeck::ty::{Ty, TyKind};

/// One goal waiting to be proved, together with the definition whose assumptions it is proved
/// under.
///
/// An initial design specified `ObligationCx` as a bare `Vec<Obligation>`. It cannot be: the context
/// outlives the moment of registration, and by the time a goal is attempted there is nothing left
/// on the stack saying which `<T: ..>` list was in scope where it came from. A `Foo<T>` written
/// inside `fun f<T: Show>` and the identical one written inside a struct declaration are different
/// questions, and `owner` is what keeps them apart. It names a definition rather than holding a
/// [`ParamEnv`](crate::typeck::traits::solve::ParamEnv) because the environment is built on demand
/// and cached anyway; storing the `DefId` is a copy of four bytes instead of a bound list.
#[derive(Clone, Debug)]
pub struct PendingObligation {
    pub goal: Obligation,

    /// Whose `ParamEnv` the goal is asked against: the function, struct, or `extend` block the
    /// instantiation was written inside.
    pub owner: DefId,
}

/// The goals raised so far and not yet answered.
#[derive(Default)]
pub struct ObligationCx {
    pending: Vec<PendingObligation>,
}

impl ObligationCx {
    pub fn new() -> Self {
        ObligationCx::default()
    }

    /// Records that `goal` has to hold, under `owner`'s assumptions.
    pub fn register(&mut self, goal: Obligation, owner: DefId) {
        self.pending.push(PendingObligation { goal, owner });
    }

    pub fn len(&self) -> usize {
        self.pending.len()
    }

    pub fn is_empty(&self) -> bool {
        self.pending.is_empty()
    }
}

/// The interface the fixpoint loop requires.
///
/// The loop is separated from the checker behind this trait for the same reason
/// [`overlaps`](crate::typeck::traits::overlap::overlaps) takes no diagnostic context: its bugs are
/// the ones a source fixture cannot reach. "A goal that becomes provable on a later pass" and "a
/// goal registered while a pass was running" are both states of *inference*, and inference does not
/// move between two passes of a drain that the test itself drives. A fake prover can put the loop
/// in those states directly; a fixture cannot put it in them at all until expression checking
/// exists to move inference along.
trait Prover {
    /// Attempts one goal.
    fn prove(&mut self, pending: &PendingObligation) -> Solution;

    /// Goals registered while the pass that just finished was running, taken out of wherever they
    /// accumulated. Proving one goal can raise others -- an impl selected recursively may itself
    /// instantiate something bounded -- and they belong to this drain rather than to the next one.
    fn newly_registered(&mut self) -> Vec<PendingObligation>;

    /// The goal does not hold: report it.
    fn unsatisfied(&mut self, pending: &PendingObligation);

    /// The goal is still ambiguous and no pass will change that: report it.
    fn stalled(&mut self, pending: &PendingObligation);
}

/// Proves `pending` to a fixpoint, reporting whatever is left over.
///
/// Each pass discharges what it can and keeps what is not yet knowable. Two things end the
/// loop. The pending set going empty is the ordinary one. The other is a pass that discharges
/// nothing and raises nothing: the set it started with and the set it ended with are the same, so
/// every later pass would do the same again, and those goals are reported as needing an annotation
/// rather than spun on.
///
/// Termination does not rest on the pending set shrinking, because a pass may add to it. It rests
/// on that pass having discharged something -- and the goals that can ever be registered are finite,
/// bounded by the program's own text, so a pass that only adds can only happen finitely often.
fn select_all(mut pending: Vec<PendingObligation>, prover: &mut impl Prover) {
    while !pending.is_empty() {
        let mut retained = Vec::new();
        let mut discharged = 0;

        for goal in pending {
            match prover.prove(&goal) {
                // `Error` counts as settled rather than as a failure: the goal mentions a type
                // that was already reported, and saying anything more about it would be a second
                // diagnostic for one mistake.
                Solution::Holds(_) | Solution::Error => discharged += 1,
                Solution::DoesNotHold => {
                    prover.unsatisfied(&goal);
                    discharged += 1;
                }
                Solution::Ambiguous => retained.push(goal),
            }
        }

        let fresh = prover.newly_registered();
        if discharged == 0 && fresh.is_empty() {
            for goal in &retained {
                prover.stalled(goal);
            }
            return;
        }

        pending = retained;
        pending.extend(fresh);
    }
}

/// The concrete prover: the checker itself, plus which of its two contexts this drain owns.
///
/// The context is reached through a function pointer rather than held as a borrow because the
/// drain hands the checker out mutably to prove each goal, and mid-pass registrations land back in
/// the field while it does. Naming the field instead of borrowing it is what lets both happen.
struct TypeckProver<'a, 'hir> {
    checker: &'a mut Typeck<'hir>,
    cx: for<'t> fn(&'t mut Typeck<'hir>) -> &'t mut ObligationCx,
}

impl Prover for TypeckProver<'_, '_> {
    fn prove(&mut self, pending: &PendingObligation) -> Solution {
        let env = self.checker.param_env(pending.owner);
        self.checker.implements(&pending.goal, &env)
    }

    fn newly_registered(&mut self) -> Vec<PendingObligation> {
        mem::take(&mut (self.cx)(self.checker).pending)
    }

    fn unsatisfied(&mut self, pending: &PendingObligation) {
        self.checker.report_unsatisfied_bound(&pending.goal);
    }

    fn stalled(&mut self, pending: &PendingObligation) {
        self.checker.report_annotations_needed(&pending.goal);
    }
}

impl<'hir> Typeck<'hir> {
    // -----------------------------------------------------------------
    // Registration
    // -----------------------------------------------------------------

    /// Registers everything `def`'s declared bounds demand of `args`.
    ///
    /// This is the one mechanism behind every registration site: applying an argument list to a
    /// definition that declares parameters means each bound written on those parameters becomes a
    /// goal about the corresponding argument. `struct Sorted<T: Comparable>` met with
    /// `Sorted<Foo>` raises `Foo: Comparable`, and the bound's own shape is carried through, so a
    /// hypothetical `T: Into<T>` would raise `Foo: Into<Foo>`.
    ///
    /// A mismatched argument count registers nothing. There is nothing to substitute the extra
    /// parameters with, so every goal built from a half-filled substitution would be about a type
    /// the user never wrote -- and the caller has already reported the count itself, which is the
    /// mistake worth reporting.
    ///
    /// `cause` is where the failure will be pointed at, and `owner` is the definition whose
    /// assumptions it may be discharged from: inside `fun f<U: Comparable>(x: Sorted<U>)` the goal
    /// `U: Comparable` is proved from `f`'s own environment rather than from any impl.
    ///
    /// All four registration sites call this: a written annotation and a `dyn`, in
    /// [`lower_ty`](Typeck::lower_ty); an `extend` block's two argument lists, in
    /// [`check_impl_headers`](Typeck::check_impl_headers); and a call to a generic callee, whose
    /// parameters are instantiated by the arguments at the call site, in
    /// [`register_instantiation`](Typeck::register_instantiation). That last one registers *after*
    /// the argument list has been checked rather than at instantiation, so that the goal it stores
    /// names the type the call settled on instead of the inference variable it started as -- a
    /// failure then reads `Bare: Show` rather than `_: Show`, since a goal is reported as stored.
    pub fn register_bound_obligations(
        &mut self,
        def: DefId,
        args: &[Ty],
        cause: SrcSpan,
        owner: DefId,
    ) {
        let params = self.declared_generics(def);
        if params.len() != args.len() {
            return;
        }

        let mut bounds = Vec::new();
        for &param in &params {
            self.collect_bounds(param, &mut bounds);
        }
        if bounds.is_empty() {
            return;
        }

        let subst: HashMap<HirId, Ty> = params.iter().copied().zip(args.iter().copied()).collect();
        for bound in bounds {
            let self_ty = self.subst_ty(bound.self_ty, &subst);
            let trait_args = bound
                .trait_ref
                .args
                .iter()
                .map(|&arg| self.subst_ty(arg, &subst))
                .collect();
            // The goal is raised at the instantiation, but it exists because of the bound the
            // declaration writes, and that is a second location to show in the diagnostic. `collect_bounds`
            // left it in the bound's own `cause`, which is about to be replaced by this one.
            let mut goal = Obligation::new(
                self_ty,
                TraitRef {
                    def: bound.trait_ref.def,
                    args: trait_args,
                },
                cause,
            );
            if let Some(declared_at) = bound.declared_at {
                goal = goal.with_declared_at(declared_at);
            }
            self.register_obligation(goal, owner);
        }
    }

    /// Puts one goal in whichever context is open right now.
    ///
    /// The two are told apart by *when* rather than by *what*: the same
    /// [`lower_ty`](Typeck::lower_ty) call registers program-level during collection and per-body
    /// inside a body, because the annotation it is lowering is the same annotation either way and
    /// only the surrounding phase differs.
    fn register_obligation(&mut self, goal: Obligation, owner: DefId) {
        if self.in_body {
            self.body_obligations.register(goal, owner);
        } else {
            self.program_obligations.register(goal, owner);
        }
    }

    // -----------------------------------------------------------------
    // Draining
    // -----------------------------------------------------------------

    /// Drains everything raised before body checking began.
    ///
    /// Runs once, immediately after coherence. Collection raises these while the index it would
    /// take to prove them does not exist yet, so they are held until it does -- which is this
    /// moment, and no later: a bound on a struct's parameter is a fact about a *declaration*, and
    /// waiting for some body to mention it would leave a program with no bodies unchecked.
    pub fn select_program_obligations(&mut self) {
        self.select_all(|checker| &mut checker.program_obligations);
    }

    /// Drains everything one function body raised, now that its inference has settled.
    pub fn select_body_obligations(&mut self) {
        self.select_all(|checker| &mut checker.body_obligations);
    }

    fn select_all(&mut self, cx: for<'t> fn(&'t mut Typeck<'hir>) -> &'t mut ObligationCx) {
        let pending = mem::take(&mut cx(self).pending);
        let mut prover = TypeckProver { checker: self, cx };
        select_all(pending, &mut prover);
    }

    // -----------------------------------------------------------------
    // Validity: is this a bound at all?
    // -----------------------------------------------------------------

    /// Checks that every bound written anywhere in the program names a trait.
    ///
    /// Building a [`ParamEnv`](crate::typeck::traits::solve::ParamEnv) skips a bound that does not,
    /// because a struct in bound position cannot be assumed and pretending otherwise would let a
    /// body call methods nothing will ever provide. That leaves the mistake invisible -- `T` simply
    /// comes out unbounded -- so it is reported here instead, once per declaration.
    ///
    /// A whole-program walk rather than a check inside `collect_bounds`, because that runs once per
    /// *environment* a parameter appears in: an `extend` block's `<T: ..>` is collected again for
    /// every method in the block, and reporting there would say the same thing once per method.
    pub fn check_declared_bounds(&mut self) {
        let hir = self.hir;
        for def in hir.def_ids() {
            for &generic in &self.declared_generics(def) {
                for path in &hir.generic(generic).bounds {
                    Self::check_declared_bound(path);
                }
            }
        }
    }

    fn check_declared_bound(path: &Path) {
        match path.res {
            Res::Type(Type::Def(TyDef::Trait(_))) => {}
            // Already reported by name resolution; staying quiet here keeps one mistake from
            // producing a second diagnostic.
            Res::Err => {}
            _ => Self::report_bound_is_not_a_trait(path),
        }
    }

    // -----------------------------------------------------------------
    // Validity: does this argument list fit?
    // -----------------------------------------------------------------

    /// Checks both argument lists of every `extend` block, and registers what they have to satisfy.
    ///
    /// An `extend` block applies arguments twice, and neither list was checked before now.
    /// `extend Foo<i32, bool>` names the type being extended, which
    /// [`self_ty`](Typeck::self_ty) builds without counting -- unlike a written annotation, which
    /// [`lower_ty`](Typeck::lower_ty) does count. `with Index` names the trait, and
    /// [`check_trait_members`](Typeck::check_trait_members) explicitly leaves a wrong count here
    /// rather than comparing every signature against a substitution it could not build.
    ///
    /// Reads the index rather than the HIR, so a block whose self type was rejected at index time
    /// is not measured a second time against a type it does not have.
    pub fn check_impl_headers(&mut self) {
        let impls: Vec<ImplId> = self
            .impls
            .extended_types()
            .into_iter()
            .flat_map(|head| self.impls.for_self(head).to_vec())
            .collect();

        for impl_id in impls {
            let header = self.impls.header(impl_id);
            let (self_ty, trait_ref, def) = (header.self_ty, header.trait_ref.clone(), header.def);

            let TyKind::Adt { def: adt, args } = self.tcx.kind(self_ty).clone() else {
                unreachable!("an indexed impl's self type is always an ADT; see build_impl_index");
            };
            let (adt_path, trait_path) = match self.hir.def(def) {
                OwnerNode::Extend(block) => (&block.adt_path, block.trait_path.as_ref()),
                _ => unreachable!("an ImplHeader's def is always the extend block it came from"),
            };

            if self.check_arg_count(adt, args.len(), adt_path.span) {
                // The bounds the extended type declares are the block's to discharge: writing
                // `extend Sorted<Foo>` is as much an instantiation of `Sorted` as writing the type
                // out in a signature is.
                self.register_bound_obligations(adt, &args, adt_path.span, def);
            }

            if let Some(trait_ref) = trait_ref {
                let span = trait_path.map_or(self.impls.header(impl_id).span, |path| path.span);
                if self.check_arg_count(trait_ref.def, trait_ref.args.len(), span) {
                    self.register_bound_obligations(trait_ref.def, &trait_ref.args, span, def);
                }
            }
        }
    }

    /// Whether `def` declares exactly `found` parameters, reporting the mismatch if it does not.
    ///
    /// Shared by every caller here and by [`lower_dyn`](Typeck::lower_ty), so that applying too few
    /// arguments reads the same way wherever it is written.
    pub fn check_arg_count(&self, def: DefId, found: usize, span: SrcSpan) -> bool {
        let declared = self.declared_generics(def).len();
        if declared == found {
            return true;
        }

        self.report_arg_count_mismatch(def, declared, found, span);
        false
    }

    /// The type parameters `def` declares for itself -- not those of anything enclosing it.
    ///
    /// An `extend` block's is its first bracket group alone. The other two apply arguments rather
    /// than declaring parameters, which is exactly the distinction every caller here rests on.
    fn declared_generics(&self, def: DefId) -> Vec<HirId> {
        match self.hir.def(def) {
            OwnerNode::Function(f) => f.generics.clone(),
            OwnerNode::Struct(s) => s.generics.clone(),
            OwnerNode::Enum(e) => e.generics.clone(),
            OwnerNode::Trait(t) => t.generics.clone(),
            OwnerNode::Extend(e) => e.extend_generics.clone(),
            OwnerNode::Module(_) | OwnerNode::Closure(_) => Vec::new(),
        }
    }

    /// What a definition is called, for a diagnostic that has to name one.
    fn def_name(&self, def: DefId) -> &'static str {
        let name = match self.hir.def(def) {
            OwnerNode::Function(f) => f.name.text,
            OwnerNode::Struct(s) => s.name.text,
            OwnerNode::Enum(e) => e.name.text,
            OwnerNode::Trait(t) => t.name.text,
            OwnerNode::Extend(_) | OwnerNode::Module(_) | OwnerNode::Closure(_) => {
                unreachable!("only a named definition is ever applied to generic arguments")
            }
        };
        Interner::resolve(name)
    }

    /// Where a definition's name was written, for a diagnostic pointing back at what it declares.
    fn def_span(&self, def: DefId) -> SrcSpan {
        match self.hir.def(def) {
            OwnerNode::Function(f) => f.name.span,
            OwnerNode::Struct(s) => s.name.span,
            OwnerNode::Enum(e) => e.name.span,
            OwnerNode::Trait(t) => t.name.span,
            OwnerNode::Extend(_) | OwnerNode::Module(_) | OwnerNode::Closure(_) => {
                unreachable!("only a named definition is ever applied to generic arguments")
            }
        }
    }

    // -----------------------------------------------------------------
    // Diagnostics
    // -----------------------------------------------------------------

    fn report_unsatisfied_bound(&self, goal: &Obligation) {
        let mut diag = Diagnostic::error(
            format!("the trait bound {} is not satisfied", self.show_goal(goal)),
            goal.cause,
        )
        .with_label("this instantiation does not meet the bound its declaration writes")
        .with_help(
            "either write an `extend .. with` block implementing the trait for this type, or \
             pass a type that already has one",
        );

        if let Some(declared_at) = goal.declared_at {
            diag = diag.with_secondary(declared_at, "required by this bound");
        }

        DiagCtx::emit(diag);
    }

    /// A goal that no further pass could decide. Not a failed bound -- it is a bound nobody ever
    /// finished asking about, because the type it is about never became known.
    fn report_annotations_needed(&self, goal: &Obligation) {
        let mut diag = Diagnostic::error(
            format!(
                "type annotations needed: cannot tell whether {} holds",
                self.show_goal(goal)
            ),
            goal.cause,
        )
        .with_label("the type here is still unknown")
        .with_help(
            "nothing in this body pins the type down, so whether it satisfies the bound \
             cannot be decided; write the type out",
        );

        if let Some(declared_at) = goal.declared_at {
            diag = diag.with_secondary(declared_at, "the bound that has to be decided is here");
        }

        DiagCtx::emit(diag);
    }

    fn report_bound_is_not_a_trait(path: &Path) {
        let name = path
            .segments
            .last()
            .map_or("this path", |segment| Interner::resolve(segment.text));

        DiagCtx::emit(
            Diagnostic::error(format!("`{name}` is not a trait"), path.span)
                .with_label("not a trait")
                .with_help(
                    "a bound says what a type parameter must implement, and only a trait can be \
                     implemented; a bound naming anything else promises the body something \
                     nothing could ever supply",
                ),
        );
    }

    fn report_arg_count_mismatch(&self, def: DefId, declared: usize, found: usize, span: SrcSpan) {
        let plural = if declared == 1 { "" } else { "s" };
        DiagCtx::emit(
            Diagnostic::error(
                format!(
                    "`{}` takes {declared} generic argument{plural} but {found} {} supplied",
                    self.def_name(def),
                    if found == 1 { "was" } else { "were" }
                ),
                span,
            )
            .with_label(format!("expected {declared} argument{plural}"))
            .with_secondary(
                self.def_span(def),
                format!(
                    "`{}` declares {declared} type parameter{plural} here",
                    self.def_name(def)
                ),
            )
            .with_help(
                "every parameter has to be given an argument, since the declaration is written in \
                 terms of all of them",
            ),
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::diag::DiagCtx;
    use crate::hir::Hir;
    use crate::nameres::PrimTy;
    use crate::testing::resolve_src;
    use crate::typeck::tyctx::TyCtx;

    // -----------------------------------------------------------------
    // The fixpoint loop
    //
    // Driven against a fake prover, because the key property of the loop is how it behaves
    // when inference moves between two passes -- and inference cannot move during a drain that the
    // test itself is running. See `Prover`.
    // -----------------------------------------------------------------

    /// A prover scripted by pass number: `answers[n]` is what it says to the goals of pass `n`.
    #[derive(Default)]
    struct Fake {
        answers: Vec<Solution>,
        /// Goals to register after the pass whose index this is keyed by.
        register_after: HashMap<usize, Vec<PendingObligation>>,
        pass: usize,
        proved: usize,
        unsatisfied: Vec<Ty>,
        stalled: Vec<Ty>,
    }

    impl Prover for Fake {
        fn prove(&mut self, _pending: &PendingObligation) -> Solution {
            self.proved += 1;
            self.answers
                .get(self.pass)
                .cloned()
                .unwrap_or(Solution::Ambiguous)
        }

        fn newly_registered(&mut self) -> Vec<PendingObligation> {
            let fresh = self.register_after.remove(&self.pass).unwrap_or_default();
            self.pass += 1;
            fresh
        }

        fn unsatisfied(&mut self, pending: &PendingObligation) {
            self.unsatisfied.push(pending.goal.self_ty);
        }

        fn stalled(&mut self, pending: &PendingObligation) {
            self.stalled.push(pending.goal.self_ty);
        }
    }

    fn fake_goal(tcx: &mut TyCtx, prim: PrimTy) -> PendingObligation {
        let self_ty = tcx.mk_prim(prim);
        PendingObligation {
            goal: Obligation::new(
                self_ty,
                TraitRef {
                    def: DefId::from_usize(1),
                    args: Vec::new(),
                },
                SrcSpan::new(0, 0),
            ),
            owner: DefId::from_usize(0),
        }
    }

    /// The point of looping at all: a goal that could not be decided on one pass gets another go.
    #[test]
    fn a_goal_left_ambiguous_is_retried_and_discharged_on_a_later_pass() {
        let mut tcx = TyCtx::new();
        let goal = fake_goal(&mut tcx, PrimTy::I32);
        let mut prover = Fake {
            answers: vec![
                Solution::Ambiguous,
                Solution::Ambiguous,
                Solution::Holds(crate::typeck::traits::solve::ImplSource::FromDyn),
            ],
            ..Fake::default()
        };

        // Two ambiguous passes in a row would be the fixpoint, so the goal only survives to the
        // third because something registers in between.
        prover
            .register_after
            .insert(0, vec![fake_goal(&mut tcx, PrimTy::Bool)]);
        prover
            .register_after
            .insert(1, vec![fake_goal(&mut tcx, PrimTy::Char)]);

        select_all(vec![goal], &mut prover);

        assert!(prover.unsatisfied.is_empty());
        assert!(
            prover.stalled.is_empty(),
            "everything was decided in the end"
        );
        assert_eq!(prover.pass, 3);
    }

    /// The termination condition: a pass that changes nothing means no pass ever will.
    #[test]
    fn a_stalled_set_is_reported_once_and_stops_the_loop() {
        let mut tcx = TyCtx::new();
        let goals = vec![
            fake_goal(&mut tcx, PrimTy::I32),
            fake_goal(&mut tcx, PrimTy::Bool),
        ];
        let mut prover = Fake {
            answers: vec![Solution::Ambiguous],
            ..Fake::default()
        };

        select_all(goals, &mut prover);

        assert_eq!(prover.stalled.len(), 2, "each stalled goal is named once");
        assert_eq!(
            prover.proved, 2,
            "one pass, and no second attempt at either"
        );
    }

    /// Proving one goal may raise others. They belong to this drain, not to the next one.
    #[test]
    fn goals_registered_during_a_pass_are_proved_by_the_same_drain() {
        let mut tcx = TyCtx::new();
        let goal = fake_goal(&mut tcx, PrimTy::I32);
        let mut prover = Fake {
            answers: vec![Solution::DoesNotHold, Solution::DoesNotHold],
            ..Fake::default()
        };
        prover
            .register_after
            .insert(0, vec![fake_goal(&mut tcx, PrimTy::Bool)]);

        select_all(vec![goal], &mut prover);

        assert_eq!(
            prover.proved, 2,
            "the goal raised mid-pass was attempted too"
        );
        assert_eq!(prover.unsatisfied.len(), 2);
    }

    /// A goal about an already-reported type is settled rather than reported again.
    #[test]
    fn a_goal_answering_error_is_discharged_silently() {
        let mut tcx = TyCtx::new();
        let goal = fake_goal(&mut tcx, PrimTy::I32);
        let mut prover = Fake {
            answers: vec![Solution::Error],
            ..Fake::default()
        };

        select_all(vec![goal], &mut prover);

        assert!(prover.unsatisfied.is_empty());
        assert!(prover.stalled.is_empty());
    }

    #[test]
    fn an_empty_context_does_no_passes_at_all() {
        let mut prover = Fake::default();
        select_all(Vec::new(), &mut prover);
        assert_eq!(prover.pass, 0);
    }

    // -----------------------------------------------------------------
    // Source-level
    // -----------------------------------------------------------------

    /// Runs everything up to and including bound checking over `src`, and hands back everything
    /// type checking reported.
    ///
    /// The clear comes *before* collection rather than after it, which is where `coherence`'s and
    /// `members`'s helpers put theirs. One of the registration sites is in `lower_ty`, which runs
    /// during collection, so clearing afterwards would hide exactly what that site says. What is
    /// cleared is name resolution's own output: a fixture is resolved without the core library, so
    /// every one of them reports the whole set of missing lang items first.
    ///
    /// Bodies are deliberately not checked -- most of `check_expr` is still `todo!()` -- so what is
    /// exercised here is the program-level context.
    fn bounds(hir: &Hir) -> Vec<String> {
        DiagCtx::clear();

        let mut checker = Typeck::new(hir);
        checker.collect_module(hir.root_id());
        checker.build_impl_index();
        checker.check_coherence();
        checker.check_trait_members();
        checker.check_declared_bounds();
        checker.check_impl_headers();
        checker.select_program_obligations();

        DiagCtx::diagnostics()
            .into_iter()
            .map(|diagnostic| diagnostic.message)
            .collect()
    }

    #[test]
    fn a_bound_that_is_not_met_by_the_argument_is_reported() {
        let hir = resolve_src(
            "trait Show { fun show(&self); }
             struct Sorted<T: Show> { inner: T }
             struct Bare {}
             fun f(x: Sorted<Bare>) {}",
        );

        assert_eq!(
            bounds(&hir),
            ["the trait bound `Bare: Show` is not satisfied"]
        );
    }

    /// The failure is at the instantiation, but it is only a failure because of the bound the
    /// declaration writes -- so the diagnostic points at both, and the bound's own span survives
    /// being re-raised against the instantiation to get there.
    #[test]
    fn an_unmet_bound_points_at_the_declaration_that_requires_it() {
        let hir = resolve_src(
            "trait Show { fun show(&self); }
             struct Sorted<T: Show> { inner: T }
             struct Bare {}
             fun f(x: Sorted<Bare>) {}",
        );

        DiagCtx::clear();
        let mut checker = Typeck::new(&hir);
        checker.collect_module(hir.root_id());
        checker.build_impl_index();
        checker.select_program_obligations();

        let diagnostics = DiagCtx::diagnostics();
        let [unmet] = diagnostics.as_slice() else {
            panic!("expected exactly one diagnostic, got {diagnostics:?}");
        };
        let [bound] = unmet.secondary.as_slice() else {
            panic!("expected exactly one secondary label");
        };
        assert_eq!(bound.message, "required by this bound");

        // The bound is written on `Sorted`'s declaration, above the use in `f` that failed it.
        let primary = unmet.span.expect("an unmet bound names a place");
        assert!(bound.span.get_begin() < primary.get_begin());
    }

    #[test]
    fn a_bound_met_by_an_impl_is_accepted() {
        let hir = resolve_src(
            "trait Show { fun show(&self); }
             struct Sorted<T: Show> { inner: T }
             struct Foo {}
             extend Foo with Show { fun show(&self) {} }
             fun f(x: Sorted<Foo>) {}",
        );

        assert!(bounds(&hir).is_empty());
    }

    /// The conditional impl's own bound is proved recursively, so the whole chain either holds or
    /// fails as one.
    #[test]
    fn a_bound_met_through_a_conditional_impl_is_accepted() {
        let hir = resolve_src(
            "trait Show { fun show(&self); }
             struct Sorted<T: Show> { inner: T }
             struct Wrap<T> { inner: T }
             struct Foo {}
             extend Foo with Show { fun show(&self) {} }
             extend<T: Show> Wrap<T> with Show { fun show(&self) {} }
             fun f(x: Sorted<Wrap<Foo>>) {}",
        );

        assert!(bounds(&hir).is_empty(), "{:?}", bounds(&hir));
    }

    #[test]
    fn a_conditional_impl_whose_own_bound_fails_does_not_satisfy_the_goal() {
        let hir = resolve_src(
            "trait Show { fun show(&self); }
             struct Sorted<T: Show> { inner: T }
             struct Wrap<T> { inner: T }
             struct Bare {}
             extend<T: Show> Wrap<T> with Show { fun show(&self) {} }
             fun f(x: Sorted<Wrap<Bare>>) {}",
        );

        assert_eq!(
            bounds(&hir),
            ["the trait bound `Wrap<Bare>: Show` is not satisfied"]
        );
    }

    /// The case the `ParamEnv` exists for: nothing is known about `U` except what `f` declared,
    /// which is sufficient to discharge the bound.
    #[test]
    fn a_bound_met_by_an_assumption_in_scope_is_accepted() {
        let hir = resolve_src(
            "trait Show { fun show(&self); }
             struct Sorted<T: Show> { inner: T }
             fun f<U: Show>(x: Sorted<U>) {}",
        );

        assert!(bounds(&hir).is_empty());
    }

    #[test]
    fn a_parameter_passed_on_without_the_bound_it_needs_is_reported() {
        let hir = resolve_src(
            "trait Show { fun show(&self); }
             struct Sorted<T: Show> { inner: T }
             fun f<U>(x: Sorted<U>) {}",
        );

        assert_eq!(bounds(&hir), ["the trait bound `U: Show` is not satisfied"]);
    }

    /// An `extend` block instantiates the type it extends, so its arguments are checked like any
    /// other.
    #[test]
    fn an_extend_blocks_arguments_have_to_satisfy_the_extended_types_bounds() {
        let hir = resolve_src(
            "trait Show { fun show(&self); }
             struct Sorted<T: Show> { inner: T }
             struct Bare {}
             extend Sorted<Bare> { fun get(&self) {} }",
        );

        assert_eq!(
            bounds(&hir),
            ["the trait bound `Bare: Show` is not satisfied"]
        );
    }

    // -----------------------------------------------------------------
    // A bound has to name a trait
    // -----------------------------------------------------------------

    #[test]
    fn a_bound_naming_a_struct_is_reported() {
        let hir = resolve_src(
            "struct Foo {}
             fun f<T: Foo>(x: T) {}",
        );

        assert_eq!(bounds(&hir), ["`Foo` is not a trait"]);
    }

    /// The other nominal thing a bound can name. A primitive cannot be written in bound position
    /// at all -- a bound is parsed as a path of identifiers and a primitive is a keyword -- which
    /// is why the check is phrased over what the path *resolved* to rather than over a list of
    /// kinds it might have been.
    #[test]
    fn a_bound_naming_an_enum_is_reported() {
        let hir = resolve_src(
            "enum Direction { up, down }
             fun f<T: Direction>(x: T) {}",
        );

        assert_eq!(bounds(&hir), ["`Direction` is not a trait"]);
    }

    /// Reported once per declaration, however many environments the parameter turns up in: the
    /// block's `<T>` is collected again for every method it holds.
    #[test]
    fn a_bad_bound_on_an_extend_block_is_reported_once() {
        let hir = resolve_src(
            "struct Foo {}
             struct Wrap<T> { inner: T }
             extend<T: Foo> Wrap<T> { fun a(&self) {} fun b(&self) {} }",
        );

        assert_eq!(bounds(&hir), ["`Foo` is not a trait"]);
    }

    #[test]
    fn a_bound_that_did_not_resolve_reports_nothing_further() {
        let hir = resolve_src("fun f<T: Nope>(x: T) {}");

        assert!(
            bounds(&hir).is_empty(),
            "name resolution already reported the missing name"
        );
    }

    #[test]
    fn a_bound_naming_a_trait_is_accepted() {
        let hir = resolve_src(
            "trait Show { fun show(&self); }
             fun f<T: Show>(x: T) {}",
        );

        assert!(bounds(&hir).is_empty());
    }

    // -----------------------------------------------------------------
    // Argument counts
    // -----------------------------------------------------------------

    #[test]
    fn a_with_clause_missing_the_traits_arguments_is_reported() {
        let hir = resolve_src(
            "trait Index<K, V> { fun get(&self, key: K) -> V; }
             struct Map {}
             extend Map with Index { fun get(&self, key: i32) -> bool {} }",
        );

        assert_eq!(
            bounds(&hir),
            ["`Index` takes 2 generic arguments but 0 were supplied"]
        );
    }

    #[test]
    fn a_with_clause_with_the_right_number_of_arguments_is_accepted() {
        let hir = resolve_src(
            "trait Index<K, V> { fun get(&self, key: K) -> V; }
             struct Map {}
             extend Map with Index<i32, bool> { fun get(&self, key: i32) -> bool {} }",
        );

        assert!(bounds(&hir).is_empty());
    }

    /// Unlike a written annotation, the type an `extend` block names is built without counting --
    /// so this is the one place the count was never checked before.
    #[test]
    fn an_extend_block_applying_the_wrong_number_of_arguments_is_reported() {
        let hir = resolve_src(
            "struct Wrap<T> { inner: T }
             extend Wrap<i32, bool> { fun get(&self) {} }",
        );

        assert_eq!(
            bounds(&hir),
            ["`Wrap` takes 1 generic argument but 2 were supplied"]
        );
    }

    /// A `dyn` is an application of the trait's parameters like any other, so leaving them off a
    /// trait that declares some is the same mistake as leaving them off a struct -- and, since
    /// `dyn` carries its own argument list, one with a spelling that fixes it.
    #[test]
    fn a_dyn_naming_a_trait_with_parameters_is_reported() {
        let hir = resolve_src(
            "trait Index<K, V> { fun get(&self, key: K) -> V; }
             fun f(x: &dyn Index) {}",
        );

        assert_eq!(
            bounds(&hir),
            ["`Index` takes 2 generic arguments but 0 were supplied"]
        );
    }
}
