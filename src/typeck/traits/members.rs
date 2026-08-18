use std::collections::{HashMap, HashSet};

use crate::ast::Symbol;
use crate::diagnostics::typeck::traits::members::{
    report_generic_count, report_missing_methods, report_not_a_member, report_param_count,
    report_param_ty, report_ret_ty, report_self_mode,
};
use crate::driver::source::SrcSpan;
use crate::hir::{DefId, HirId};
use crate::typeck::Typeck;
use crate::typeck::traits::TraitRef;
use crate::typeck::ty::Ty;

impl<'hir> Typeck<'hir> {
    /// Checks every trait `extend` block in the index against the trait it implements.
    pub fn check_trait_members(&mut self) {
        for block in self.extends.all() {
            self.check_extend_against_trait(block);
        }
    }

    /// Checks one `extend .. with Trait` block against its trait.
    fn check_extend_against_trait(&mut self, block: DefId) {
        let Some(trait_ref) = self.extends.trait_of(block).cloned() else {
            return;
        };
        let self_ty = self.adt_of_with_args(block);

        let hir = self.hir;
        let node = hir.extend(block);
        let extend_span = node.span;
        let trait_ = hir.trait_(trait_ref.def);
        // Both sides are read in declaration order, so a block missing two methods names them in
        // the order the trait declares them, and a block with two stray ones reports them top to
        // bottom.
        let (provided, declared) = (&node.methods, &trait_.functions);
        let trait_generics = trait_.generics.clone();

        self.check_for_missing_methods(provided, declared, &trait_ref, self_ty, extend_span);

        let arguments_line_up = trait_generics.len() == trait_ref.args.len();
        let trait_subst: HashMap<HirId, Ty> = trait_generics
            .into_iter()
            .zip(trait_ref.args.iter().copied())
            .collect();

        for &method in provided {
            let name = self.hir.function(method).name.text;
            match self.trait_method(trait_ref.def, name) {
                None => {
                    report_not_a_member(self.hir, self.display_cx(), method, &trait_ref, self_ty)
                }
                Some(declaration) if arguments_line_up => {
                    self.check_method_signature(method, declaration, &trait_subst, self_ty);
                }
                Some(_) => {}
            }
        }
    }

    fn check_for_missing_methods(
        &self,
        provided: &[DefId],
        declared: &[DefId],
        trait_ref: &TraitRef,
        self_ty: Ty,
        extend_span: SrcSpan,
    ) {
        let present: HashSet<Symbol> = provided
            .iter()
            .map(|&method| self.hir.function(method).name.text)
            .collect();

        // Kept as definitions rather than reduced to names, so the diagnostic can underline each
        // one where the trait declares it.
        let missing: Vec<DefId> = declared
            .iter()
            .copied()
            .filter(|&declaration| {
                let declaration = self.hir.function(declaration);
                declaration.block.is_none() && !present.contains(&declaration.name.text)
            })
            .collect();

        if !missing.is_empty() {
            report_missing_methods(
                self.hir,
                self.display_cx(),
                &missing,
                trait_ref,
                self_ty,
                extend_span,
            );
        }
    }

    fn check_method_signature(
        &mut self,
        method: DefId,
        declaration: DefId,
        trait_subst: &HashMap<HirId, Ty>,
        self_ty: Ty,
    ) {
        let (found, expected) = (self.hir.function(method), self.hir.function(declaration));

        if found.generics.len() != expected.generics.len() {
            report_generic_count(found, expected);
            return;
        }
        let mut subst = trait_subst.clone();
        for (&declared, &implemented) in expected.generics.iter().zip(found.generics.iter()) {
            let ty = self.tcx.mk_generic(implemented);
            subst.insert(declared, ty);
        }

        let (found_mode, expected_mode) =
            (self.receiver_mode(method), self.receiver_mode(declaration));
        if found_mode != expected_mode {
            report_self_mode(self.hir, found, expected, found_mode, expected_mode);
            return;
        }

        let signature = |checker: &mut Self, def| {
            checker
                .signature(def)
                .expect("collect_function records every function's own signature")
        };
        let (found_params, found_ret) = signature(self, method);
        let (expected_params, expected_ret) = signature(self, declaration);
        let expected_params: Vec<Ty> = expected_params
            .into_iter()
            .map(|ty| self.subst_sig_ty(ty, &subst, self_ty))
            .collect();
        let expected_ret = expected_ret.map(|ty| self.subst_sig_ty(ty, &subst, self_ty));

        if found_params.len() != expected_params.len() {
            report_param_count(found, expected, found_params.len(), expected_params.len());
            return;
        }

        let offset = usize::from(found.self_param.is_some());
        for (index, (&got, &want)) in found_params
            .iter()
            .zip(expected_params.iter())
            .enumerate()
            .skip(offset)
        {
            if got != want {
                report_param_ty(
                    self.hir,
                    self.display_cx(),
                    found,
                    found.params[index - offset],
                    expected.params[index - offset],
                    got,
                    want,
                );
            }
        }

        if found_ret != expected_ret {
            report_ret_ty(
                self.hir,
                self.display_cx(),
                found,
                expected,
                found_ret,
                expected_ret,
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use crate::diagnostics::DiagCtx;
    use crate::hir::{Hir, OwnerNode};
    use crate::nameres::PrimTy;
    use crate::testing::{Stage, checker_through, messages, resolve_src};
    use crate::typeck::Typeck;
    use crate::typeck::ty::TyKind;

    /// Runs everything up to and including trait-member checking over `src`, and hands back what
    /// this pass reported.
    ///
    /// Coherence is deliberately included, so that a fixture which accidentally overlaps itself
    /// shows up as an extra message rather than passing silently. Diagnostics are cleared after
    /// the index is built: a fixture is resolved without the core library, so name resolution
    /// reports the whole set of missing lang items first.
    fn members(hir: &Hir) -> Vec<String> {
        let mut checker = checker_through(hir, Stage::Coherence);
        DiagCtx::clear();
        checker.check_trait_members();
        messages()
    }

    // -----------------------------------------------------------------
    // Which methods are there
    // -----------------------------------------------------------------

    #[test]
    fn an_implementation_providing_exactly_the_declared_methods_is_accepted() {
        let hir = resolve_src(
            "trait Show { fun show(&self); }
             struct Foo {}
             extend Foo with Show { fun show(&self) {} }",
        );

        assert!(members(&hir).is_empty());
    }

    #[test]
    fn a_missing_method_is_reported() {
        let hir = resolve_src(
            "trait Show { fun show(&self); }
             struct Foo {}
             extend Foo with Show {}",
        );

        assert_eq!(
            members(&hir),
            ["missing method in the implementation of trait `Show` for `Foo`: `show`"]
        );
    }

    /// One diagnostic for the whole block, listing every method at once: a block missing four
    /// methods is one mistake with four parts.
    #[test]
    fn every_missing_method_is_named_in_one_diagnostic() {
        let hir = resolve_src(
            "trait Show { fun show(&self); fun size(&self); }
             struct Foo {}
             extend Foo with Show {}",
        );

        assert_eq!(
            members(&hir),
            ["missing methods in the implementation of trait `Show` for `Foo`: `show`, `size`"]
        );
    }

    /// Each missing method gets its own label at its own declaration, so a trait with many
    /// methods points at the ones that are actually missing rather than at itself.
    #[test]
    fn every_missing_method_is_underlined_where_it_is_declared() {
        let hir = resolve_src(
            "trait Show { fun show(&self); fun size(&self); fun free(&self) {} }
             struct Foo {}
             extend Foo with Show {}",
        );

        let mut checker = Typeck::new(&hir);
        checker.collect_module(hir.root_id());
        checker.build_extend_index();
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
        let hir = resolve_src(
            "trait Show { fun show(&self) {} }
             struct Foo {}
             extend Foo with Show {}",
        );

        assert!(members(&hir).is_empty());
    }

    #[test]
    fn a_method_with_a_default_body_may_still_be_overridden() {
        let hir = resolve_src(
            "trait Show { fun show(&self) {} }
             struct Foo {}
             extend Foo with Show { fun show(&self) {} }",
        );

        assert!(members(&hir).is_empty());
    }

    #[test]
    fn a_method_the_trait_does_not_declare_is_reported() {
        let hir = resolve_src(
            "trait Show { fun show(&self); }
             struct Foo {}
             extend Foo with Show { fun show(&self) {} fun extra(&self) {} }",
        );

        assert_eq!(
            members(&hir),
            ["method `extra` is not a member of trait `Show`"]
        );
    }

    /// An inherent block has no declaration to be measured against, so nothing here applies to
    /// it.
    #[test]
    fn an_inherent_block_may_define_whatever_it_likes() {
        let hir = resolve_src(
            "struct Foo {}
             extend Foo { fun anything(&self) -> i32 {} }",
        );

        assert!(members(&hir).is_empty());
    }

    // -----------------------------------------------------------------
    // Signatures
    // -----------------------------------------------------------------

    #[test]
    fn too_few_parameters_is_reported() {
        let hir = resolve_src(
            "trait Show { fun show(&self, width: i32); }
             struct Foo {}
             extend Foo with Show { fun show(&self) {} }",
        );

        assert_eq!(
            members(&hir),
            ["method `show` takes 0 parameters where its declaration takes 1"]
        );
    }

    #[test]
    fn a_parameter_of_the_wrong_type_is_reported() {
        let hir = resolve_src(
            "trait Show { fun show(&self, width: i32); }
             struct Foo {}
             extend Foo with Show { fun show(&self, width: bool) {} }",
        );

        assert_eq!(
            members(&hir),
            ["parameter `width` of method `show` has type `bool` where its declaration has `i32`"]
        );
    }

    #[test]
    fn a_wrong_return_type_is_reported() {
        let hir = resolve_src(
            "trait Show { fun show(&self) -> i32; }
             struct Foo {}
             extend Foo with Show { fun show(&self) -> bool {} }",
        );

        assert_eq!(
            members(&hir),
            ["method `show` returns `bool` where its declaration returns `i32`"]
        );
    }

    /// Returning nothing is a different thing to say than returning a type, so the wording says
    /// so rather than inventing a `()` the user never wrote.
    #[test]
    fn a_missing_return_type_is_reported() {
        let hir = resolve_src(
            "trait Show { fun show(&self) -> i32; }
             struct Foo {}
             extend Foo with Show { fun show(&self) {} }",
        );

        assert_eq!(
            members(&hir),
            ["method `show` returns nothing where its declaration returns `i32`"]
        );
    }

    #[test]
    fn the_wrong_receiver_mode_is_reported() {
        let hir = resolve_src(
            "trait Show { fun show(&mut self); }
             struct Foo {}
             extend Foo with Show { fun show(&self) {} }",
        );

        assert_eq!(
            members(&hir),
            ["method `show` takes `&self` where its declaration takes `&mut self`"]
        );
    }

    #[test]
    fn a_receiver_where_the_declaration_has_none_is_reported() {
        let hir = resolve_src(
            "trait Show { fun make(); }
             struct Foo {}
             extend Foo with Show { fun make(&self) {} }",
        );

        assert_eq!(
            members(&hir),
            ["method `make` takes `&self` where its declaration takes no receiver"]
        );
    }

    #[test]
    fn a_different_number_of_type_parameters_is_reported() {
        let hir = resolve_src(
            "trait Show { fun show<U>(&self); }
             struct Foo {}
             extend Foo with Show { fun show(&self) {} }",
        );

        assert_eq!(
            members(&hir),
            ["method `show` declares 0 type parameters where its declaration declares 1"]
        );
    }

    /// The two `U`s are different `HirId`s, so making this pass is exactly the renaming step the
    /// substitution does.
    #[test]
    fn a_methods_own_type_parameters_are_matched_up_positionally() {
        let hir = resolve_src(
            "trait Show { fun show<U>(&self, value: U); }
             struct Foo {}
             extend Foo with Show { fun show<U>(&self, value: U) {} }",
        );

        assert!(members(&hir).is_empty());
    }

    /// A signature that would *unify* with the declaration is still wrong: `T` accepts arguments
    /// the trait never promised the implementation would take.
    #[test]
    fn a_signature_that_merely_unifies_is_still_rejected() {
        let hir = resolve_src(
            "trait Show { fun show(&self, width: i32); }
             struct Foo {}
             extend<T> Foo with Show { fun show(&self, width: T) {} }",
        );

        assert_eq!(
            members(&hir),
            ["parameter `width` of method `show` has type `T` where its declaration has `i32`"]
        );
    }

    // -----------------------------------------------------------------
    // Substitution: `Self` and the trait's own parameters
    // -----------------------------------------------------------------

    /// `Self` in the declaration means the implementing type, so both spellings check.
    #[test]
    fn self_in_a_declaration_stands_for_the_implementing_type() {
        let hir = resolve_src(
            "trait Clone { fun clone(&self) -> Self; fun copy(&self) -> Self; }
             struct Foo {}
             extend Foo with Clone { fun clone(&self) -> Foo {} fun copy(&self) -> Self {} }",
        );

        assert!(members(&hir).is_empty());
    }

    #[test]
    fn a_declaration_returning_self_is_not_satisfied_by_another_type() {
        let hir = resolve_src(
            "trait Clone { fun clone(&self) -> Self; }
             struct Foo {}
             struct Bar {}
             extend Foo with Clone { fun clone(&self) -> Bar {} }",
        );

        assert_eq!(
            members(&hir),
            ["method `clone` returns `Bar` where its declaration returns `Foo`"]
        );
    }

    /// The case the substitution exists for: the declaration is written in `K` and `V`, and the
    /// implementation in whatever the block applied the trait to.
    #[test]
    fn a_generic_traits_parameters_are_substituted_from_the_blocks_arguments() {
        let hir = resolve_src(
            "trait Index<K, V> { fun get(&self, key: K) -> V; }
             struct Map {}
             extend Map with Index<i32, bool> { fun get(&self, key: i32) -> bool {} }",
        );

        assert!(members(&hir).is_empty());
    }

    #[test]
    fn a_generic_traits_parameters_are_not_satisfied_by_the_wrong_arguments() {
        let hir = resolve_src(
            "trait Index<K, V> { fun get(&self, key: K) -> V; }
             struct Map {}
             extend Map with Index<i32, bool> { fun get(&self, key: bool) -> bool {} }",
        );

        assert_eq!(
            members(&hir),
            ["parameter `key` of method `get` has type `bool` where its declaration has `i32`"]
        );
    }

    /// The block's own parameters may be what it applies to the trait, in which case the
    /// declaration substitutes to a signature that is itself open.
    #[test]
    fn a_blocks_own_parameters_may_be_the_traits_arguments() {
        let hir = resolve_src(
            "trait Index<K, V> { fun get(&self, key: K) -> V; }
             struct Map<T> { inner: T }
             extend<T> Map<T> with Index<i32, T> { fun get(&self, key: i32) -> T {} }",
        );

        assert!(members(&hir).is_empty());
    }

    /// A composite type is rewritten through, not just a bare `Self` or a bare parameter.
    #[test]
    fn substitution_reaches_inside_composite_types() {
        let hir = resolve_src(
            "trait Index<K, V> { fun get(&self, key: (K, &Self)) -> V; }
             struct Map {}
             extend Map with Index<i32, bool> { fun get(&self, key: (i32, &Map)) -> bool {} }",
        );

        assert!(members(&hir).is_empty());
    }

    // -----------------------------------------------------------------
    // Reading the declaration through the block's substitution
    // -----------------------------------------------------------------

    /// `Self` is replaced wherever it appears, however deeply nested, and nothing else is
    /// touched. This is the half of [`Typeck::subst_sig_ty`] member checking relies on, asked
    /// with an empty parameter substitution so that only the `Self` rule can fire.
    #[test]
    fn substituting_self_rewrites_every_occurrence_and_only_those() {
        let hir = resolve_src("struct Foo {}");
        let mut checker = Typeck::new(&hir);
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

        let no_params = HashMap::new();
        assert_eq!(checker.subst_sig_ty(self_param, &no_params, foo_ty), foo_ty);

        let substituted = checker.subst_sig_ty(nested, &no_params, foo_ty);
        assert_eq!(
            *checker.tcx.kind(substituted),
            TyKind::Tuple(vec![foo_ty, i32_ty])
        );
        assert_eq!(
            checker.subst_sig_ty(i32_ty, &no_params, foo_ty),
            i32_ty,
            "a type with no `Self` in it comes back unchanged"
        );
    }
}
