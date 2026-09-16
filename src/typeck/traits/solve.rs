//! Solving trait goals and resolving `.` accesses: the trait API the rest of type checking
//! calls. The index this reads is built by [`crate::typeck::traits::collect`]; the program-level
//! well-formedness rules live in [`crate::typeck::traits::check`].

use std::collections::HashMap;
use std::iter::once;

use crate::hir::{DefId, HirId, OwnerNode, Res, TyDef, Type};
use crate::typeck::Typeck;
use crate::typeck::traits::TraitRef;
use crate::typeck::traits::collect::TypeHead;
use crate::typeck::ty::ctx::TyCtx;
use crate::typeck::ty::visitor;
use crate::typeck::ty::{Ty, TyKind};

mod method;
mod obligations;

pub(crate) use method::PendingMethodCall;
pub use obligations::Obligation;

/// A goal: does `self_ty` implement `trait_`?
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct Goal {
    pub self_ty: Ty,
    pub trait_: TraitRef,
}

impl Goal {
    pub fn new(self_ty: Ty, trait_def: DefId) -> Self {
        Goal {
            self_ty,
            trait_: TraitRef {
                def: trait_def,
                args: Vec::new(),
            },
        }
    }

    /// Applies `f` to every type the goal mentions.
    fn map(&self, f: &mut impl FnMut(Ty) -> Ty) -> Goal {
        Goal {
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
    pub bounds: Vec<Goal>,
}

/// The answer to a goal.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Solution {
    Holds,
    DoesNotHold,
    /// The goal still contains inference variables, so it can be neither proved nor disproved
    /// yet. Ask again once more of the body has been checked.
    Ambiguous,
    /// The goal contained [`TyKind::Error`]. A diagnostic for this already exists.
    Error,
}

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
    visitor::decompose(tcx, open, closed).is_some_and(|components| {
        components
            .into_iter()
            .all(|(x, y)| match_ty(tcx, generics, x, y, subst))
    })
}

impl<'hir> Typeck<'hir> {
    /// Returns whether `goal` holds, given the bounds already in scope in `env`.
    pub fn implements(&mut self, goal: &Goal, env: &BoundsEnv) -> Solution {
        let goal = self.resolve_goal(goal);

        if self.goal_mentions_error(&goal) {
            return Solution::Error;
        }
        if self.goal_is_unresolved(&goal) {
            return Solution::Ambiguous;
        }
        if env.bounds.contains(&goal) {
            return Solution::Holds;
        }
        if let Some(solution) = self.dyn_goal_solution(&goal) {
            return solution;
        }

        // Only a nominal type -- a struct, an enum, a primitive, or a tuple -- has an `extend`
        // block to look in. Anything else answers no.
        let Some(head) = self.type_head(goal.self_ty) else {
            return Solution::DoesNotHold;
        };
        let Some((block, subst)) = self.find_proving_block(head, &goal) else {
            return Solution::DoesNotHold;
        };
        self.prove_block_bounds(block, &subst, env)
    }

    /// Returns the answer for a `dyn Foo<T>` self type, which satisfies exactly the trait it names.
    fn dyn_goal_solution(&self, goal: &Goal) -> Option<Solution> {
        let TyKind::Dyn { trait_, args } = self.tcx.kind(goal.self_ty) else {
            return None;
        };
        let names_the_trait = *trait_ == goal.trait_.def && *args == goal.trait_.args;
        Some(if names_the_trait {
            Solution::Holds
        } else {
            Solution::DoesNotHold
        })
    }

    /// Returns whether the goal's self type is still an inference variable, so it can be neither
    /// proved nor disproved yet.
    fn goal_is_unresolved(&self, goal: &Goal) -> bool {
        matches!(self.tcx.kind(goal.self_ty), TyKind::Var(_))
    }

    /// Proves the bounds that `block`'s own declaration requires -- `extend<T: Show> Wrap<T>
    /// with Show` only proves the goal if `T` actually implements `Show` -- under the
    /// substitution that made the block apply.
    fn prove_block_bounds(
        &mut self,
        block: DefId,
        subst: &HashMap<HirId, Ty>,
        env: &BoundsEnv,
    ) -> Solution {
        for obligation in self.bounds_env(block).bounds {
            let sub_goal = self.subst_query(&obligation, subst);
            match self.implements(&sub_goal, env) {
                Solution::Holds => {}
                // Propagates `DoesNotHold`, `Ambiguous`, and `Error` up unchanged.
                answer => return answer,
            }
        }
        Solution::Holds
    }

    /// Returns the first block in the index that proves `goal`, and what its parameters had to be.
    fn find_proving_block(
        &self,
        head: TypeHead,
        goal: &Goal,
    ) -> Option<(DefId, HashMap<HirId, Ty>)> {
        self.extends
            .for_type(head)
            .iter()
            .find_map(|&block| Some((block, self.block_proves_goal(block, goal)?)))
    }

    /// Returns whether `block` implements the goal's trait, and if so what its parameters had to
    /// be.
    fn block_proves_goal(&self, block: DefId, goal: &Goal) -> Option<HashMap<HirId, Ty>> {
        let trait_ = self.extends.trait_of(block)?;
        if trait_.def != goal.trait_.def || trait_.args.len() != goal.trait_.args.len() {
            return None;
        }

        // The extended type and every trait argument match under one substitution.
        let extended = (self.extended_type(block), goal.self_ty);
        let args = trait_
            .args
            .iter()
            .copied()
            .zip(goal.trait_.args.iter().copied());
        self.match_block_header(block, once(extended).chain(args))
    }

    /// Returns whether `block`'s header applies to `self_ty`.
    pub(crate) fn header_applies(&self, block: DefId, self_ty: Ty) -> Option<HashMap<HirId, Ty>> {
        self.match_block_header(block, once((self.extended_type(block), self_ty)))
    }

    /// Matches each of `block`'s header types against the closed type beside it.
    fn match_block_header(
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

    /// Returns the bounds in scope for `owner`.
    pub fn bounds_env(&mut self, owner: DefId) -> BoundsEnv {
        let mut bounds = Vec::new();
        let mut current = Some(owner);
        while let Some(owner) = current {
            self.collect_bounds_of(owner, &mut bounds);
            current = self.hir.parent(owner);
        }

        BoundsEnv { bounds }
    }

    /// Adds the bounds one definition puts in scope for the definitions inside it: those its own
    /// parameters declare, and, inside a trait, the implicit `Self: ThisTrait`.
    fn collect_bounds_of(&mut self, owner: DefId, bounds: &mut Vec<Goal>) {
        let generics = self.declared_generics(owner);
        bounds.extend(generics.iter().flat_map(|&generic| self.bounds_of(generic)));

        if matches!(self.hir.def(owner), OwnerNode::Trait(_)) {
            bounds.push(self.self_trait_goal(owner, generics));
        }
    }

    /// Returns the implicit `Self: ThisTrait` bound that holds inside a trait's declaration.
    fn self_trait_goal(&mut self, trait_def: DefId, generics: &[HirId]) -> Goal {
        let self_ty = self.tcx.mk_self_param(trait_def);
        let args = generics.iter().map(|&id| self.tcx.mk_generic(id)).collect();
        Goal {
            self_ty,
            trait_: TraitRef {
                def: trait_def,
                args,
            },
        }
    }

    /// Returns the trait bounds declared on `generic`, e.g. `Show` for `T: Show`.
    pub(crate) fn bounds_of(&mut self, generic: HirId) -> Vec<Goal> {
        let hir: &'hir crate::hir::Hir = self.hir;
        let self_ty = self.tcx.mk_generic(generic);

        hir.generic(generic)
            .bounds
            .iter()
            .filter_map(|bound| match bound.path.res {
                Res::Type(Type::Def(TyDef::Trait(def))) => {
                    let args = self.lower_tys(&bound.args);
                    Some(Goal {
                        self_ty,
                        trait_: TraitRef { def, args },
                    })
                }
                _ => None,
            })
            .collect()
    }

    /// Rebuilds `goal` with every parameter in `subst` replaced by what it is bound to.
    pub(crate) fn subst_query(&mut self, goal: &Goal, subst: &HashMap<HirId, Ty>) -> Goal {
        goal.map(&mut |ty| self.subst_ty(ty, subst))
    }

    /// Rebuilds `goal` with every inference variable in it replaced by whatever it has since
    /// resolved to.
    fn resolve_goal(&mut self, goal: &Goal) -> Goal {
        goal.map(&mut |ty| self.unifier.find_deep(&mut self.tcx, ty))
    }

    /// Returns whether any part of the goal is [`TyKind::Error`].
    fn goal_mentions_error(&self, goal: &Goal) -> bool {
        let mut tys = std::iter::once(goal.self_ty).chain(goal.trait_.args.iter().copied());
        tys.any(|ty| visitor::mentions_error(&self.tcx, ty))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::hir::Hir;
    use crate::nameres::PrimTy;
    use crate::testing::{TypeckStage, checker_through, lower_to_hir};

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

    fn solver<'hir>(hir: &'hir Hir) -> Typeck<'hir> {
        let checker = checker_through(hir, TypeckStage::Index);
        crate::testing::clear_diagnostics();
        checker
    }

    fn named(checker: &Typeck<'_>, name: &str) -> DefId {
        crate::testing::named_def(checker.hir, name)
    }

    const SRC: &str = "trait Show { fun show(&self); }
                       struct Foo {}
                       struct Bare {}
                       extend Foo with Show { fun show(&self) {} }";

    #[test]
    fn a_matching_impl_proves_the_goal() {
        let hir = lower_to_hir(SRC);
        let mut checker = solver(&hir);
        let (foo, show) = (named(&checker, "Foo"), named(&checker, "Show"));
        let foo_ty = checker.tcx.mk_adt(foo, vec![]);

        let goal = Goal::new(foo_ty, show);
        assert_eq!(
            checker.implements(&goal, &BoundsEnv::default()),
            Solution::Holds
        );
    }

    #[test]
    fn a_type_with_no_impl_does_not_implement() {
        let hir = lower_to_hir(SRC);
        let mut checker = solver(&hir);
        let (bare, show) = (named(&checker, "Bare"), named(&checker, "Show"));
        let bare_ty = checker.tcx.mk_adt(bare, vec![]);

        let goal = Goal::new(bare_ty, show);
        assert_eq!(
            checker.implements(&goal, &BoundsEnv::default()),
            Solution::DoesNotHold
        );
    }

    #[test]
    fn an_unresolved_self_type_is_ambiguous() {
        let hir = lower_to_hir(SRC);
        let mut checker = solver(&hir);
        let show = named(&checker, "Show");
        let var = checker.tcx.next_infer_var();

        let goal = Goal::new(var, show);
        assert_eq!(
            checker.implements(&goal, &BoundsEnv::default()),
            Solution::Ambiguous
        );
        assert!(
            crate::testing::messages().is_empty(),
            "an ambiguity is not a diagnostic"
        );
    }

    #[test]
    fn a_goal_containing_an_error_answers_error_without_reporting() {
        let hir = lower_to_hir(SRC);
        let mut checker = solver(&hir);
        let show = named(&checker, "Show");
        let error = checker.tcx.error();

        let goal = Goal::new(error, show);
        assert_eq!(
            checker.implements(&goal, &BoundsEnv::default()),
            Solution::Error
        );
        assert!(
            crate::testing::messages().is_empty(),
            "a diagnostic for the error type already exists"
        );
    }

    #[test]
    fn a_reference_implements_nothing() {
        use crate::ast::Mutability;

        let hir = lower_to_hir(SRC);
        let mut checker = solver(&hir);
        let (foo, show) = (named(&checker, "Foo"), named(&checker, "Show"));
        let foo_ty = checker.tcx.mk_adt(foo, vec![]);
        let ref_ty = checker.tcx.mk_ref(foo_ty, Mutability::Immutable);

        let goal = Goal::new(ref_ty, show);
        assert_eq!(
            checker.implements(&goal, &BoundsEnv::default()),
            Solution::DoesNotHold
        );
    }

    #[test]
    fn a_primitive_satisfies_a_trait_it_extends() {
        let hir = lower_to_hir(
            "trait Show { fun show(&self); }
             extend i32 with Show { fun show(&self) {} }",
        );
        let mut checker = solver(&hir);
        let show = named(&checker, "Show");
        let i32_ty = checker.tcx.mk_prim(PrimTy::I32);

        let query = Goal::new(i32_ty, show);
        assert_eq!(
            checker.implements(&query, &BoundsEnv::default()),
            Solution::Holds
        );
    }

    #[test]
    fn a_tuple_satisfies_a_trait_it_extends() {
        let hir = lower_to_hir(
            "trait Show { fun show(&self); }
             extend (i32, i32) with Show { fun show(&self) {} }",
        );
        let mut checker = solver(&hir);
        let show = named(&checker, "Show");
        let i32_ty = checker.tcx.mk_prim(PrimTy::I32);
        let tuple_ty = checker.tcx.mk_tuple(vec![i32_ty, i32_ty]);

        let query = Goal::new(tuple_ty, show);
        assert_eq!(
            checker.implements(&query, &BoundsEnv::default()),
            Solution::Holds
        );
    }

    #[test]
    fn dyn_implements_exactly_the_trait_it_names() {
        let hir = lower_to_hir(
            "trait Show { fun show(&self); }
             trait Other { fun other(&self); }",
        );
        let mut checker = solver(&hir);
        let (show, other) = (named(&checker, "Show"), named(&checker, "Other"));
        let dyn_show = checker.tcx.mk_dyn(show, vec![]);

        let its_own = Goal::new(dyn_show, show);
        assert_eq!(
            checker.implements(&its_own, &BoundsEnv::default()),
            Solution::Holds
        );

        let another = Goal::new(dyn_show, other);
        assert_eq!(
            checker.implements(&another, &BoundsEnv::default()),
            Solution::DoesNotHold
        );
    }

    #[test]
    fn a_bound_in_the_environment_proves_a_goal_about_a_parameter() {
        let hir = lower_to_hir(
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

        let goal = Goal::new(t, show);
        assert_eq!(checker.implements(&goal, &env), Solution::Holds);
    }

    #[test]
    fn a_parameter_with_no_bound_implements_nothing() {
        let hir = lower_to_hir(
            "trait Show { fun show(&self); }
             fun f<T>(x: T) {}",
        );
        let mut checker = solver(&hir);
        let (f, show) = (named(&checker, "f"), named(&checker, "Show"));
        let function = hir.function(f);
        let t = checker.tcx.mk_generic(function.generics[0]);

        let env = checker.bounds_env(f);
        let goal = Goal::new(t, show);
        assert_eq!(checker.implements(&goal, &env), Solution::DoesNotHold);
    }

    #[test]
    fn a_traits_own_self_implements_it() {
        let hir = lower_to_hir("trait Show { fun show(&self); }");
        let mut checker = solver(&hir);
        let show = named(&checker, "Show");
        let self_ty = checker.tcx.mk_self_param(show);

        let env = checker.bounds_env(show);
        let goal = Goal::new(self_ty, show);
        assert_eq!(checker.implements(&goal, &env), Solution::Holds);
    }

    #[test]
    fn a_method_inherits_its_extend_blocks_bounds() {
        let hir = lower_to_hir(
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

    #[test]
    fn a_conditional_impls_own_bounds_are_proved_recursively() {
        let hir = lower_to_hir(
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

        let holds = Goal::new(wrap_foo, show);
        assert_eq!(
            checker.implements(&holds, &BoundsEnv::default()),
            Solution::Holds
        );

        let fails = Goal::new(wrap_bare, show);
        assert_eq!(
            checker.implements(&fails, &BoundsEnv::default()),
            Solution::DoesNotHold
        );
    }
}
