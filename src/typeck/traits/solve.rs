use std::collections::HashMap;
use std::iter::once;

use crate::hir::{DefId, HirId, OwnerNode, Res, TyDef, Type};
use crate::typeck::Typeck;
use crate::typeck::fold;
use crate::typeck::traits::TraitRef;
use crate::typeck::ty::{Ty, TyKind};
use crate::typeck::tyctx::TyCtx;

/// A goal: does `self_ty` implement `trait_`?
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct Query {
    pub self_ty: Ty,
    pub trait_: TraitRef,
}

impl Query {
    pub fn new(self_ty: Ty, trait_def: DefId) -> Self {
        Query {
            self_ty,
            trait_: TraitRef {
                def: trait_def,
                args: Vec::new(),
            },
        }
    }

    /// Applies `f` to every type the query mentions.
    fn map(&self, f: &mut impl FnMut(Ty) -> Ty) -> Query {
        Query {
            self_ty: f(self.self_ty),
            trait_: TraitRef {
                def: self.trait_.def,
                args: self.trait_.args.iter().map(|&arg| f(arg)).collect(),
            },
        }
    }
}

/// The trait bounds in scope.
#[derive(Clone, Debug, Default)]
pub struct BoundsEnv {
    pub bounds: Vec<Query>,
}

/// The answer to a query.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Solution {
    Holds,
    DoesNotHold,
    /// The query still contains inference variables, so it can be neither proved nor disproved
    /// yet. Ask again once more of the body has been checked.
    Ambiguous,
    /// The goal contained [`TyKind::Error`]. A diagnostic for this already exists.
    Error,
}

/// Matches the open type `open` against the closed type `closed`, recording in `subst` what
/// each of the header's own parameters had to be. A parameter in `generics` binds on first
/// sight and must agree with itself on every later sight; anything else is an ordinary rigid
/// constant, matching only structurally identical types.
///
/// Nothing in `closed` is ever bound: matching is one-way, which is what keeps candidate
/// selection from constraining the caller's own inference variables.
pub fn match_ty(
    tcx: &TyCtx,
    generics: &[HirId],
    open: Ty,
    closed: Ty,
    subst: &mut HashMap<HirId, Ty>,
) -> bool {
    if let TyKind::Generic(param) = *tcx.kind(open)
        && generics.contains(&param)
    {
        return match subst.get(&param) {
            Some(&bound) => bound == closed,
            None => {
                subst.insert(param, closed);
                true
            }
        };
    }

    // Identical handles are identical types.
    if open == closed {
        return true;
    }

    if matches!(tcx.kind(open), TyKind::Error) || matches!(tcx.kind(closed), TyKind::Error) {
        return false;
    }

    // Recurse into the components of the types.
    fold::decompose(tcx, open, closed).is_some_and(|components| {
        components
            .into_iter()
            .all(|(x, y)| match_ty(tcx, generics, x, y, subst))
    })
}

impl<'hir> Typeck<'hir> {
    /// Whether `query` holds, given the bounds already in scope in `env`.
    pub fn implements(&mut self, query: &Query, env: &BoundsEnv) -> Solution {
        // Resolves any inference variables in the query before comparing it against anything.
        let query = self.resolve_query(query);

        if self.goal_mentions_error(&query) {
            return Solution::Error;
        }
        if matches!(self.tcx.kind(query.self_ty), TyKind::Var(_)) {
            return Solution::Ambiguous;
        }

        // Already an assumption in scope, verbatim.
        if env.bounds.contains(&query) {
            return Solution::Holds;
        }

        // A `dyn Foo<T>` value satisfies exactly `Foo<T>`: the query holds only if it names the
        // same trait, applied to the same arguments, that the `dyn` itself carries.
        if let TyKind::Dyn { trait_, args } = self.tcx.kind(query.self_ty) {
            let holds = *trait_ == query.trait_.def && *args == query.trait_.args;
            return if holds {
                Solution::Holds
            } else {
                Solution::DoesNotHold
            };
        }

        // Only a struct, an enum, or (handled above) a `dyn` can implement a trait, so anything
        // else answers no.
        let TyKind::Adt { def, .. } = *self.tcx.kind(query.self_ty) else {
            return Solution::DoesNotHold;
        };

        // Looks for an extend block in the index that proves the query.
        let Some((block, subst)) = self.search_index(def, &query) else {
            return Solution::DoesNotHold;
        };

        // The block's own bounds have to hold: `extend<T: Show> Wrap<T> with Show` only
        // proves the goal if `T` actually implements `Show`.
        let obligations = self.bounds_env(block).bounds;
        for obligation in obligations {
            let sub_goal = self.subst_query(&obligation, &subst);
            match self.implements(&sub_goal, env) {
                Solution::Holds => {}
                // Propagate the error up
                answer => return answer,
            }
        }

        Solution::Holds
    }

    /// The first block in the index that proves `goal`, and what its parameters had to be.
    fn search_index(&self, head: DefId, goal: &Query) -> Option<(DefId, HashMap<HirId, Ty>)> {
        self.extends
            .for_type(head)
            .iter()
            .find_map(|&block| Some((block, self.header_proves(block, goal)?)))
    }

    fn header_proves(&self, block: DefId, goal: &Query) -> Option<HashMap<HirId, Ty>> {
        // Whether the block implements a trait at all.
        let trait_ = self.extends.trait_of(block)?;

        // Whether it is the trait being asked about, applied to as many arguments.
        if trait_.def != goal.trait_.def || trait_.args.len() != goal.trait_.args.len() {
            return None;
        }

        // Whether the extended type and every trait argument match, under one substitution.
        let extended = (self.adt_of_with_args(block), goal.self_ty);
        let args = trait_
            .args
            .iter()
            .copied()
            .zip(goal.trait_.args.iter().copied());
        self.match_header(block, once(extended).chain(args))
    }

    /// Whether `block`'s header applies to `self_ty`, used to search the index for a header
    /// that can prove a query.
    pub(crate) fn header_applies(&self, block: DefId, self_ty: Ty) -> Option<HashMap<HirId, Ty>> {
        self.match_header(block, once((self.adt_of_with_args(block), self_ty)))
    }

    /// Matches each of `block`'s header types against the closed type beside it.
    fn match_header(
        &self,
        block: DefId,
        mut pairs: impl Iterator<Item = (Ty, Ty)>,
    ) -> Option<HashMap<HirId, Ty>> {
        let generics = self.declared_generics(block);
        let mut subst = HashMap::new();
        pairs
            .all(|(open, closed)| match_ty(&self.tcx, generics, open, closed, &mut subst))
            .then_some(subst)
    }

    /// The bounds in scope for `owner`, collected by walking outward through its enclosing
    /// definitions.
    pub fn bounds_env(&mut self, owner: DefId) -> BoundsEnv {
        let mut bounds = Vec::new();
        let mut current = Some(owner);
        while let Some(owner) = current {
            let generics = self.declared_generics(owner);
            for &generic in generics {
                bounds.extend(self.bounds_of(generic));
            }

            // `Self` implements the trait it is declared inside.
            if matches!(self.hir.def(owner), OwnerNode::Trait(_)) {
                let self_ty = self.tcx.mk_self_param(owner);
                let args = generics.iter().map(|&id| self.tcx.mk_generic(id)).collect();
                bounds.push(Query {
                    self_ty,
                    trait_: TraitRef { def: owner, args },
                });
            }

            current = self.hir.parent(owner);
        }

        BoundsEnv { bounds }
    }

    /// The trait bounds declared on `generic`, e.g. `Show` for `T: Show`.
    pub(crate) fn bounds_of(&mut self, generic: HirId) -> Vec<Query> {
        let hir: &'hir crate::hir::Hir = self.hir;
        let self_ty = self.tcx.mk_generic(generic);

        hir.generic(generic)
            .bounds
            .iter()
            .filter_map(|path| match path.res {
                Res::Type(Type::Def(TyDef::Trait(def))) => Some(Query::new(self_ty, def)),
                _ => None,
            })
            .collect()
    }

    /// Rebuilds `goal` with every parameter in `subst` replaced by what it is bound to.
    pub(crate) fn subst_query(&mut self, query: &Query, subst: &HashMap<HirId, Ty>) -> Query {
        query.map(&mut |ty| fold::subst_ty(&mut self.tcx, ty, subst))
    }

    /// Rebuilds `ty` with every parameter in `subst` replaced by what it is bound to.
    pub fn subst_ty(&mut self, ty: Ty, subst: &HashMap<HirId, Ty>) -> Ty {
        fold::subst_ty(&mut self.tcx, ty, subst)
    }

    /// Rebuilds `query` with every inference variable in it replaced by whatever it has since
    /// resolved to.
    fn resolve_query(&mut self, goal: &Query) -> Query {
        goal.map(&mut |ty| self.unifier.find_deep(&mut self.tcx, ty))
    }

    /// Whether any part of the goal is [`TyKind::Error`].
    fn goal_mentions_error(&self, goal: &Query) -> bool {
        let mut tys = std::iter::once(goal.self_ty).chain(goal.trait_.args.iter().copied());
        tys.any(|ty| fold::mentions_error(&self.tcx, ty))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::diagnostics::DiagCtx;
    use crate::hir::Hir;
    use crate::nameres::PrimTy;
    use crate::testing::{Stage, checker_through, messages, resolve_src};

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
        let open_ty = tcx.mk_adt(def(FOO), vec![t]);
        let goal_ty = tcx.mk_adt(def(FOO), vec![i32_ty]);

        let mut subst = HashMap::new();
        assert!(match_ty(&tcx, &[param(10)], open_ty, goal_ty, &mut subst));
        assert_eq!(subst, HashMap::from([(param(10), i32_ty)]));
    }

    #[test]
    fn a_parameter_used_twice_must_bind_to_the_same_type() {
        let mut tcx = TyCtx::new();
        let t = tcx.mk_generic(param(10));
        let (i32_ty, bool_ty) = (tcx.mk_prim(PrimTy::I32), tcx.mk_prim(PrimTy::Bool));
        let open_ty = tcx.mk_adt(def(FOO), vec![t, t]);
        let consistent = tcx.mk_adt(def(FOO), vec![i32_ty, i32_ty]);
        let inconsistent = tcx.mk_adt(def(FOO), vec![i32_ty, bool_ty]);

        assert!(match_ty(
            &tcx,
            &[param(10)],
            open_ty,
            consistent,
            &mut HashMap::new()
        ));
        assert!(!match_ty(
            &tcx,
            &[param(10)],
            open_ty,
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
            "`i32` does not match `T`; only the open side may bind"
        );
    }

    #[test]
    fn a_generic_that_is_not_the_headers_own_parameter_is_rigid() {
        let mut tcx = TyCtx::new();
        let outer = tcx.mk_generic(param(20));
        let i32_ty = tcx.mk_prim(PrimTy::I32);
        let open_ty = tcx.mk_adt(def(FOO), vec![outer]);
        let concrete = tcx.mk_adt(def(FOO), vec![i32_ty]);
        let same = tcx.mk_adt(def(FOO), vec![outer]);

        // The header declares `param(10)`, not `param(20)`, so `outer` may not be bound.
        assert!(!match_ty(
            &tcx,
            &[param(10)],
            open_ty,
            concrete,
            &mut HashMap::new()
        ));
        assert!(match_ty(
            &tcx,
            &[param(10)],
            open_ty,
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
        let open_ty = tcx.mk_adt(def(FOO), vec![bar_t]);
        let goal_ty = tcx.mk_adt(def(FOO), vec![bar_i32]);

        let mut subst = HashMap::new();
        assert!(match_ty(&tcx, &[param(10)], open_ty, goal_ty, &mut subst));
        assert_eq!(subst[&param(10)], i32_ty);
    }

    /// A parameter binds to whatever is there, including a whole composite.
    #[test]
    fn a_parameter_binds_to_a_composite() {
        let mut tcx = TyCtx::new();
        let t = tcx.mk_generic(param(10));
        let i32_ty = tcx.mk_prim(PrimTy::I32);
        let bar_i32 = tcx.mk_adt(def(BAR), vec![i32_ty]);
        let open_ty = tcx.mk_adt(def(FOO), vec![t]);
        let goal_ty = tcx.mk_adt(def(FOO), vec![bar_i32]);

        let mut subst = HashMap::new();
        assert!(match_ty(&tcx, &[param(10)], open_ty, goal_ty, &mut subst));
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

    /// Collects `src` and builds the extend index, which is everything the query reads.
    fn solver<'hir>(hir: &'hir Hir) -> Typeck<'hir> {
        let checker = checker_through(hir, Stage::Index);
        DiagCtx::clear();
        checker
    }

    /// The `DefId` of the top-level definition named `name`.
    fn named(checker: &Typeck<'_>, name: &str) -> DefId {
        crate::testing::named_def(checker.hir, name)
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

        let goal = Query::new(foo_ty, show);
        assert_eq!(
            checker.implements(&goal, &BoundsEnv::default()),
            Solution::Holds
        );
    }

    #[test]
    fn a_type_with_no_impl_does_not_implement() {
        let hir = resolve_src(SRC);
        let mut checker = solver(&hir);
        let (bare, show) = (named(&checker, "Bare"), named(&checker, "Show"));
        let bare_ty = checker.tcx.mk_adt(bare, vec![]);

        let goal = Query::new(bare_ty, show);
        assert_eq!(
            checker.implements(&goal, &BoundsEnv::default()),
            Solution::DoesNotHold
        );
    }

    /// A goal whose self type is still an inference variable is not "no", it is "not yet".
    #[test]
    fn an_unresolved_self_type_is_ambiguous() {
        let hir = resolve_src(SRC);
        let mut checker = solver(&hir);
        let show = named(&checker, "Show");
        let var = checker.tcx.next_ty_var();

        let goal = Query::new(var, show);
        assert_eq!(
            checker.implements(&goal, &BoundsEnv::default()),
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

        let goal = Query::new(error, show);
        assert_eq!(
            checker.implements(&goal, &BoundsEnv::default()),
            Solution::Error
        );
        assert!(
            messages().is_empty(),
            "a diagnostic for the error type already exists"
        );
    }

    /// Nothing but a struct, an enum, or a `dyn` can implement anything: a reference to a type
    /// that implements `Show` does not itself implement it.
    #[test]
    fn a_reference_implements_nothing() {
        use crate::ast::Mutability;

        let hir = resolve_src(SRC);
        let mut checker = solver(&hir);
        let (foo, show) = (named(&checker, "Foo"), named(&checker, "Show"));
        let foo_ty = checker.tcx.mk_adt(foo, vec![]);
        let ref_ty = checker.tcx.mk_ref(foo_ty, Mutability::Immutable);

        let goal = Query::new(ref_ty, show);
        assert_eq!(
            checker.implements(&goal, &BoundsEnv::default()),
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

        let its_own = Query::new(dyn_show, show);
        assert_eq!(
            checker.implements(&its_own, &BoundsEnv::default()),
            Solution::Holds
        );

        let another = Query::new(dyn_show, other);
        assert_eq!(
            checker.implements(&another, &BoundsEnv::default()),
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

        let env = checker.bounds_env(f);
        assert_eq!(env.bounds.len(), 1, "`T: Show` is the only bound in scope");

        let goal = Query::new(t, show);
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

        let env = checker.bounds_env(f);
        let goal = Query::new(t, show);
        assert_eq!(checker.implements(&goal, &env), Solution::DoesNotHold);
    }

    /// Inside a trait, `Self` implements that trait by definition.
    #[test]
    fn a_traits_own_self_implements_it() {
        let hir = resolve_src("trait Show { fun show(&self); }");
        let mut checker = solver(&hir);
        let show = named(&checker, "Show");
        let self_ty = checker.tcx.mk_self_param(show);

        let env = checker.bounds_env(show);
        let goal = Query::new(self_ty, show);
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

        let env = checker.bounds_env(block.methods[0]);
        assert_eq!(
            env.bounds.len(),
            1,
            "the method itself declares nothing, so its only bound comes from the block"
        );
    }

    /// A conditional block is honored: `Wrap<T>: Show` holds exactly when `T: Show` does.
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

        let holds = Query::new(wrap_foo, show);
        assert_eq!(
            checker.implements(&holds, &BoundsEnv::default()),
            Solution::Holds
        );

        // `Bare: Show` fails, so `Wrap<Bare>: Show` fails with it.
        let fails = Query::new(wrap_bare, show);
        assert_eq!(
            checker.implements(&fails, &BoundsEnv::default()),
            Solution::DoesNotHold
        );
    }
}
