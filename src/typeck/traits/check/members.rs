use std::collections::{HashMap, HashSet};

use crate::ast::Symbol;
use crate::diagnostics::typeck::traits::members::{
    report_generic_count, report_missing_methods, report_not_a_member, report_param_count,
    report_param_ty, report_ret_ty, report_self_mode,
};
use crate::hir::{DefId, Function, HirId};
use crate::typeck::Typeck;
use crate::typeck::traits::TraitRef;
use crate::typeck::ty::Ty;

impl<'hir> Typeck<'hir> {
    /// Checks that every extend-with block (1) implements every method defined in the trait
    /// (2) we do not implement extra methods (or methods with the wrong signature)
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
        let self_ty = self.extended_type(block);

        self.check_for_missing_methods(block, &trait_ref, self_ty);
        self.check_provided_methods(block, &trait_ref, self_ty);
    }

    /// Checks that the block implements every method the trait declares without a default body.
    fn check_for_missing_methods(&self, block: DefId, trait_ref: &TraitRef, self_ty: Ty) {
        let hir = self.hir;
        let node = hir.extend(block);
        let declared = &hir.trait_(trait_ref.def).functions;

        let present: HashSet<Symbol> = node
            .methods
            .iter()
            .map(|&method| hir.function(method).name.text)
            .collect();

        // Kept as definitions rather than reduced to names, so the diagnostic can underline each
        // one where the trait declares it.
        let missing: Vec<DefId> = declared
            .iter()
            .copied()
            .filter(|&declaration| {
                let declaration = hir.function(declaration);
                declaration.block.is_none() && !present.contains(&declaration.name.text)
            })
            .collect();

        if !missing.is_empty() {
            report_missing_methods(
                hir,
                self.display_cx(),
                &missing,
                trait_ref,
                self_ty,
                node.span,
            );
        }
    }

    /// Checks that the extend block does not declare an extra method or one with an incorrect
    /// signature
    fn check_provided_methods(&mut self, block: DefId, trait_ref: &TraitRef, self_ty: Ty) {
        let hir = self.hir;
        let provided = &hir.extend(block).methods;
        let subst = self.subst_trait_type_parameters(trait_ref);

        for &method in provided {
            self.check_provided_method(method, trait_ref, subst.as_ref(), self_ty);
        }
    }

    /// Checks that the trait declares a method with the same name and signature
    fn check_provided_method(
        &mut self,
        method: DefId,
        trait_ref: &TraitRef,
        substitution: Option<&HashMap<HirId, Ty>>,
        self_ty: Ty,
    ) {
        let name = self.hir.function(method).name.text;
        match self.trait_method(trait_ref.def, name) {
            None => report_not_a_member(self.hir, self.display_cx(), method, trait_ref, self_ty),
            Some(declaration) => {
                if let Some(substitution) = substitution {
                    self.check_method_signature(method, declaration, substitution, self_ty);
                }
            }
        }
    }

    /// Returns a substitution mapping a trait's type parameters to the extend block's arguments
    /// In the case of an arity mismatch, returns `None`
    fn subst_trait_type_parameters(&self, trait_ref: &TraitRef) -> Option<HashMap<HirId, Ty>> {
        let arity_matches = self.declared_generics(trait_ref.def).len() == trait_ref.args.len();
        arity_matches.then(|| self.trait_subst(trait_ref.def, &trait_ref.args))
    }

    /// Checks that an implementation method's signature matches its declaration
    fn check_method_signature(
        &mut self,
        method: DefId,
        declaration: DefId,
        trait_subst: &HashMap<HirId, Ty>,
        self_ty: Ty,
    ) {
        let (found, expected) = (self.hir.function(method), self.hir.function(declaration));

        if !self.check_method_generics_arity(found, expected) {
            return;
        }

        if !self.check_method_receiver(method, declaration) {
            return;
        }

        let subst = self.method_subst(found, expected, trait_subst);
        let (found_params, found_ret) = self.collected_signature(method);
        let (expected_params, expected_ret) = self.expected_signature(declaration, &subst, self_ty);

        if self.check_method_arity(found, expected, &found_params, &expected_params) {
            self.check_method_param_types(found, expected, &found_params, &expected_params);
        }
        self.check_method_return(found, expected, found_ret, expected_ret);
    }

    /// Returns the expected function signature of a method declared in a trait
    fn expected_signature(
        &mut self,
        declaration: DefId,
        subst: &HashMap<HirId, Ty>,
        self_ty: Ty,
    ) -> (Vec<Ty>, Option<Ty>) {
        let (params, ret) = self.collected_signature(declaration);
        let params = params
            .into_iter()
            .map(|ty| self.subst_sig_ty(ty, subst, self_ty))
            .collect();
        let ret = ret.map(|ty| self.subst_sig_ty(ty, subst, self_ty));
        (params, ret)
    }

    /// Checks if the implementation declares the same amount of type parameters as its declaration.
    fn check_method_generics_arity(&self, found: &Function, expected: &Function) -> bool {
        if found.generics.len() == expected.generics.len() {
            return true;
        }
        report_generic_count(self.session, found, expected);
        false
    }

    /// Returns a substitution from the declared method's type parameters and implementation method's
    /// type parameters
    fn method_subst(
        &mut self,
        found: &Function,
        expected: &Function,
        trait_subst: &HashMap<HirId, Ty>,
    ) -> HashMap<HirId, Ty> {
        let mut subst = trait_subst.clone();
        for (&declared, &implemented) in expected.generics.iter().zip(found.generics.iter()) {
            subst.insert(declared, self.tcx.mk_generic(implemented));
        }
        subst
    }

    /// Checks the implementation's receiver matches the declaration's receiver.
    fn check_method_receiver(&self, method: DefId, declaration: DefId) -> bool {
        let (found, expected) = (self.hir.function(method), self.hir.function(declaration));
        let (found_mode, expected_mode) =
            (self.receiver_mode(method), self.receiver_mode(declaration));
        if found_mode == expected_mode {
            return true;
        }
        report_self_mode(
            self.session,
            self.hir,
            found,
            expected,
            found_mode,
            expected_mode,
        );
        false
    }

    fn collected_signature(&mut self, def: DefId) -> (Vec<Ty>, Option<Ty>) {
        self.signature(def)
            .expect("collect_function records every function's own signature")
    }

    /// Checks the implementation takes the same number of parameters as its declaration.
    fn check_method_arity(
        &self,
        found: &Function,
        expected: &Function,
        found_params: &[Ty],
        expected_params: &[Ty],
    ) -> bool {
        if found_params.len() == expected_params.len() {
            return true;
        }
        report_param_count(
            self.session,
            found,
            expected,
            found_params.len(),
            expected_params.len(),
        );
        false
    }

    /// Checks every written parameter against its declaration, skipping the receiver, which was
    /// already checked.
    fn check_method_param_types(
        &mut self,
        found: &Function,
        expected: &Function,
        found_params: &[Ty],
        expected_params: &[Ty],
    ) {
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
    }

    fn check_method_return(
        &self,
        found: &Function,
        expected: &Function,
        found_ret: Option<Ty>,
        expected_ret: Option<Ty>,
    ) {
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

    use crate::hir::{Hir, OwnerNode};
    use crate::nameres::PrimTy;
    use crate::testing::{TypeckStage, checker_through, lower_to_hir};
    use crate::typeck::Typeck;
    use crate::typeck::ty::TyKind;

    fn members(hir: &Hir) -> Vec<String> {
        let mut checker = checker_through(hir, TypeckStage::Coherence);
        crate::testing::clear_diagnostics();
        checker.check_trait_members();
        crate::testing::messages()
    }

    #[test]
    fn an_implementation_providing_exactly_the_declared_methods_is_accepted() {
        let hir = lower_to_hir(
            "trait Show { fun show(&self); }
             struct Foo {}
             extend Foo with Show { fun show(&self) {} }",
        );

        assert!(members(&hir).is_empty());
    }

    #[test]
    fn a_missing_method_is_reported() {
        let hir = lower_to_hir(
            "trait Show { fun show(&self); }
             struct Foo {}
             extend Foo with Show {}",
        );

        assert_eq!(
            members(&hir),
            ["missing method in the implementation of trait `Show` for `Foo`: `show`"]
        );
    }

    #[test]
    fn every_missing_method_is_named_in_one_diagnostic() {
        let hir = lower_to_hir(
            "trait Show { fun show(&self); fun size(&self); }
             struct Foo {}
             extend Foo with Show {}",
        );

        assert_eq!(
            members(&hir),
            ["missing methods in the implementation of trait `Show` for `Foo`: `show`, `size`"]
        );
    }

    #[test]
    fn every_missing_method_is_underlined_where_it_is_declared() {
        let hir = lower_to_hir(
            "trait Show { fun show(&self); fun size(&self); fun free(&self) {} }
             struct Foo {}
             extend Foo with Show {}",
        );

        let mut checker = Typeck::new(crate::testing::session(), &hir);
        checker.collect_module(hir.root_id());
        checker.collect_traits();
        checker.check_coherence();
        crate::testing::clear_diagnostics();
        checker.check_trait_members();

        let diagnostics = crate::testing::diagnostics();
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

    #[test]
    fn a_method_with_a_default_body_need_not_be_implemented() {
        let hir = lower_to_hir(
            "trait Show { fun show(&self) {} }
             struct Foo {}
             extend Foo with Show {}",
        );

        assert!(members(&hir).is_empty());
    }

    #[test]
    fn a_method_with_a_default_body_may_still_be_overridden() {
        let hir = lower_to_hir(
            "trait Show { fun show(&self) {} }
             struct Foo {}
             extend Foo with Show { fun show(&self) {} }",
        );

        assert!(members(&hir).is_empty());
    }

    #[test]
    fn a_method_the_trait_does_not_declare_is_reported() {
        let hir = lower_to_hir(
            "trait Show { fun show(&self); }
             struct Foo {}
             extend Foo with Show { fun show(&self) {} fun extra(&self) {} }",
        );

        assert_eq!(
            members(&hir),
            ["method `extra` is not a member of trait `Show`"]
        );
    }

    #[test]
    fn an_inherent_block_may_define_whatever_it_likes() {
        let hir = lower_to_hir(
            "struct Foo {}
             extend Foo { fun anything(&self) -> i32 {} }",
        );

        assert!(members(&hir).is_empty());
    }

    #[test]
    fn too_few_parameters_is_reported() {
        let hir = lower_to_hir(
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
        let hir = lower_to_hir(
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
        let hir = lower_to_hir(
            "trait Show { fun show(&self) -> i32; }
             struct Foo {}
             extend Foo with Show { fun show(&self) -> bool {} }",
        );

        assert_eq!(
            members(&hir),
            ["method `show` returns `bool` where its declaration returns `i32`"]
        );
    }

    #[test]
    fn a_missing_return_type_is_reported() {
        let hir = lower_to_hir(
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
        let hir = lower_to_hir(
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
        let hir = lower_to_hir(
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
        let hir = lower_to_hir(
            "trait Show { fun show<U>(&self); }
             struct Foo {}
             extend Foo with Show { fun show(&self) {} }",
        );

        assert_eq!(
            members(&hir),
            ["method `show` declares 0 type parameters where its declaration declares 1"]
        );
    }

    #[test]
    fn a_methods_own_type_parameters_are_matched_up_positionally() {
        let hir = lower_to_hir(
            "trait Show { fun show<U>(&self, value: U); }
             struct Foo {}
             extend Foo with Show { fun show<U>(&self, value: U) {} }",
        );

        assert!(members(&hir).is_empty());
    }

    #[test]
    fn a_signature_that_merely_unifies_is_still_rejected() {
        let hir = lower_to_hir(
            "trait Show { fun show(&self, width: i32); }
             struct Foo {}
             extend<T> Foo with Show { fun show(&self, width: T) {} }",
        );

        assert_eq!(
            members(&hir),
            ["parameter `width` of method `show` has type `T` where its declaration has `i32`"]
        );
    }

    #[test]
    fn self_in_a_declaration_stands_for_the_implementing_type() {
        let hir = lower_to_hir(
            "trait Clone { fun clone(&self) -> Self; fun copy(&self) -> Self; }
             struct Foo {}
             extend Foo with Clone { fun clone(&self) -> Foo {} fun copy(&self) -> Self {} }",
        );

        assert!(members(&hir).is_empty());
    }

    #[test]
    fn a_declaration_returning_self_is_not_satisfied_by_another_type() {
        let hir = lower_to_hir(
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

    #[test]
    fn a_generic_traits_parameters_are_substituted_from_the_blocks_arguments() {
        let hir = lower_to_hir(
            "trait Index<K, V> { fun get(&self, key: K) -> V; }
             struct Map {}
             extend Map with Index<i32, bool> { fun get(&self, key: i32) -> bool {} }",
        );

        assert!(members(&hir).is_empty());
    }

    #[test]
    fn a_generic_traits_parameters_are_not_satisfied_by_the_wrong_arguments() {
        let hir = lower_to_hir(
            "trait Index<K, V> { fun get(&self, key: K) -> V; }
             struct Map {}
             extend Map with Index<i32, bool> { fun get(&self, key: bool) -> bool {} }",
        );

        assert_eq!(
            members(&hir),
            ["parameter `key` of method `get` has type `bool` where its declaration has `i32`"]
        );
    }

    #[test]
    fn a_blocks_own_parameters_may_be_the_traits_arguments() {
        let hir = lower_to_hir(
            "trait Index<K, V> { fun get(&self, key: K) -> V; }
             struct Map<T> { inner: T }
             extend<T> Map<T> with Index<i32, T> { fun get(&self, key: i32) -> T {} }",
        );

        assert!(members(&hir).is_empty());
    }

    #[test]
    fn substitution_reaches_inside_composite_types() {
        let hir = lower_to_hir(
            "trait Index<K, V> { fun get(&self, key: (K, &Self)) -> V; }
             struct Map {}
             extend Map with Index<i32, bool> { fun get(&self, key: (i32, &Map)) -> bool {} }",
        );

        assert!(members(&hir).is_empty());
    }

    #[test]
    fn substituting_self_rewrites_every_occurrence_and_only_those() {
        let hir = lower_to_hir("struct Foo {}");
        let mut checker = Typeck::new(crate::testing::session(), &hir);
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
