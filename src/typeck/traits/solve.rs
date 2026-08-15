//! The query: does `self_ty` implement `trait_ref`, given what the surrounding definition lets
//! us assume?
//!
//! [`Typeck::implements`] answers it, in this order, and the order is the design:
//!
//! 1. **Resolve** the goal through the inference unifier. A goal containing
//!    [`TyKind::Error`] answers [`Solution::Error`] -- a diagnostic for it already exists, and a
//!    second one would be noise. A goal whose self type is still an inference variable answers
//!    [`Solution::Ambiguous`]: not "no", but "ask again once inference has settled".
//! 2. **The environment first.** A bound written on a parameter beats any impl that happens to
//!    match, because inside `fun f<T: Show>(x: T)` the only thing known about `T` is what was
//!    declared. This step is the whole reason a generic function can use its own bounds.
//! 3. **`dyn`.** `dyn Show` satisfies `Show` and nothing else. There is no impl behind it --
//!    impls are nominal -- so it is a rule here rather than an entry in the index.
//! 4. **Otherwise require an ADT.** References, tuples, arrays and `any` implement nothing.
//!    `x.show()` where `x: &Foo` is the job of receiver adjustment in method resolution, not of
//!    making references implement things.
//! 5. **Match candidates**, one-way (see [`match_ty`]), and
//! 6. **recurse** into the selected impl's own obligations.
//!
//! ## Matching, not unification
//!
//! [`match_ty`] is one-way on purpose: it never touches the global
//! [`Unifier`](crate::typeck::unify::Unifier), so selecting a candidate can never constrain the
//! goal's inference variables. Two things fall out of that. Step 1 can honestly report
//! `Ambiguous` instead of guessing an impl and poisoning inference with the guess; and no
//! snapshot/rollback machinery has to be added to `Unifier`, which has none.
//!
//! ## Termination
//!
//! Two cutoffs, both of which report and answer [`Solution::Error`] rather than succeeding
//! quietly. A goal already in progress is a cyclic bound -- there is no coinduction here, because
//! phi has no auto traits for which "assume it holds" would be the right answer. And a hard depth
//! cap catches the growing-goal case, `T` -> `Box<T>` -> `Box<Box<T>>`, which no cycle check can
//! see because no goal ever repeats.

use std::collections::HashMap;

use crate::diagnostics::typeck::traits::solve::{
    report_ambiguous_extends, report_cyclic_bound, report_recursion_limit,
    report_require_extends_fails,
};
use crate::driver::source::SrcSpan;
use crate::hir::{DefId, HirId, OwnerNode, Res, TyDef, Type};
use crate::typeck::traits::index::ImplId;
use crate::typeck::traits::TraitRef;
use crate::typeck::ty::{Ty, TyKind};
use crate::typeck::tyctx::TyCtx;
use crate::typeck::Typeck;

/// How deep the solver will chase an impl's obligations before giving up.
///
/// This is not the cycle check, which catches a goal that *repeats*. It catches a goal that grows
/// without ever repeating, where every step is a new question and the recursion is nonetheless
/// infinite.
pub const RECURSION_LIMIT: usize = 128;

/// One thing that must be proved: that `self_ty` implements `trait_ref`.
///
/// `cause` is where to point when it fails, and is deliberately *not* part of what makes two
/// obligations the same question -- see [`Obligation::same_goal`].
///
/// `declared_at` is the other half of that story. A goal raised because a declaration writes
/// `<T: Show>` is about two places at once: the instantiation that failed to meet the bound, and
/// the bound itself. `cause` is the first, `declared_at` the second. It is `None` for a goal
/// nobody declared -- one the solver raised for itself while chasing an impl's own obligations.
#[derive(Clone, Debug)]
pub struct Obligation {
    pub self_ty: Ty,
    pub trait_ref: TraitRef,
    pub cause: SrcSpan,
    pub declared_at: Option<SrcSpan>,
}

impl Obligation {
    pub fn new(self_ty: Ty, trait_ref: TraitRef, cause: SrcSpan) -> Self {
        Obligation {
            self_ty,
            trait_ref,
            cause,
            declared_at: None,
        }
    }

    /// Records where the bound this goal came from was written. See [`Obligation::declared_at`].
    pub fn with_declared_at(mut self, span: SrcSpan) -> Self {
        self.declared_at = Some(span);
        self
    }

    /// Whether two obligations ask the same question, ignoring where each was raised.
    ///
    /// Cycle detection and the environment lookup both want this rather than `==`: the same goal
    /// reached from two places is still the same goal, and a bound in a `ParamEnv` carries the
    /// span of its own declaration, not of the call site asking about it.
    pub fn same_goal(&self, other: &Obligation) -> bool {
        self.self_ty == other.self_ty && self.trait_ref == other.trait_ref
    }
}

/// What may be *assumed* while checking one definition: every bound on every type parameter in
/// scope, plus a trait's implicit `Self: ThatTrait`.
///
/// Built once per definition and cached, since every goal raised inside a body is asked against
/// the same one.
///
/// The same set plays two roles depending on which side of the impl you stand on. Checking an
/// `extend<T: Show> Box<T>` block's own method bodies, `T: Show` is an assumption. Selecting that
/// impl from outside, the very same `T: Show` is an *obligation* the caller has to discharge.
/// That is why phi needs no `where` clause syntax and this design stores an impl's obligations
/// nowhere: they are already here.
#[derive(Clone, Debug, Default)]
pub struct ParamEnv {
    pub bounds: Vec<Obligation>,
}

impl ParamEnv {
    /// The environment that assumes nothing, for a goal raised where no parameters are in scope.
    pub fn empty() -> Self {
        ParamEnv::default()
    }
}

/// The answer to a query.
///
/// `Holds` carries no payload. An earlier design had it carry an `ImplSource` recording *why*
/// the goal held -- which impl matched and under what substitution, or that a bound or `dyn`
/// answered it instead -- meant for method resolution to instantiate the method it found with.
/// Method resolution never ended up asking this query at all: a call site has its own candidates
/// to collect, because a bound and a `dyn` receiver offer methods this query has no impl to point
/// at (see [`method`](crate::typeck::traits::method)), so it re-derives a matching impl's
/// substitution itself rather than reading one out of a `Solution`. With nothing left to read it,
/// carrying it was a second bookkeeping burden for no reader -- every real caller already
/// collapsed `Holds(_)` to `()`. What a match still needs internally (which impl, and under what
/// substitution) is computed and used locally inside [`Typeck::implements`], and never has to
/// leave it.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Solution {
    Holds,
    DoesNotHold,
    /// The goal still contains inference variables, so it can be neither proved nor disproved
    /// yet. Ask again once more of the body has been checked.
    Ambiguous,
    /// The goal contained [`TyKind::Error`], or a cutoff fired. Either way a diagnostic already
    /// exists, and this exists so that one earlier mistake does not cascade into a second --
    /// exactly the role `TyKind::Error` plays in unification.
    Error,
}

/// Matches the open type `impl_ty` against the closed type `goal_ty`, recording in `subst` what
/// each of the impl's own parameters had to be.
///
/// One-way. Where `impl_ty` is a [`TyKind::Generic`] belonging to `generics`, it binds -- or
/// checks against an existing binding, so a parameter written twice has to take one value in both
/// places. A `TyKind::Generic` that is *not* one of this impl's parameters is an ordinary rigid
/// constant, matching only itself. Everywhere else this requires structural equality, recursing
/// into components.
///
/// Nothing in `goal_ty` is ever bound, which is what keeps candidate selection from constraining
/// the caller's inference variables; see the [module docs](self).
pub fn match_ty(
    tcx: &TyCtx,
    generics: &[HirId],
    impl_ty: Ty,
    goal_ty: Ty,
    subst: &mut HashMap<HirId, Ty>,
) -> bool {
    if let TyKind::Generic(param) = *tcx.kind(impl_ty)
        && generics.contains(&param)
    {
        return match subst.get(&param) {
            // Interning makes this the whole of the consistency check: two structurally equal
            // ground types are the same handle.
            Some(&bound) => bound == goal_ty,
            None => {
                subst.insert(param, goal_ty);
                true
            }
        };
    }

    // Identical handles are identical types, so the common case costs one comparison. This also
    // short-circuits the pairs below that carry no components at all.
    if impl_ty == goal_ty {
        return true;
    }

    match (tcx.kind(impl_ty), tcx.kind(goal_ty)) {
        // An impl header that failed to lower matches nothing. A goal containing an error never
        // reaches here -- `implements` answers `Solution::Error` before selecting candidates.
        (TyKind::Error, _) | (_, TyKind::Error) => false,

        (TyKind::Adt { def: d, args: x }, TyKind::Adt { def: e, args: y })
        | (TyKind::Dyn { trait_: d, args: x }, TyKind::Dyn { trait_: e, args: y }) => {
            d == e && match_all(tcx, generics, x, y, subst)
        }

        (
            TyKind::Ref {
                base: x,
                mutability: m,
            },
            TyKind::Ref {
                base: y,
                mutability: n,
            },
        ) => m == n && match_ty(tcx, generics, *x, *y, subst),

        (TyKind::Any(x), TyKind::Any(y)) => match_ty(tcx, generics, *x, *y, subst),

        (TyKind::Tuple(x), TyKind::Tuple(y)) => match_all(tcx, generics, x, y, subst),

        (TyKind::Array { elem: x, len: m }, TyKind::Array { elem: y, len: n }) => {
            m == n && match_ty(tcx, generics, *x, *y, subst)
        }

        (
            TyKind::Fun {
                params: x,
                ret: r_x,
            },
            TyKind::Fun {
                params: y,
                ret: r_y,
            },
        ) => match (r_x, r_y) {
            _ if !match_all(tcx, generics, x, y, subst) => false,
            (Some(r_x), Some(r_y)) => match_ty(tcx, generics, *r_x, *r_y, subst),
            (None, None) => true,
            (Some(_), None) | (None, Some(_)) => false,
        },

        // Everything with no components was already decided by the handle comparison above: two
        // primitives, two rigid parameters, two `SelfTy`s, `Unit`, `Never`, and an inference
        // variable in the goal that the impl does not have a parameter to absorb.
        _ => false,
    }
}

fn match_all(
    tcx: &TyCtx,
    generics: &[HirId],
    impl_tys: &[Ty],
    goal_tys: &[Ty],
    subst: &mut HashMap<HirId, Ty>,
) -> bool {
    impl_tys.len() == goal_tys.len()
        && impl_tys
            .iter()
            .zip(goal_tys.iter())
            .all(|(&a, &b)| match_ty(tcx, generics, a, b, subst))
}

/// How a caller identifies the trait in a goal, before it is resolved to a [`DefId`].
///
/// The two variants differ in whether the lookup can fail. [`TraitName::Def`] carries a `DefId`
/// name resolution already produced, so it is infallible. [`TraitName::Lang`] carries a
/// [`LangItem`](crate::langitems::LangItem), which [`LangItems::get`](crate::langitems::LangItems::get)
/// resolves to a `DefId` only if the core library declared it -- it returns `None` otherwise,
/// which [`Typeck::extends`] maps to [`Solution::Error`].
///
/// Both variants exist so that [`Typeck::extends`] has one signature rather than one per lookup
/// kind. `Obligation` stores the resolved `DefId`, so this type appears only in the argument
/// position and never inside a goal.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum TraitName {
    /// Resolved through [`Hir::lang_items`](crate::hir::Hir::lang_items). Yields no `DefId` when
    /// the item is missing.
    Lang(crate::langitems::LangItem),
    /// Already resolved by name resolution: a bound's `Path::res`, an `extend` header's
    /// `trait_path`, or a `dyn` type's trait.
    Def(DefId),
}

impl<'hir> Typeck<'hir> {
    /// Resolves `name` to a `DefId`, builds the [`Obligation`] for `self_ty: name<args>`, reads
    /// `owner`'s [`ParamEnv`], and calls [`Typeck::implements`] with the two.
    ///
    /// Those four steps are what separates a caller holding a [`Ty`] and a trait from the solver,
    /// which takes a constructed goal and an environment. `implements` remains available for
    /// callers inside [`traits`](super) that already hold both -- [`bounds`](super::bounds)
    /// replays stored `Obligation`s, and `solve` recurses on substituted ones.
    ///
    /// Returns [`Solution::Error`] when `name` is a [`TraitName::Lang`] that did not resolve.
    /// This is distinct from [`Solution::DoesNotHold`] in what the caller must do: `Error` means
    /// [`collect_ast`](crate::langitems::collect_ast) has already emitted a diagnostic for the
    /// missing item and the caller must not emit another, while `DoesNotHold` is an answer about
    /// the program that the caller is expected to report. Reporting `DoesNotHold` for an
    /// unresolved lang item would attribute a missing core library to the program under
    /// compilation.
    pub fn extends(
        &mut self,
        self_ty: Ty,
        name: TraitName,
        args: Vec<Ty>,
        owner: DefId,
        span: SrcSpan,
    ) -> Solution {
        let Some(def) = self.trait_def(name) else {
            return Solution::Error;
        };

        let goal = Obligation::new(self_ty, TraitRef { def, args }, span);
        let env = self.param_env(owner);
        self.implements(&goal, &env)
    }

    /// [`Typeck::extends`], collapsing the four-variant [`Solution`] to a `bool` and emitting the
    /// diagnostic for [`Solution::DoesNotHold`].
    ///
    /// Returns `true` only for [`Solution::Holds`]. The other three variants all return `false`
    /// and differ only in whether a diagnostic is emitted, which is the caller's reason for not
    /// needing to distinguish them:
    ///
    /// - `DoesNotHold`: emits one error here, naming `self_ty` and the trait.
    /// - `Ambiguous`: `self_ty` is an unresolved [`TyKind::Var`], so no impl can be selected yet.
    ///   Emits nothing, since the goal may hold once unification resolves the variable.
    /// - `Error`: `self_ty` contains [`TyKind::Error`], or `name` is an unresolved lang item.
    ///   Emits nothing, since a diagnostic for the underlying failure already exists.
    ///
    /// `because` is interpolated into the label as "`{because}` needs an `extend .. with T`
    /// block providing it", naming the construct that raised the goal -- an operator, an index
    /// expression, a `for` loop. [`Obligation::cause`] records only a [`SrcSpan`], so the
    /// construct's identity is not recoverable from the goal itself.
    pub fn require_extends(
        &mut self,
        self_ty: Ty,
        name: TraitName,
        args: Vec<Ty>,
        owner: DefId,
        span: SrcSpan,
        because: &str,
    ) -> bool {
        match self.extends(self_ty, name, args, owner, span) {
            Solution::Holds => true,
            Solution::DoesNotHold => {
                let trait_name = self
                    .trait_def(name)
                    .map(|def| crate::diagnostics::typeck::display::def_name(self.hir, def))
                    .unwrap_or("<unresolved trait>");
                report_require_extends_fails(self.cx(), self_ty, trait_name, because, span);
                false
            }
            Solution::Ambiguous | Solution::Error => false,
        }
    }

    /// The definition a [`TraitName`] names, or `None` for a lang item that did not resolve.
    fn trait_def(&self, name: TraitName) -> Option<DefId> {
        match name {
            TraitName::Lang(item) => self.hir.lang_items().get(item),
            TraitName::Def(def) => Some(def),
        }
    }

    /// Answers whether `goal` holds, assuming `env`. See the [module docs](self) for the order
    /// the steps run in and why.
    ///
    /// Prefer [`Typeck::extends`] from outside this module: it builds the goal and the
    /// environment, which is what a caller holding a type and a trait actually has.
    pub fn implements(&mut self, goal: &Obligation, env: &ParamEnv) -> Solution {
        // Step 1. Everything below compares interned handles, which only means "same type" once
        // every inference variable that has been resolved is replaced by what it resolved to.
        let goal = Obligation {
            self_ty: self.resolve_deep(goal.self_ty),
            trait_ref: TraitRef {
                def: goal.trait_ref.def,
                args: goal
                    .trait_ref
                    .args
                    .iter()
                    .map(|&arg| self.resolve_deep(arg))
                    .collect(),
            },
            cause: goal.cause,
            declared_at: goal.declared_at,
        };

        if self.goal_mentions_error(&goal) {
            return Solution::Error;
        }
        if matches!(self.tcx.kind(goal.self_ty), TyKind::Var(_)) {
            return Solution::Ambiguous;
        }

        if self.goal_stack.iter().any(|open| open.same_goal(&goal)) {
            report_cyclic_bound(self.hir, self.cx(), &goal);
            return Solution::Error;
        }
        if self.goal_stack.len() >= RECURSION_LIMIT {
            report_recursion_limit(self.hir, self.cx(), &goal);
            return Solution::Error;
        }

        self.goal_stack.push(goal.clone());
        let solution = self.solve(&goal, env);
        self.goal_stack.pop();
        solution
    }

    /// Steps 2 through 6, with the goal already resolved and on the in-progress stack.
    fn solve(&mut self, goal: &Obligation, env: &ParamEnv) -> Solution {
        // Step 2. Bounds are ground in their parameter's own terms -- `TyKind::Generic` or
        // `TyKind::SelfTy` -- so this is equality, not matching.
        if env.bounds.iter().any(|bound| bound.same_goal(goal)) {
            return Solution::Holds;
        }

        // Step 3.
        if let TyKind::Dyn { trait_, args } = self.tcx.kind(goal.self_ty) {
            let holds = *trait_ == goal.trait_ref.def && *args == goal.trait_ref.args;
            return if holds {
                Solution::Holds
            } else {
                Solution::DoesNotHold
            };
        }

        // Step 4.
        let TyKind::Adt { def, .. } = *self.tcx.kind(goal.self_ty) else {
            return Solution::DoesNotHold;
        };

        // Step 5.
        let Some((impl_id, subst)) = self.select(def, goal) else {
            return Solution::DoesNotHold;
        };

        // Step 6. The impl's own obligations are the bounds on its `extend<T: ..>` generics --
        // see `ParamEnv` -- substituted through what made it match. They are proved in the
        // *caller's* environment, since that is who has to discharge them.
        let obligations = self.param_env(self.impls.header(impl_id).def).bounds;
        for obligation in obligations {
            let sub_goal = Obligation {
                self_ty: self.subst_ty(obligation.self_ty, &subst),
                trait_ref: TraitRef {
                    def: obligation.trait_ref.def,
                    args: obligation
                        .trait_ref
                        .args
                        .iter()
                        .map(|&arg| self.subst_ty(arg, &subst))
                        .collect(),
                },
                // The failure is about the goal that dragged this in, so that is what the error
                // points at. Where the bound was declared is not lost, though -- it rides along
                // as the secondary location, which is the one thing it is good for.
                cause: goal.cause,
                declared_at: obligation.declared_at,
            };

            match self.implements(&sub_goal, env) {
                Solution::Holds => {}
                Solution::DoesNotHold => return Solution::DoesNotHold,
                Solution::Ambiguous => return Solution::Ambiguous,
                Solution::Error => return Solution::Error,
            }
        }

        Solution::Holds
    }

    /// The one impl of `goal`'s trait whose header matches `goal`, if there is one.
    ///
    /// Coherence guarantees at most one, which is what makes the query a function. A second match
    /// is a bug in coherence rather than a program error, so it trips an assertion in debug and
    /// is reported rather than silently resolved by arbitrary choice in release.
    fn select(&mut self, head: DefId, goal: &Obligation) -> Option<(ImplId, HashMap<HirId, Ty>)> {
        let mut matches: Vec<(ImplId, HashMap<HirId, Ty>)> = Vec::new();

        for &impl_id in self.impls.for_self(head) {
            let header = self.impls.header(impl_id);
            let Some(trait_ref) = &header.trait_ref else {
                // An inherent block implements no trait, so it is never a candidate here. Its
                // methods are found by method resolution, not by this query.
                continue;
            };
            if trait_ref.def != goal.trait_ref.def {
                continue;
            }

            let mut subst = HashMap::new();
            let matched = match_ty(
                &self.tcx,
                &header.generics,
                header.self_ty,
                goal.self_ty,
                &mut subst,
            ) && match_all(
                &self.tcx,
                &header.generics,
                &trait_ref.args,
                &goal.trait_ref.args,
                &mut subst,
            );

            if matched {
                matches.push((impl_id, subst));
            }
        }

        debug_assert!(
            matches.len() <= 1,
            "two impls matched one goal, which coherence is supposed to have made impossible"
        );
        if matches.len() > 1 {
            report_ambiguous_extends(self.hir, self.cx(), goal);
            return None;
        }
        matches.pop()
    }

    /// Everything `def` may assume: the bounds on its own generics, on those of every definition
    /// enclosing it, and -- inside a trait -- that `Self` implements that trait.
    ///
    /// Walking up the parent chain is what gives a method the `extend` block's parameters. The
    /// three bracket groups of an `extend` block are not equal here: only the first *declares*
    /// parameters, so only it can carry bounds.
    ///
    /// No elaboration step exists, because phi has no supertraits. If they are added, closing the
    /// bound set under the supertrait relation goes here and nothing else in this design changes.
    pub fn param_env(&mut self, def: DefId) -> ParamEnv {
        if let Some(env) = self.param_envs.get(&def) {
            return env.clone();
        }

        let mut bounds = Vec::new();
        let mut current = Some(def);
        while let Some(owner) = current {
            let (generics, self_bound) = match self.hir.def(owner) {
                OwnerNode::Function(f) => (f.generics.clone(), None),
                OwnerNode::Struct(s) => (s.generics.clone(), None),
                OwnerNode::Enum(e) => (e.generics.clone(), None),
                // Inside a trait, the implicit `Self` implements that trait by definition, which
                // is what lets one default method call another.
                OwnerNode::Trait(t) => (
                    t.generics.clone(),
                    Some((owner, t.generics.clone(), t.span)),
                ),
                OwnerNode::Extend(e) => (e.extend_generics.clone(), None),
                // A module or a closure declares no generics of its own. Walking through rather
                // than stopping is what lets a closure body see its enclosing function's bounds.
                OwnerNode::Module(_) | OwnerNode::Closure(_) => (Vec::new(), None),
            };

            for generic in generics {
                self.collect_bounds(generic, &mut bounds);
            }
            if let Some((trait_def, trait_generics, span)) = self_bound {
                let self_ty = self.tcx.mk_self_param(trait_def);
                let args = trait_generics
                    .iter()
                    .map(|&id| self.tcx.mk_generic(id))
                    .collect();
                bounds.push(Obligation::new(
                    self_ty,
                    TraitRef {
                        def: trait_def,
                        args,
                    },
                    span,
                ));
            }

            current = self.hir.parent(owner);
        }

        let env = ParamEnv { bounds };
        self.param_envs.insert(def, env.clone());
        env
    }

    /// Turns the bounds written on one type parameter into obligations about it.
    ///
    /// A bound that did not resolve to a trait is skipped rather than reported. Name resolution
    /// already reported the unresolvable ones, and a bound naming a struct is a mistake that
    /// belongs to bound *checking* rather than to building the set of things that may be assumed:
    /// [`check_declared_bounds`](Typeck::check_declared_bounds) is what reports it, once per
    /// declaration rather than once per environment the parameter appears in. Skipping it here
    /// means it can never be assumed, which is the safe answer.
    pub(crate) fn collect_bounds(&mut self, generic: HirId, bounds: &mut Vec<Obligation>) {
        let hir: &'hir crate::hir::Hir = self.hir;
        let span = hir.generic(generic).span;

        for path in &hir.generic(generic).bounds {
            let Res::Type(Type::Def(TyDef::Trait(def))) = path.res else {
                continue;
            };

            let self_ty = self.tcx.mk_generic(generic);
            // A bound is a bare path with no argument list, so a trait's own parameters can never
            // be applied in one. When that syntax arrives, its arguments are lowered here.
            //
            // `span` serves as both cause and declaration site here, because a bound read straight
            // out of a declaration *is* its own declaration. It stops being both once
            // `register_bound_obligations` re-raises this against an instantiation, which is where
            // the two come apart.
            bounds.push(
                Obligation::new(
                    self_ty,
                    TraitRef {
                        def,
                        args: Vec::new(),
                    },
                    span,
                )
                .with_declared_at(span),
            );
        }
    }

    /// Rebuilds `ty` with every parameter in `subst` replaced by what it is bound to.
    ///
    /// Only the parameters in `subst` are touched; anything else -- including a parameter of some
    /// enclosing definition -- is left exactly as it was.
    pub fn subst_ty(&mut self, ty: Ty, subst: &HashMap<HirId, Ty>) -> Ty {
        match self.tcx.kind(ty).clone() {
            TyKind::Generic(param) => subst.get(&param).copied().unwrap_or(ty),
            TyKind::Adt { def, args } => {
                let args = self.subst_tys(&args, subst);
                self.tcx.mk_adt(def, args)
            }
            TyKind::Dyn { trait_, args } => {
                let args = self.subst_tys(&args, subst);
                self.tcx.mk_dyn(trait_, args)
            }
            TyKind::Tuple(elems) => {
                let elems = self.subst_tys(&elems, subst);
                self.tcx.mk_tuple(elems)
            }
            TyKind::Ref { base, mutability } => {
                let base = self.subst_ty(base, subst);
                self.tcx.mk_ref(base, mutability)
            }
            TyKind::Any(base) => {
                let base = self.subst_ty(base, subst);
                self.tcx.mk_any(base)
            }
            TyKind::Array { elem, len } => {
                let elem = self.subst_ty(elem, subst);
                self.tcx.mk_array(elem, len)
            }
            TyKind::Fun { params, ret } => {
                let params = self.subst_tys(&params, subst);
                let ret = ret.map(|ret| self.subst_ty(ret, subst));
                self.tcx.mk_fun(params, ret)
            }
            // Nothing to substitute into. `SelfTy` in particular is not a parameter this
            // substitution knows about: an impl's `Self` was already replaced by its self type
            // when the header was lowered.
            TyKind::Var(_)
            | TyKind::Primitive(_)
            | TyKind::SelfTy(_)
            | TyKind::Unit
            | TyKind::Never
            | TyKind::Error => ty,
        }
    }

    fn subst_tys(&mut self, tys: &[Ty], subst: &HashMap<HirId, Ty>) -> Vec<Ty> {
        tys.iter().map(|&ty| self.subst_ty(ty, subst)).collect()
    }

    /// Whether any part of the goal is [`TyKind::Error`], which means something about it was
    /// already reported.
    fn goal_mentions_error(&self, goal: &Obligation) -> bool {
        self.mentions_error(goal.self_ty)
            || goal
                .trait_ref
                .args
                .iter()
                .any(|&arg| self.mentions_error(arg))
    }

    fn mentions_error(&self, ty: Ty) -> bool {
        match self.tcx.kind(ty) {
            TyKind::Error => true,
            TyKind::Adt { args, .. } | TyKind::Dyn { args, .. } | TyKind::Tuple(args) => {
                args.iter().any(|&arg| self.mentions_error(arg))
            }
            TyKind::Ref { base, .. } | TyKind::Any(base) => self.mentions_error(*base),
            TyKind::Array { elem, .. } => self.mentions_error(*elem),
            TyKind::Fun { params, ret } => {
                params.iter().any(|&param| self.mentions_error(param))
                    || ret.is_some_and(|ret| self.mentions_error(ret))
            }
            TyKind::Var(_)
            | TyKind::Primitive(_)
            | TyKind::Generic(_)
            | TyKind::SelfTy(_)
            | TyKind::Unit
            | TyKind::Never => false,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::diag::DiagCtx;
    use crate::hir::Hir;
    use crate::nameres::PrimTy;
    use crate::testing::resolve_src;

    // -----------------------------------------------------------------
    // match_ty
    // -----------------------------------------------------------------

    fn param(n: usize) -> HirId {
        DefId::from_usize(n).owner_id()
    }

    fn def(n: usize) -> DefId {
        DefId::from_usize(n)
    }

    const FOO: usize = 1;
    const BAR: usize = 2;

    #[test]
    fn a_parameter_binds_to_whatever_the_goal_has_there() {
        let mut tcx = TyCtx::new();
        let t = tcx.mk_generic(param(10));
        let i32_ty = tcx.mk_prim(PrimTy::I32);
        let impl_ty = tcx.mk_adt(def(FOO), vec![t]);
        let goal_ty = tcx.mk_adt(def(FOO), vec![i32_ty]);

        let mut subst = HashMap::new();
        assert!(match_ty(&tcx, &[param(10)], impl_ty, goal_ty, &mut subst));
        assert_eq!(subst, HashMap::from([(param(10), i32_ty)]));
    }

    #[test]
    fn a_parameter_used_twice_must_bind_to_the_same_type() {
        let mut tcx = TyCtx::new();
        let t = tcx.mk_generic(param(10));
        let (i32_ty, bool_ty) = (tcx.mk_prim(PrimTy::I32), tcx.mk_prim(PrimTy::Bool));
        let impl_ty = tcx.mk_adt(def(FOO), vec![t, t]);
        let consistent = tcx.mk_adt(def(FOO), vec![i32_ty, i32_ty]);
        let inconsistent = tcx.mk_adt(def(FOO), vec![i32_ty, bool_ty]);

        assert!(match_ty(
            &tcx,
            &[param(10)],
            impl_ty,
            consistent,
            &mut HashMap::new()
        ));
        assert!(!match_ty(
            &tcx,
            &[param(10)],
            impl_ty,
            inconsistent,
            &mut HashMap::new()
        ));
    }

    /// The asymmetry that makes this matching rather than unification: a parameter on the *goal*
    /// side is a rigid constant, not something to bind.
    #[test]
    fn matching_is_one_way() {
        let mut tcx = TyCtx::new();
        let t = tcx.mk_generic(param(10));
        let i32_ty = tcx.mk_prim(PrimTy::I32);
        let open = tcx.mk_adt(def(FOO), vec![t]);
        let closed = tcx.mk_adt(def(FOO), vec![i32_ty]);

        assert!(match_ty(
            &tcx,
            &[param(10)],
            open,
            closed,
            &mut HashMap::new()
        ));
        assert!(
            !match_ty(&tcx, &[param(10)], closed, open, &mut HashMap::new()),
            "`i32` does not match `T`; only the impl side may bind"
        );
    }

    #[test]
    fn a_generic_that_is_not_the_impls_own_parameter_is_rigid() {
        let mut tcx = TyCtx::new();
        let outer = tcx.mk_generic(param(20));
        let i32_ty = tcx.mk_prim(PrimTy::I32);
        let impl_ty = tcx.mk_adt(def(FOO), vec![outer]);
        let concrete = tcx.mk_adt(def(FOO), vec![i32_ty]);
        let same = tcx.mk_adt(def(FOO), vec![outer]);

        // The impl declares `param(10)`, not `param(20)`, so `outer` may not be bound.
        assert!(!match_ty(
            &tcx,
            &[param(10)],
            impl_ty,
            concrete,
            &mut HashMap::new()
        ));
        assert!(match_ty(
            &tcx,
            &[param(10)],
            impl_ty,
            same,
            &mut HashMap::new()
        ));
    }

    #[test]
    fn a_structural_mismatch_does_not_match() {
        let mut tcx = TyCtx::new();
        let i32_ty = tcx.mk_prim(PrimTy::I32);
        let foo = tcx.mk_adt(def(FOO), vec![i32_ty]);
        let bar = tcx.mk_adt(def(BAR), vec![i32_ty]);
        let arity = tcx.mk_adt(def(FOO), vec![i32_ty, i32_ty]);

        assert!(!match_ty(&tcx, &[], foo, bar, &mut HashMap::new()));
        assert!(!match_ty(&tcx, &[], foo, arity, &mut HashMap::new()));
    }

    #[test]
    fn matching_recurses_into_nested_arguments() {
        let mut tcx = TyCtx::new();
        let t = tcx.mk_generic(param(10));
        let i32_ty = tcx.mk_prim(PrimTy::I32);
        let bar_t = tcx.mk_adt(def(BAR), vec![t]);
        let bar_i32 = tcx.mk_adt(def(BAR), vec![i32_ty]);
        let impl_ty = tcx.mk_adt(def(FOO), vec![bar_t]);
        let goal_ty = tcx.mk_adt(def(FOO), vec![bar_i32]);

        let mut subst = HashMap::new();
        assert!(match_ty(&tcx, &[param(10)], impl_ty, goal_ty, &mut subst));
        assert_eq!(subst[&param(10)], i32_ty);
    }

    /// A parameter binds to whatever is there, including a whole composite.
    #[test]
    fn a_parameter_binds_to_a_composite() {
        let mut tcx = TyCtx::new();
        let t = tcx.mk_generic(param(10));
        let i32_ty = tcx.mk_prim(PrimTy::I32);
        let bar_i32 = tcx.mk_adt(def(BAR), vec![i32_ty]);
        let impl_ty = tcx.mk_adt(def(FOO), vec![t]);
        let goal_ty = tcx.mk_adt(def(FOO), vec![bar_i32]);

        let mut subst = HashMap::new();
        assert!(match_ty(&tcx, &[param(10)], impl_ty, goal_ty, &mut subst));
        assert_eq!(subst[&param(10)], bar_i32);
    }

    #[test]
    fn an_error_type_matches_nothing() {
        let mut tcx = TyCtx::new();
        let error = tcx.error();
        let i32_ty = tcx.mk_prim(PrimTy::I32);

        assert!(!match_ty(&tcx, &[], error, i32_ty, &mut HashMap::new()));
        assert!(!match_ty(&tcx, &[], i32_ty, error, &mut HashMap::new()));
    }

    // -----------------------------------------------------------------
    // implements
    // -----------------------------------------------------------------

    /// Collects `src` and builds the impl index, which is everything the query reads.
    fn solver<'hir>(hir: &'hir Hir) -> Typeck<'hir> {
        let mut checker = Typeck::new(hir);
        checker.collect_module(hir.root_id());
        checker.build_impl_index();
        DiagCtx::clear();
        checker
    }

    fn messages() -> Vec<String> {
        DiagCtx::diagnostics()
            .into_iter()
            .map(|diagnostic| diagnostic.message)
            .collect()
    }

    /// The `DefId` of the top-level definition named `name`.
    fn named(checker: &Typeck<'_>, name: &str) -> DefId {
        crate::testing::named_def(checker.hir, name)
    }

    fn goal(checker: &mut Typeck<'_>, self_ty: Ty, trait_def: DefId) -> Obligation {
        Obligation::new(
            self_ty,
            TraitRef {
                def: trait_def,
                args: Vec::new(),
            },
            SrcSpan::new(0, 0),
        )
    }

    const SRC: &str = "trait Show { fun show(&self); }
                       struct Foo {}
                       struct Bare {}
                       extend Foo with Show { fun show(&self) {} }";

    #[test]
    fn a_matching_impl_proves_the_goal() {
        let hir = resolve_src(SRC);
        let mut checker = solver(&hir);
        let (foo, show) = (named(&checker, "Foo"), named(&checker, "Show"));
        let foo_ty = checker.tcx.mk_adt(foo, vec![]);

        let goal = goal(&mut checker, foo_ty, show);
        assert_eq!(
            checker.implements(&goal, &ParamEnv::empty()),
            Solution::Holds
        );
    }

    #[test]
    fn a_type_with_no_impl_does_not_implement() {
        let hir = resolve_src(SRC);
        let mut checker = solver(&hir);
        let (bare, show) = (named(&checker, "Bare"), named(&checker, "Show"));
        let bare_ty = checker.tcx.mk_adt(bare, vec![]);

        let goal = goal(&mut checker, bare_ty, show);
        assert_eq!(
            checker.implements(&goal, &ParamEnv::empty()),
            Solution::DoesNotHold
        );
    }

    /// A goal whose self type is still an inference variable is not "no" -- it is "not yet".
    #[test]
    fn an_unresolved_self_type_is_ambiguous() {
        let hir = resolve_src(SRC);
        let mut checker = solver(&hir);
        let show = named(&checker, "Show");
        let var = checker.tcx.next_ty_var();

        let goal = goal(&mut checker, var, show);
        assert_eq!(
            checker.implements(&goal, &ParamEnv::empty()),
            Solution::Ambiguous
        );
        assert!(messages().is_empty(), "an ambiguity is not a diagnostic");
    }

    #[test]
    fn a_goal_containing_an_error_answers_error_without_reporting() {
        let hir = resolve_src(SRC);
        let mut checker = solver(&hir);
        let show = named(&checker, "Show");
        let error = checker.tcx.error();

        let goal = goal(&mut checker, error, show);
        assert_eq!(
            checker.implements(&goal, &ParamEnv::empty()),
            Solution::Error
        );
        assert!(
            messages().is_empty(),
            "a diagnostic for the error type already exists"
        );
    }

    /// Nothing but a struct, an enum, or a `dyn` can implement anything -- a reference to a type
    /// that implements `Show` does not itself implement it.
    #[test]
    fn a_reference_implements_nothing() {
        use crate::ast::Mutability;

        let hir = resolve_src(SRC);
        let mut checker = solver(&hir);
        let (foo, show) = (named(&checker, "Foo"), named(&checker, "Show"));
        let foo_ty = checker.tcx.mk_adt(foo, vec![]);
        let ref_ty = checker.tcx.mk_ref(foo_ty, Mutability::Immutable);

        let goal = goal(&mut checker, ref_ty, show);
        assert_eq!(
            checker.implements(&goal, &ParamEnv::empty()),
            Solution::DoesNotHold
        );
    }

    #[test]
    fn dyn_implements_exactly_the_trait_it_names() {
        let hir = resolve_src(
            "trait Show { fun show(&self); }
             trait Other { fun other(&self); }",
        );
        let mut checker = solver(&hir);
        let (show, other) = (named(&checker, "Show"), named(&checker, "Other"));
        let dyn_show = checker.tcx.mk_dyn(show, vec![]);

        let its_own = goal(&mut checker, dyn_show, show);
        assert_eq!(
            checker.implements(&its_own, &ParamEnv::empty()),
            Solution::Holds
        );

        let another = goal(&mut checker, dyn_show, other);
        assert_eq!(
            checker.implements(&another, &ParamEnv::empty()),
            Solution::DoesNotHold
        );
    }

    /// The environment is consulted before the index, and it is the only thing that can answer a
    /// goal about a bare type parameter.
    #[test]
    fn a_bound_in_the_environment_proves_a_goal_about_a_parameter() {
        let hir = resolve_src(
            "trait Show { fun show(&self); }
             fun f<T: Show>(x: T) {}",
        );
        let mut checker = solver(&hir);
        let (f, show) = (named(&checker, "f"), named(&checker, "Show"));
        let function = hir.function(f);
        let param = function.generics[0];
        let t = checker.tcx.mk_generic(param);

        let env = checker.param_env(f);
        assert_eq!(env.bounds.len(), 1, "`T: Show` is the only bound in scope");

        let goal = goal(&mut checker, t, show);
        assert_eq!(checker.implements(&goal, &env), Solution::Holds);
    }

    #[test]
    fn a_parameter_with_no_bound_implements_nothing() {
        let hir = resolve_src(
            "trait Show { fun show(&self); }
             fun f<T>(x: T) {}",
        );
        let mut checker = solver(&hir);
        let (f, show) = (named(&checker, "f"), named(&checker, "Show"));
        let function = hir.function(f);
        let t = checker.tcx.mk_generic(function.generics[0]);

        let env = checker.param_env(f);
        let goal = goal(&mut checker, t, show);
        assert_eq!(checker.implements(&goal, &env), Solution::DoesNotHold);
    }

    /// Inside a trait, `Self` implements that trait by definition.
    #[test]
    fn a_traits_own_self_implements_it() {
        let hir = resolve_src("trait Show { fun show(&self); }");
        let mut checker = solver(&hir);
        let show = named(&checker, "Show");
        let self_ty = checker.tcx.mk_self_param(show);

        let env = checker.param_env(show);
        let goal = goal(&mut checker, self_ty, show);
        assert_eq!(checker.implements(&goal, &env), Solution::Holds);
    }

    /// A method sees the bounds of the `extend` block it is declared in, not just its own.
    #[test]
    fn a_method_inherits_its_extend_blocks_bounds() {
        let hir = resolve_src(
            "trait Show { fun show(&self); }
             struct Wrap<T> { inner: T }
             extend<T: Show> Wrap<T> { fun get(&self) {} }",
        );
        let mut checker = solver(&hir);
        let extend = crate::testing::first_extend(&hir);
        let block = hir.extend(extend);

        let env = checker.param_env(block.methods[0]);
        assert_eq!(
            env.bounds.len(),
            1,
            "the method itself declares nothing, so its only bound comes from the block"
        );
    }

    /// A conditional impl is honored: `Wrap<T>: Show` holds exactly when `T: Show` does.
    #[test]
    fn a_conditional_impls_own_bounds_are_proved_recursively() {
        let hir = resolve_src(
            "trait Show { fun show(&self); }
             struct Wrap<T> { inner: T }
             struct Foo {}
             struct Bare {}
             extend Foo with Show { fun show(&self) {} }
             extend<T: Show> Wrap<T> with Show { fun show(&self) {} }",
        );
        let mut checker = solver(&hir);
        let (show, wrap) = (named(&checker, "Show"), named(&checker, "Wrap"));
        let (foo, bare) = (named(&checker, "Foo"), named(&checker, "Bare"));

        let foo_ty = checker.tcx.mk_adt(foo, vec![]);
        let bare_ty = checker.tcx.mk_adt(bare, vec![]);
        let wrap_foo = checker.tcx.mk_adt(wrap, vec![foo_ty]);
        let wrap_bare = checker.tcx.mk_adt(wrap, vec![bare_ty]);

        let holds = goal(&mut checker, wrap_foo, show);
        assert_eq!(
            checker.implements(&holds, &ParamEnv::empty()),
            Solution::Holds
        );

        // `Bare: Show` fails, so `Wrap<Bare>: Show` fails with it.
        let fails = goal(&mut checker, wrap_bare, show);
        assert_eq!(
            checker.implements(&fails, &ParamEnv::empty()),
            Solution::DoesNotHold
        );
    }

    // -----------------------------------------------------------------
    // Termination
    //
    // Neither cutoff is reachable from source today: a bound may only be written on the impl's
    // own parameters, and matching binds those to strict subterms of the goal, so every sub-goal
    // is smaller than its parent. Both are exercised directly against the in-progress stack, so
    // that they still work the day a language feature makes them reachable.
    // -----------------------------------------------------------------

    #[test]
    fn a_goal_already_in_progress_is_reported_as_a_cycle() {
        let hir = resolve_src(SRC);
        let mut checker = solver(&hir);
        let (foo, show) = (named(&checker, "Foo"), named(&checker, "Show"));
        let foo_ty = checker.tcx.mk_adt(foo, vec![]);
        let goal = goal(&mut checker, foo_ty, show);

        checker.goal_stack.push(goal.clone());
        assert_eq!(
            checker.implements(&goal, &ParamEnv::empty()),
            Solution::Error
        );
        assert_eq!(
            messages(),
            ["cyclic trait bound: proving `Foo: Show` requires proving it again"]
        );
    }

    /// The same goal reached from a different place is still the same goal: the cause span is not
    /// part of what the cycle check compares.
    #[test]
    fn the_cycle_check_ignores_where_the_goal_was_raised() {
        let hir = resolve_src(SRC);
        let mut checker = solver(&hir);
        let (foo, show) = (named(&checker, "Foo"), named(&checker, "Show"));
        let foo_ty = checker.tcx.mk_adt(foo, vec![]);

        let mut open = goal(&mut checker, foo_ty, show);
        open.cause = SrcSpan::new(100, 110);
        checker.goal_stack.push(open);

        let asked = goal(&mut checker, foo_ty, show);
        assert_eq!(
            checker.implements(&asked, &ParamEnv::empty()),
            Solution::Error
        );
    }

    #[test]
    fn a_goal_deeper_than_the_recursion_limit_is_reported() {
        let hir = resolve_src(SRC);
        let mut checker = solver(&hir);
        let (foo, show) = (named(&checker, "Foo"), named(&checker, "Show"));
        let foo_ty = checker.tcx.mk_adt(foo, vec![]);

        // Distinct goals, so the cycle check cannot fire first: each is about a fresh inference
        // variable, which nothing else in the program is ever about.
        for _ in 0..RECURSION_LIMIT {
            let var = checker.tcx.next_ty_var();
            let filler = goal(&mut checker, var, show);
            checker.goal_stack.push(filler);
        }

        let goal = goal(&mut checker, foo_ty, show);
        assert_eq!(
            checker.implements(&goal, &ParamEnv::empty()),
            Solution::Error
        );
        assert_eq!(
            messages(),
            ["recursion limit reached while proving `Foo: Show`"]
        );
    }

    /// One below the limit still answers the question rather than giving up.
    #[test]
    fn a_goal_just_inside_the_recursion_limit_is_answered() {
        let hir = resolve_src(SRC);
        let mut checker = solver(&hir);
        let (foo, show) = (named(&checker, "Foo"), named(&checker, "Show"));
        let foo_ty = checker.tcx.mk_adt(foo, vec![]);

        for _ in 0..RECURSION_LIMIT - 1 {
            let var = checker.tcx.next_ty_var();
            let filler = goal(&mut checker, var, show);
            checker.goal_stack.push(filler);
        }

        let goal = goal(&mut checker, foo_ty, show);
        assert_eq!(
            checker.implements(&goal, &ParamEnv::empty()),
            Solution::Holds
        );
        assert!(messages().is_empty(), "{:?}", messages());
    }

    /// The stack is unwound as the recursion returns, so one query does not make the next one
    /// look deeper than it is.
    #[test]
    fn the_in_progress_stack_is_empty_once_a_query_returns() {
        let hir = resolve_src(SRC);
        let mut checker = solver(&hir);
        let (foo, show) = (named(&checker, "Foo"), named(&checker, "Show"));
        let foo_ty = checker.tcx.mk_adt(foo, vec![]);

        let goal = goal(&mut checker, foo_ty, show);
        checker.implements(&goal, &ParamEnv::empty());
        assert!(checker.goal_stack.is_empty());
    }

    // -----------------------------------------------------------------
    // extends
    // -----------------------------------------------------------------

    /// `TraitName::Def` reaches the same solver path as an operator's `TraitName::Lang` does,
    /// without consulting `Hir::lang_items` at all.
    #[test]
    fn extends_answers_for_a_user_written_trait() {
        let hir = resolve_src(SRC);
        let mut checker = solver(&hir);
        let (foo, show) = (named(&checker, "Foo"), named(&checker, "Show"));
        let foo_ty = checker.tcx.mk_adt(foo, vec![]);

        assert_eq!(
            checker.extends(
                foo_ty,
                TraitName::Def(show),
                Vec::new(),
                hir.root_id(),
                SrcSpan::new(0, 0),
            ),
            Solution::Holds
        );
    }

    #[test]
    fn extends_reports_no_for_a_user_written_trait_with_no_impl() {
        let hir = resolve_src(SRC);
        let mut checker = solver(&hir);
        let (bare, show) = (named(&checker, "Bare"), named(&checker, "Show"));
        let bare_ty = checker.tcx.mk_adt(bare, vec![]);

        assert_eq!(
            checker.extends(
                bare_ty,
                TraitName::Def(show),
                Vec::new(),
                hir.root_id(),
                SrcSpan::new(0, 0),
            ),
            Solution::DoesNotHold
        );
    }

    /// `SRC` declares no core library, so `LangItems::get` returns `None` for every item.
    /// `extends` maps that to `Solution::Error` rather than `Solution::DoesNotHold`, so the
    /// caller suppresses its diagnostic instead of reporting `Foo` as lacking an `Add` impl.
    #[test]
    fn extends_treats_an_unresolved_lang_item_as_already_reported() {
        let hir = resolve_src(SRC);
        let mut checker = solver(&hir);
        let foo = named(&checker, "Foo");
        let foo_ty = checker.tcx.mk_adt(foo, vec![]);

        assert_eq!(
            checker.extends(
                foo_ty,
                TraitName::Lang(crate::langitems::LangItem::Add),
                Vec::new(),
                hir.root_id(),
                SrcSpan::new(0, 0),
            ),
            Solution::Error
        );
    }

    /// `Solution::DoesNotHold` produces exactly one diagnostic, containing the self type's
    /// rendered name and the trait's declared name.
    #[test]
    fn require_extends_reports_the_type_and_the_trait_by_name() {
        let hir = resolve_src(SRC);
        let mut checker = solver(&hir);
        let (bare, show) = (named(&checker, "Bare"), named(&checker, "Show"));
        let bare_ty = checker.tcx.mk_adt(bare, vec![]);

        let held = checker.require_extends(
            bare_ty,
            TraitName::Def(show),
            Vec::new(),
            hir.root_id(),
            SrcSpan::new(0, 0),
            "this index",
        );

        assert!(!held);
        let messages = messages();
        assert_eq!(messages.len(), 1, "{messages:?}");
        assert!(
            messages[0].contains("Bare") && messages[0].contains("Show"),
            "{messages:?}"
        );
    }

    /// `Ambiguous` and `Error` both return `false` without emitting: an unresolved `TyKind::Var`
    /// may still resolve to a type that implements the trait, and an unresolved lang item was
    /// already reported by `langitems::collect_ast`.
    #[test]
    fn require_extends_stays_quiet_when_the_question_has_no_answer() {
        let hir = resolve_src(SRC);
        let mut checker = solver(&hir);
        let show = named(&checker, "Show");
        let var = checker.tcx.next_ty_var();

        // Ambiguous: the self type is still an inference variable.
        assert!(!checker.require_extends(
            var,
            TraitName::Def(show),
            Vec::new(),
            hir.root_id(),
            SrcSpan::new(0, 0),
            "this operator",
        ));
        // Error: an unresolved lang item, already reported by `langitems::collect_ast`.
        let foo_ty = {
            let foo = named(&checker, "Foo");
            checker.tcx.mk_adt(foo, vec![])
        };
        assert!(!checker.require_extends(
            foo_ty,
            TraitName::Lang(crate::langitems::LangItem::Add),
            Vec::new(),
            hir.root_id(),
            SrcSpan::new(0, 0),
            "this operator",
        ));

        assert!(messages().is_empty(), "{:?}", messages());
    }

    /// `extends` performs no matching of its own: for the same self type, trait, and owner, it
    /// returns exactly what `implements` returns for the goal and `ParamEnv` built by hand.
    #[test]
    fn extends_agrees_with_implements() {
        let hir = resolve_src(SRC);
        let mut checker = solver(&hir);
        let (foo, show) = (named(&checker, "Foo"), named(&checker, "Show"));
        let foo_ty = checker.tcx.mk_adt(foo, vec![]);

        let direct = {
            let goal = goal(&mut checker, foo_ty, show);
            let env = checker.param_env(hir.root_id());
            checker.implements(&goal, &env)
        };
        let through_extends = checker.extends(
            foo_ty,
            TraitName::Def(show),
            Vec::new(),
            hir.root_id(),
            SrcSpan::new(0, 0),
        );

        assert_eq!(direct, through_extends);
    }
}
