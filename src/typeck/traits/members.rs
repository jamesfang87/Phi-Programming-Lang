//! Trait-member checking: does an `extend .. with Trait` block provide exactly the methods that
//! trait declares, at exactly the signatures it declared them with?
//!
//! Coherence has already established that at most one impl answers any one question. This is the
//! other half of making the answer trustworthy: that the impl the solver selects actually has the
//! members a caller who only knows the *trait* will go looking for, and that calling one through
//! the trait and calling it directly mean the same thing. Without it, `x.show()` proved through a
//! bound would dispatch to a body taking different arguments than the bound promised.
//!
//! Three rules, run per trait impl:
//!
//! - every method the trait declares **without a default body** has to be provided. One
//!   diagnostic lists all of them at once, because a block missing four methods is one mistake
//!   with four parts rather than four mistakes.
//! - a method the trait does **not** declare is reported at its own span. An `extend .. with`
//!   block is not a place to add extra inherent methods, because there would be no trait
//!   declaration for a caller reaching the type through the trait to find them by.
//! - a provided method's signature has to **equal** the declaration's.
//!
//! ## Equality, not unification
//!
//! The comparison is `==` on interned [`Ty`] handles, which is what the design asks for: a method
//! that merely *could* unify with the declaration is still wrong. `fun show(&self, x: T)` where
//! the trait declared `fun show(&self, x: i32)` would unify happily by binding `T := i32`, and
//! accepting it would mean the implementation promises more than it delivers -- a caller with a
//! `bool` would type-check against the trait and land in a body expecting an `i32`.
//!
//! Equality is only meaningful once both signatures are phrased in the same terms, which is what
//! the substitution below is for. A trait's declaration is written in the trait's own vocabulary:
//! `Self`, the trait's parameters, and the method's own parameters. The impl's is written in the
//! impl's: the extended type, the arguments the block applied to the trait, and the method's own
//! parameters again -- which are *different* [`HirId`]s even when the user wrote the same letter.
//! Rewriting the declaration through all three is what makes `==` the right question.

use std::collections::{HashMap, HashSet};

use crate::ast::interner::Interner;
use crate::ast::{SelfMode, Symbol};
use crate::diag::{DiagCtx, Diagnostic};
use crate::hir::{DefId, Function, HirId, Node, OwnerNode};
use crate::lexer::src_span::SrcSpan;
use crate::typeck::Typeck;
use crate::typeck::traits::TraitRef;
use crate::typeck::traits::index::ImplId;
use crate::typeck::ty::{Ty, TyKind};

impl<'hir> Typeck<'hir> {
    /// Checks every trait impl in the index against the trait it implements.
    ///
    /// Runs in the same stage as [`check_coherence`](Typeck::check_coherence) and after it, so
    /// that a type with two conflicting impls is told about the conflict before being told about
    /// each impl's members separately. Inherent blocks are skipped: with no trait to compare
    /// against, every method they define is exactly as declared.
    pub fn check_trait_members(&mut self) {
        // Bucket order rather than index order, so that what comes out is grouped by the type
        // being implemented and does not depend on a hash map's iteration order.
        let impls: Vec<ImplId> = self
            .impls
            .extended_types()
            .into_iter()
            .flat_map(|head| self.impls.for_self(head).to_vec())
            .collect();

        for impl_id in impls {
            self.check_impl_members(impl_id);
        }
    }

    /// Checks one `extend .. with Trait` block against its trait.
    fn check_impl_members(&mut self, impl_id: ImplId) {
        let header = self.impls.header(impl_id);
        let Some(trait_ref) = header.trait_ref.clone() else {
            return;
        };
        let (self_ty, impl_def, impl_span) = (header.self_ty, header.def, header.span);

        let hir = self.hir;
        let OwnerNode::Extend(block) = hir.def(impl_def) else {
            unreachable!("an ImplHeader's def is always the extend block it was built from");
        };
        let OwnerNode::Trait(trait_) = hir.def(trait_ref.def) else {
            unreachable!("a TraitRef's def always names a trait; the index is what enforces it");
        };
        // Declaration order on both sides, so a block missing two methods names them in the order
        // the trait declares them and a block with two stray ones reports them top to bottom.
        let (provided, declared) = (&block.methods, &trait_.functions);
        let trait_generics = trait_.generics.clone();

        self.check_for_missing_methods(provided, declared, &trait_ref, self_ty, impl_span);

        let by_name: HashMap<Symbol, DefId> = declared
            .iter()
            .map(|&declaration| (self.function(declaration).name.text, declaration))
            .collect();

        // The block's arguments to the trait stand in for the trait's own parameters. A block
        // that wrote the wrong number of them has nothing to stand in for the rest, so signatures
        // are left uncompared rather than reported against a half-built substitution -- every one
        // of them would fail, and all for the same reason. The mismatch itself is reported by
        // `check_impl_headers` in [`super::bounds`], alongside the other argument-count checks,
        // so saying nothing here loses nothing.
        let arguments_line_up = trait_generics.len() == trait_ref.args.len();
        let trait_subst: HashMap<HirId, Ty> = trait_generics
            .into_iter()
            .zip(trait_ref.args.iter().copied())
            .collect();

        for &method in provided {
            let name = self.function(method).name.text;
            match by_name.get(&name) {
                None => self.report_not_a_member(method, &trait_ref, self_ty),
                Some(&declaration) if arguments_line_up => {
                    self.check_method_signature(method, declaration, &trait_subst, self_ty);
                }
                Some(_) => {}
            }
        }
    }

    /// Reports, once, every method the trait insists on that the block does not provide.
    ///
    /// A declaration with a default body is not insisted on: the type gets that body without
    /// writing anything, which is the whole point of defaults.
    fn check_for_missing_methods(
        &self,
        provided: &[DefId],
        declared: &[DefId],
        trait_ref: &TraitRef,
        self_ty: Ty,
        impl_span: SrcSpan,
    ) {
        let present: HashSet<Symbol> = provided
            .iter()
            .map(|&method| self.function(method).name.text)
            .collect();

        // Kept as definitions rather than reduced to names, so the diagnostic can underline each
        // one where the trait declares it.
        let missing: Vec<DefId> = declared
            .iter()
            .copied()
            .filter(|&declaration| {
                let declaration = self.function(declaration);
                declaration.block.is_none() && !present.contains(&declaration.name.text)
            })
            .collect();

        if !missing.is_empty() {
            self.report_missing_methods(&missing, trait_ref, self_ty, impl_span);
        }
    }

    /// Checks that `method` is declared exactly as `declaration` declares it.
    ///
    /// Each step gates the next rather than being reported alongside it. Once the type parameter
    /// lists disagree there is nothing to phrase the rest of the signature in; once the receivers
    /// disagree every parameter after them is off by one. Reporting the follow-on differences
    /// would describe consequences of the mistake already reported rather than separate mistakes.
    fn check_method_signature(
        &mut self,
        method: DefId,
        declaration: DefId,
        trait_subst: &HashMap<HirId, Ty>,
        self_ty: Ty,
    ) {
        let (found, expected) = (self.function(method), self.function(declaration));

        // The two lists declare different `HirId`s for what the user wrote as the same letter, so
        // the declaration's parameters have to be rewritten into the implementation's before
        // anything mentioning them can be compared.
        if found.generics.len() != expected.generics.len() {
            self.report_generic_count(found, expected);
            return;
        }
        let mut subst = trait_subst.clone();
        for (&declared, &implemented) in expected.generics.iter().zip(found.generics.iter()) {
            let ty = self.tcx.mk_generic(implemented);
            subst.insert(declared, ty);
        }

        // `self` is compared through the mode the user wrote rather than through the type it
        // lowers to, because "takes `&self` where the trait takes `&mut self`" is what happened,
        // and `&Foo` versus `&mut Foo` is only how it shows up.
        let (found_mode, expected_mode) = (self.self_mode(found), self.self_mode(expected));
        if found_mode != expected_mode {
            self.report_self_mode(found, expected, found_mode, expected_mode);
            return;
        }

        let (found_params, found_ret) = self.signature(method);
        let (expected_params, expected_ret) = self.signature(declaration);
        let expected_params: Vec<Ty> = expected_params
            .into_iter()
            .map(|ty| self.subst_member_ty(ty, &subst, self_ty))
            .collect();
        let expected_ret = expected_ret.map(|ty| self.subst_member_ty(ty, &subst, self_ty));

        if found_params.len() != expected_params.len() {
            self.report_param_count(found, expected, found_params.len(), expected_params.len());
            return;
        }

        // A method's `self` counts as its first parameter -- see `collect_function` -- and the
        // declaration's is `Self` in whichever mode both sides agreed on just above, which
        // substitutes to exactly the type the implementation's lowered to. It cannot differ, and
        // it has no name to report against, so the named parameters start after it.
        let offset = usize::from(found.self_param.is_some());
        for (index, (&got, &want)) in found_params
            .iter()
            .zip(expected_params.iter())
            .enumerate()
            .skip(offset)
        {
            if got != want {
                // Both lists are the same length and `self` was skipped on both sides, so the
                // declaration has a parameter at this index too.
                self.report_param_ty(
                    found,
                    found.params[index - offset],
                    expected.params[index - offset],
                    got,
                    want,
                );
            }
        }

        if found_ret != expected_ret {
            self.report_ret_ty(found, expected, found_ret, expected_ret);
        }
    }

    /// The declaration rewritten in the implementation's terms: `Self` replaced by the type being
    /// extended, and every parameter in `subst` by what it stands for there.
    fn subst_member_ty(&mut self, ty: Ty, subst: &HashMap<HirId, Ty>, self_ty: Ty) -> Ty {
        let ty = self.subst_self_ty(ty, self_ty);
        self.subst_ty(ty, subst)
    }

    /// Replaces `Self` with the type implementing the trait, everywhere inside `ty`.
    ///
    /// This is deliberately separate from [`Typeck::subst_ty`], which leaves
    /// [`TyKind::SelfTy`] alone. That is the right behavior everywhere else in the solver: inside
    /// an `extend` block `Self` is concrete and lowers straight to the extended type, so a live
    /// `SelfTy` never reaches the query. A trait's *declaration* is the one place one survives,
    /// and turning it into the implementing type is what makes the two signatures comparable at
    /// all -- folding the case into `subst_ty` would mean giving every one of its callers a self
    /// type they have no use for.
    fn subst_self_ty(&mut self, ty: Ty, self_ty: Ty) -> Ty {
        match self.tcx.kind(ty).clone() {
            TyKind::SelfTy(_) => self_ty,
            TyKind::Adt { def, args } => {
                let args = self.subst_self_tys(&args, self_ty);
                self.tcx.mk_adt(def, args)
            }
            TyKind::Dyn { trait_, args } => {
                let args = self.subst_self_tys(&args, self_ty);
                self.tcx.mk_dyn(trait_, args)
            }
            TyKind::Tuple(elems) => {
                let elems = self.subst_self_tys(&elems, self_ty);
                self.tcx.mk_tuple(elems)
            }
            TyKind::Ref { base, mutability } => {
                let base = self.subst_self_ty(base, self_ty);
                self.tcx.mk_ref(base, mutability)
            }
            TyKind::Any(base) => {
                let base = self.subst_self_ty(base, self_ty);
                self.tcx.mk_any(base)
            }
            TyKind::Array { elem, len } => {
                let elem = self.subst_self_ty(elem, self_ty);
                self.tcx.mk_array(elem, len)
            }
            TyKind::Fun { params, ret } => {
                let params = self.subst_self_tys(&params, self_ty);
                let ret = ret.map(|ret| self.subst_self_ty(ret, self_ty));
                self.tcx.mk_fun(params, ret)
            }
            // Nothing to substitute into, `Self` included: a `TyKind::Generic` names a parameter,
            // which is `subst_ty`'s business rather than this function's.
            TyKind::Var(_)
            | TyKind::Primitive(_)
            | TyKind::Generic(_)
            | TyKind::Unit
            | TyKind::Never
            | TyKind::Error => ty,
        }
    }

    fn subst_self_tys(&mut self, tys: &[Ty], self_ty: Ty) -> Vec<Ty> {
        tys.iter()
            .map(|&ty| self.subst_self_ty(ty, self_ty))
            .collect()
    }

    /// A function's lowered signature, split into its parameter types -- `self` first, if it has
    /// one -- and its return type.
    fn signature(&self, def: DefId) -> (Vec<Ty>, Option<Ty>) {
        let sig = self
            .types
            .ty_of_def(def)
            .expect("collect_function records every function's own signature");
        let TyKind::Fun { params, ret } = self.tcx.kind(sig) else {
            unreachable!("a function's own signature always lowers to TyKind::Fun");
        };
        (params.clone(), *ret)
    }

    /// How a method takes its receiver, or `None` for an associated function that takes none.
    fn self_mode(&self, function: &Function) -> Option<SelfMode> {
        let id = function.self_param?;
        let Node::SelfParam(self_param) = self.hir.node(id) else {
            unreachable!("a function's self param slot always holds a Node::SelfParam");
        };
        Some(self_param.mode)
    }

    /// The function `def` names. Borrowed at the HIR's own lifetime rather than at this borrow of
    /// `self`, so that a signature read out of it survives the `&mut self` calls that follow.
    fn function(&self, def: DefId) -> &'hir Function {
        let hir = self.hir;
        let OwnerNode::Function(function) = hir.def(def) else {
            unreachable!(
                "a trait's `functions` and an extend block's `methods` hold only functions"
            );
        };
        function
    }

    // -----------------------------------------------------------------
    // Diagnostics
    //
    // Every mismatch here is between two places: what the implementation wrote and what the trait
    // declared. The primary span is always the implementation's, because that is the side that has
    // to change -- the trait is what it is, and a method that disagrees with it is the one in the
    // wrong. The declaration gets a secondary label, at the narrowest part of it that differs: the
    // receiver for a receiver mismatch, the return type for a return mismatch, and so on, rather
    // than the whole signature every time.
    // -----------------------------------------------------------------

    fn report_missing_methods(
        &self,
        missing: &[DefId],
        trait_ref: &TraitRef,
        self_ty: Ty,
        impl_span: SrcSpan,
    ) {
        let names: Vec<String> = missing
            .iter()
            .map(|&declaration| format!("`{}`", Interner::resolve(self.function(declaration).name.text)))
            .collect();
        let (plural, these) = if missing.len() == 1 {
            ("", "this")
        } else {
            ("s", "these")
        };

        let mut diag = Diagnostic::error(
            format!(
                "missing method{plural} in the implementation of trait `{}` for `{}`: {}",
                self.declared_trait_name(trait_ref.def),
                self.cx().show(self_ty),
                names.join(", ")
            ),
            impl_span,
        )
        .with_label(format!("{these} method{plural} not implemented"))
        .with_help(
            "every method a trait declares without a default body has to be written out by \
             each implementation; giving the declaration a body makes it optional instead",
        );

        // One label per missing method rather than one for the trait as a whole: a trait with
        // twenty methods and two missing should point at the two.
        for &declaration in missing {
            let declaration = self.function(declaration);
            diag = diag.with_secondary(
                declaration.name.span,
                format!(
                    "`{}` is declared here, with no default body",
                    Interner::resolve(declaration.name.text)
                ),
            );
        }

        DiagCtx::emit(diag);
    }

    fn report_not_a_member(&self, method: DefId, trait_ref: &TraitRef, self_ty: Ty) {
        let function = self.function(method);
        let name = Interner::resolve(function.name.text);
        let trait_name = self.declared_trait_name(trait_ref.def);

        DiagCtx::emit(
            Diagnostic::error(
                format!("method `{name}` is not a member of trait `{trait_name}`"),
                function.span,
            )
            .with_label(format!("not declared by `{trait_name}`"))
            .with_secondary(
                self.declared_trait_span(trait_ref.def),
                format!("`{trait_name}` is declared here"),
            )
            .with_help(format!(
                "an `extend .. with {trait_name}` block may only implement what `{trait_name}` \
                 declares, since that is all a caller reaching `{}` through the trait can see; \
                 put `{name}` in an inherent `extend` block instead",
                self.cx().show(self_ty)
            )),
        );
    }

    fn report_generic_count(&self, found: &Function, expected: &Function) {
        let (got, want) = (found.generics.len(), expected.generics.len());
        let plural = if want == 1 { "" } else { "s" };

        DiagCtx::emit(
            Diagnostic::error(
                format!(
                    "method `{}` declares {got} type parameters where its declaration declares \
                     {want}",
                    Interner::resolve(found.name.text)
                ),
                found.name.span,
            )
            .with_label(format!("expected {want} type parameter{plural}"))
            .with_secondary(
                expected.name.span,
                format!("declared with {want} type parameter{plural} here"),
            )
            .with_help(
                "an implementation has to be as general as the declaration it fulfills, so the \
                 two parameter lists have to line up one for one",
            ),
        );
    }

    fn report_self_mode(
        &self,
        found: &Function,
        expected: &Function,
        found_mode: Option<SelfMode>,
        expected_mode: Option<SelfMode>,
    ) {
        DiagCtx::emit(
            Diagnostic::error(
                format!(
                    "method `{}` takes {} where its declaration takes {}",
                    Interner::resolve(found.name.text),
                    show_self_mode(found_mode),
                    show_self_mode(expected_mode)
                ),
                self.self_param_span(found),
            )
            .with_label(format!("expected {}", show_self_mode(expected_mode)))
            .with_secondary(
                self.self_param_span(expected),
                format!("declared taking {} here", show_self_mode(expected_mode)),
            )
            .with_help(
                "how a method takes its receiver is part of its signature: a caller reaching it \
                 through the trait is checked against what the trait declared",
            ),
        );
    }

    fn report_param_count(&self, found: &Function, expected: &Function, got: usize, want: usize) {
        // Reported the way the user wrote it, so `self` -- which the checker counts as the first
        // parameter -- is not counted here.
        let offset = usize::from(found.self_param.is_some());
        let (got, want) = (got - offset, want - offset);
        let plural = if want == 1 { "" } else { "s" };

        DiagCtx::emit(
            Diagnostic::error(
                format!(
                    "method `{}` takes {got} parameters where its declaration takes {want}",
                    Interner::resolve(found.name.text)
                ),
                found.name.span,
            )
            .with_label(format!("expected {want} parameter{plural}"))
            .with_secondary(
                expected.name.span,
                format!("declared taking {want} parameter{plural} here"),
            ),
        );
    }

    fn report_param_ty(
        &self,
        found: &Function,
        param: HirId,
        declared_param: HirId,
        got: Ty,
        want: Ty,
    ) {
        let Node::Param(param) = self.hir.node(param) else {
            unreachable!("a function's parameter list holds only Node::Params");
        };
        let Node::Param(declared_param) = self.hir.node(declared_param) else {
            unreachable!("a function's parameter list holds only Node::Params");
        };

        DiagCtx::emit(
            Diagnostic::error(
                format!(
                    "parameter `{}` of method `{}` has type `{}` where its declaration has `{}`",
                    Interner::resolve(param.name.text),
                    Interner::resolve(found.name.text),
                    self.cx().show(got),
                    self.cx().show(want)
                ),
                param.span,
            )
            .with_label(format!("expected `{}`", self.cx().show(want)))
            .with_secondary(
                declared_param.span,
                format!("declared as `{}` here", self.cx().show(want)),
            )
            .with_help(
                "a signature has to match its declaration exactly, not merely be compatible with \
                 it: a parameter that is more general still accepts arguments the trait never \
                 promised the implementation would take",
            ),
        );
    }

    fn report_ret_ty(
        &self,
        found: &Function,
        expected: &Function,
        got: Option<Ty>,
        want: Option<Ty>,
    ) {
        DiagCtx::emit(
            Diagnostic::error(
                format!(
                    "method `{}` returns {} where its declaration returns {}",
                    Interner::resolve(found.name.text),
                    self.show_ret(got),
                    self.show_ret(want)
                ),
                self.ret_span(found),
            )
            .with_label(format!("expected {}", self.show_ret(want)))
            .with_secondary(
                self.ret_span(expected),
                format!("declared returning {} here", self.show_ret(want)),
            ),
        );
    }

    /// Where a function's receiver is written, or its name when it takes none -- an associated
    /// function has no receiver to underline, but "this one takes no `self`" still has to point
    /// somewhere.
    fn self_param_span(&self, function: &Function) -> SrcSpan {
        function
            .self_param
            .map_or(function.name.span, |id| match self.hir.node(id) {
                Node::SelfParam(self_param) => self_param.span,
                _ => unreachable!("a function's self param slot always holds a Node::SelfParam"),
            })
    }

    /// Where a function's return type is written, or its name when it declares none. Same reason
    /// as [`Typeck::self_param_span`]: a missing `->` is exactly what some of these diagnostics
    /// are about.
    fn ret_span(&self, function: &Function) -> SrcSpan {
        function
            .ret
            .map_or(function.name.span, |id| match self.hir.node(id) {
                Node::Ty(ty) => ty.span,
                _ => unreachable!("a function's return slot always holds a Node::Ty"),
            })
    }

    /// How a return type reads in a diagnostic. A function with no `->` produces nothing, which
    /// is a different thing to say than naming a type.
    fn show_ret(&self, ret: Option<Ty>) -> String {
        match ret {
            Some(ty) => format!("`{}`", self.cx().show(ty)),
            None => "nothing".to_string(),
        }
    }

    /// The name a trait was declared with.
    fn declared_trait_name(&self, def: DefId) -> &'static str {
        let OwnerNode::Trait(trait_) = self.hir.def(def) else {
            unreachable!("a TraitRef's def always names a trait; the index is what enforces it");
        };
        Interner::resolve(trait_.name.text)
    }

    /// Where a trait was declared, for a diagnostic to point back at.
    fn declared_trait_span(&self, def: DefId) -> SrcSpan {
        let OwnerNode::Trait(trait_) = self.hir.def(def) else {
            unreachable!("a TraitRef's def always names a trait; the index is what enforces it");
        };
        trait_.name.span
    }
}

/// How a receiver reads in a diagnostic, including the absence of one.
fn show_self_mode(mode: Option<SelfMode>) -> &'static str {
    match mode {
        Some(SelfMode::Immutable) => "`&self`",
        Some(SelfMode::Mutable) => "`&mut self`",
        Some(SelfMode::Move) => "`self`",
        Some(SelfMode::Any) => "`any self`",
        None => "no receiver",
    }
}

#[cfg(test)]
mod tests {
    use crate::diag::DiagCtx;
    use crate::hir::{Hir, NameResolutions, OwnerNode};
    use crate::nameres::results::PrimTy;
    use crate::testing::resolve_src;
    use crate::typeck::Typeck;
    use crate::typeck::ty::TyKind;

    /// Runs everything up to and including trait-member checking over `src`, and hands back what
    /// this pass reported.
    ///
    /// Coherence is deliberately included, so that a fixture which accidentally overlaps itself
    /// shows up as an extra message rather than passing silently. Diagnostics are cleared after
    /// the index is built: a fixture is resolved without the core library, so name resolution
    /// reports the whole set of missing lang items first.
    fn members(hir: &Hir, nameres: &NameResolutions) -> Vec<String> {
        let mut checker = Typeck::new(hir, nameres);
        checker.collect_module(hir.root_id());
        checker.build_impl_index();
        checker.check_coherence();
        DiagCtx::clear();
        checker.check_trait_members();

        DiagCtx::diagnostics()
            .into_iter()
            .map(|diagnostic| diagnostic.message)
            .collect()
    }

    // -----------------------------------------------------------------
    // Which methods are there
    // -----------------------------------------------------------------

    #[test]
    fn an_implementation_providing_exactly_the_declared_methods_is_accepted() {
        let (hir, nameres) = resolve_src(
            "trait Show { fun show(&self); }
             struct Foo {}
             extend Foo with Show { fun show(&self) {} }",
        );

        assert!(members(&hir, &nameres).is_empty());
    }

    #[test]
    fn a_missing_method_is_reported() {
        let (hir, nameres) = resolve_src(
            "trait Show { fun show(&self); }
             struct Foo {}
             extend Foo with Show {}",
        );

        assert_eq!(
            members(&hir, &nameres),
            ["missing method in the implementation of trait `Show` for `Foo`: `show`"]
        );
    }

    /// One diagnostic for the whole block, listing every method at once: a block missing four
    /// methods is one mistake with four parts.
    #[test]
    fn every_missing_method_is_named_in_one_diagnostic() {
        let (hir, nameres) = resolve_src(
            "trait Show { fun show(&self); fun size(&self); }
             struct Foo {}
             extend Foo with Show {}",
        );

        assert_eq!(
            members(&hir, &nameres),
            ["missing methods in the implementation of trait `Show` for `Foo`: `show`, `size`"]
        );
    }

    /// Each missing method gets its own label at its own declaration, so a trait with many
    /// methods points at the ones that are actually missing rather than at itself.
    #[test]
    fn every_missing_method_is_underlined_where_it_is_declared() {
        let (hir, nameres) = resolve_src(
            "trait Show { fun show(&self); fun size(&self); fun free(&self) {} }
             struct Foo {}
             extend Foo with Show {}",
        );

        let mut checker = Typeck::new(&hir, &nameres);
        checker.collect_module(hir.root_id());
        checker.build_impl_index();
        checker.check_coherence();
        DiagCtx::clear();
        checker.check_trait_members();

        let diagnostics = DiagCtx::diagnostics();
        let [missing] = diagnostics.as_slice() else {
            panic!("expected exactly one diagnostic, got {diagnostics:?}");
        };
        assert_eq!(
            missing
                .secondary
                .iter()
                .map(|label| label.message.as_str())
                .collect::<Vec<_>>(),
            [
                "`show` is declared here, with no default body",
                "`size` is declared here, with no default body",
            ]
        );
    }

    /// A declaration with a body is one the type gets for free, so leaving it out is not an
    /// omission.
    #[test]
    fn a_method_with_a_default_body_need_not_be_implemented() {
        let (hir, nameres) = resolve_src(
            "trait Show { fun show(&self) {} }
             struct Foo {}
             extend Foo with Show {}",
        );

        assert!(members(&hir, &nameres).is_empty());
    }

    #[test]
    fn a_method_with_a_default_body_may_still_be_overridden() {
        let (hir, nameres) = resolve_src(
            "trait Show { fun show(&self) {} }
             struct Foo {}
             extend Foo with Show { fun show(&self) {} }",
        );

        assert!(members(&hir, &nameres).is_empty());
    }

    #[test]
    fn a_method_the_trait_does_not_declare_is_reported() {
        let (hir, nameres) = resolve_src(
            "trait Show { fun show(&self); }
             struct Foo {}
             extend Foo with Show { fun show(&self) {} fun extra(&self) {} }",
        );

        assert_eq!(
            members(&hir, &nameres),
            ["method `extra` is not a member of trait `Show`"]
        );
    }

    /// An inherent block has no declaration to be measured against, so nothing here applies to
    /// it.
    #[test]
    fn an_inherent_block_may_define_whatever_it_likes() {
        let (hir, nameres) = resolve_src(
            "struct Foo {}
             extend Foo { fun anything(&self) -> i32 {} }",
        );

        assert!(members(&hir, &nameres).is_empty());
    }

    // -----------------------------------------------------------------
    // Signatures
    // -----------------------------------------------------------------

    #[test]
    fn too_few_parameters_is_reported() {
        let (hir, nameres) = resolve_src(
            "trait Show { fun show(&self, width: i32); }
             struct Foo {}
             extend Foo with Show { fun show(&self) {} }",
        );

        assert_eq!(
            members(&hir, &nameres),
            ["method `show` takes 0 parameters where its declaration takes 1"]
        );
    }

    #[test]
    fn a_parameter_of_the_wrong_type_is_reported() {
        let (hir, nameres) = resolve_src(
            "trait Show { fun show(&self, width: i32); }
             struct Foo {}
             extend Foo with Show { fun show(&self, width: bool) {} }",
        );

        assert_eq!(
            members(&hir, &nameres),
            ["parameter `width` of method `show` has type `bool` where its declaration has `i32`"]
        );
    }

    #[test]
    fn a_wrong_return_type_is_reported() {
        let (hir, nameres) = resolve_src(
            "trait Show { fun show(&self) -> i32; }
             struct Foo {}
             extend Foo with Show { fun show(&self) -> bool {} }",
        );

        assert_eq!(
            members(&hir, &nameres),
            ["method `show` returns `bool` where its declaration returns `i32`"]
        );
    }

    /// Returning nothing is a different thing to say than returning a type, so the wording says
    /// so rather than inventing a `()` the user never wrote.
    #[test]
    fn a_missing_return_type_is_reported() {
        let (hir, nameres) = resolve_src(
            "trait Show { fun show(&self) -> i32; }
             struct Foo {}
             extend Foo with Show { fun show(&self) {} }",
        );

        assert_eq!(
            members(&hir, &nameres),
            ["method `show` returns nothing where its declaration returns `i32`"]
        );
    }

    #[test]
    fn the_wrong_receiver_mode_is_reported() {
        let (hir, nameres) = resolve_src(
            "trait Show { fun show(&mut self); }
             struct Foo {}
             extend Foo with Show { fun show(&self) {} }",
        );

        assert_eq!(
            members(&hir, &nameres),
            ["method `show` takes `&self` where its declaration takes `&mut self`"]
        );
    }

    #[test]
    fn a_receiver_where_the_declaration_has_none_is_reported() {
        let (hir, nameres) = resolve_src(
            "trait Show { fun make(); }
             struct Foo {}
             extend Foo with Show { fun make(&self) {} }",
        );

        assert_eq!(
            members(&hir, &nameres),
            ["method `make` takes `&self` where its declaration takes no receiver"]
        );
    }

    #[test]
    fn a_different_number_of_type_parameters_is_reported() {
        let (hir, nameres) = resolve_src(
            "trait Show { fun show<U>(&self); }
             struct Foo {}
             extend Foo with Show { fun show(&self) {} }",
        );

        assert_eq!(
            members(&hir, &nameres),
            ["method `show` declares 0 type parameters where its declaration declares 1"]
        );
    }

    /// The two `U`s are different `HirId`s, so making this pass is exactly the renaming step the
    /// substitution does.
    #[test]
    fn a_methods_own_type_parameters_are_matched_up_positionally() {
        let (hir, nameres) = resolve_src(
            "trait Show { fun show<U>(&self, value: U); }
             struct Foo {}
             extend Foo with Show { fun show<U>(&self, value: U) {} }",
        );

        assert!(members(&hir, &nameres).is_empty());
    }

    /// A signature that would *unify* with the declaration is still wrong: `T` accepts arguments
    /// the trait never promised the implementation would take.
    #[test]
    fn a_signature_that_merely_unifies_is_still_rejected() {
        let (hir, nameres) = resolve_src(
            "trait Show { fun show(&self, width: i32); }
             struct Foo {}
             extend<T> Foo with Show { fun show(&self, width: T) {} }",
        );

        assert_eq!(
            members(&hir, &nameres),
            ["parameter `width` of method `show` has type `T` where its declaration has `i32`"]
        );
    }

    // -----------------------------------------------------------------
    // Substitution: `Self` and the trait's own parameters
    // -----------------------------------------------------------------

    /// `Self` in the declaration means the implementing type, so both spellings check.
    #[test]
    fn self_in_a_declaration_stands_for_the_implementing_type() {
        let (hir, nameres) = resolve_src(
            "trait Clone { fun clone(&self) -> Self; fun copy(&self) -> Self; }
             struct Foo {}
             extend Foo with Clone { fun clone(&self) -> Foo {} fun copy(&self) -> Self {} }",
        );

        assert!(members(&hir, &nameres).is_empty());
    }

    #[test]
    fn a_declaration_returning_self_is_not_satisfied_by_another_type() {
        let (hir, nameres) = resolve_src(
            "trait Clone { fun clone(&self) -> Self; }
             struct Foo {}
             struct Bar {}
             extend Foo with Clone { fun clone(&self) -> Bar {} }",
        );

        assert_eq!(
            members(&hir, &nameres),
            ["method `clone` returns `Bar` where its declaration returns `Foo`"]
        );
    }

    /// The case the substitution exists for: the declaration is written in `K` and `V`, and the
    /// implementation in whatever the block applied the trait to.
    #[test]
    fn a_generic_traits_parameters_are_substituted_from_the_blocks_arguments() {
        let (hir, nameres) = resolve_src(
            "trait Index<K, V> { fun get(&self, key: K) -> V; }
             struct Map {}
             extend Map with Index<i32, bool> { fun get(&self, key: i32) -> bool {} }",
        );

        assert!(members(&hir, &nameres).is_empty());
    }

    #[test]
    fn a_generic_traits_parameters_are_not_satisfied_by_the_wrong_arguments() {
        let (hir, nameres) = resolve_src(
            "trait Index<K, V> { fun get(&self, key: K) -> V; }
             struct Map {}
             extend Map with Index<i32, bool> { fun get(&self, key: bool) -> bool {} }",
        );

        assert_eq!(
            members(&hir, &nameres),
            ["parameter `key` of method `get` has type `bool` where its declaration has `i32`"]
        );
    }

    /// The block's own parameters may be what it applies to the trait, in which case the
    /// declaration substitutes to a signature that is itself open.
    #[test]
    fn a_blocks_own_parameters_may_be_the_traits_arguments() {
        let (hir, nameres) = resolve_src(
            "trait Index<K, V> { fun get(&self, key: K) -> V; }
             struct Map<T> { inner: T }
             extend<T> Map<T> with Index<i32, T> { fun get(&self, key: i32) -> T {} }",
        );

        assert!(members(&hir, &nameres).is_empty());
    }

    /// A composite type is rewritten through, not just a bare `Self` or a bare parameter.
    #[test]
    fn substitution_reaches_inside_composite_types() {
        let (hir, nameres) = resolve_src(
            "trait Index<K, V> { fun get(&self, key: (K, &Self)) -> V; }
             struct Map {}
             extend Map with Index<i32, bool> { fun get(&self, key: (i32, &Map)) -> bool {} }",
        );

        assert!(members(&hir, &nameres).is_empty());
    }

    // -----------------------------------------------------------------
    // subst_self_ty
    // -----------------------------------------------------------------

    /// `Self` is replaced wherever it appears, however deeply nested, and nothing else is
    /// touched.
    #[test]
    fn substituting_self_rewrites_every_occurrence_and_only_those() {
        let (hir, nameres) = resolve_src("struct Foo {}");
        let mut checker = Typeck::new(&hir, &nameres);
        checker.collect_module(hir.root_id());

        let foo = hir
            .root()
            .items
            .iter()
            .copied()
            .find(|&id| matches!(hir.def(id), OwnerNode::Struct(_)))
            .expect("the fixture declares a struct");
        let foo_ty = checker.tcx.mk_adt(foo, vec![]);

        let self_param = checker.tcx.mk_self_param(foo);
        let i32_ty = checker.tcx.mk_prim(PrimTy::I32);
        let nested = checker.tcx.mk_tuple(vec![self_param, i32_ty]);

        assert_eq!(checker.subst_self_ty(self_param, foo_ty), foo_ty);

        let substituted = checker.subst_self_ty(nested, foo_ty);
        assert_eq!(
            *checker.tcx.kind(substituted),
            TyKind::Tuple(vec![foo_ty, i32_ty])
        );
        assert_eq!(
            checker.subst_self_ty(i32_ty, foo_ty),
            i32_ty,
            "a type with no `Self` in it comes back unchanged"
        );
    }
}
