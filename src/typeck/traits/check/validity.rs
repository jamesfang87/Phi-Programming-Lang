use crate::diagnostics::typeck::traits::validity::{
    report_arg_count_mismatch, report_bound_is_not_a_trait,
};
use crate::driver::source::SrcSpan;
use crate::hir::{DefId, HirId, Path, Res, TyDef, Type};
use crate::session::Session;
use crate::typeck::Typeck;
use crate::typeck::ty::TyKind;

impl<'hir> Typeck<'hir> {
    /// Checks that every bound is a trait (and not another type)
    pub fn check_all_bounds_are_traits(&mut self) {
        for def in self.hir.def_ids() {
            for &generic in self.declared_generics(def) {
                self.check_generic_bounds_are_traits(generic);
            }
        }
    }

    /// Checks that every bound for a type parameter is a trait.
    fn check_generic_bounds_are_traits(&self, generic: HirId) {
        for bound in &self.hir.generic(generic).bounds {
            Self::check_bound_is_a_trait(self.session, &bound.path);
        }
    }

    fn check_bound_is_a_trait(session: &Session, path: &Path) {
        match path.res {
            Res::Type(Type::Def(TyDef::Trait(_))) | Res::Err => {}
            _ => report_bound_is_not_a_trait(session, path),
        }
    }

    pub fn check_extend_headers_arity(&mut self) {
        for block in self.extends.all() {
            self.check_extend_header_arity(block);
        }
    }

    fn check_extend_header_arity(&mut self, block: DefId) {
        self.check_extended_type_arity(block);
        self.check_implemented_trait_arity(block);
    }

    /// Checks that the extended type supplies as many arguments as its declaration takes.
    fn check_extended_type_arity(&mut self, block: DefId) {
        let self_ty = self.extended_type(block);
        let span = self.hir.ty(self.hir.extend(block).self_ty).span;
        if let TyKind::Adt { def, args } = self.tcx.kind(self_ty).clone() {
            self.check_arg_count(def, args.len(), span);
        }
    }

    /// Checks that the `with`-clause trait supplies as many arguments as its declaration takes.
    fn check_implemented_trait_arity(&mut self, block: DefId) {
        let Some(trait_ref) = self.extends.trait_of(block).cloned() else {
            return;
        };
        let span = self.trait_path_span(block);
        self.check_arg_count(trait_ref.def, trait_ref.args.len(), span);
    }

    /// Checks that the number of supplied arguments matches the declared arity of the def
    /// If it does not match, a diagnostic is emitted
    pub fn check_arg_count(&self, def: DefId, found: usize, span: SrcSpan) -> bool {
        let declared = self.declared_generics(def).len();
        if declared == found {
            return true;
        }

        report_arg_count_mismatch(self.session, self.hir, def, declared, found, span);
        false
    }
}

#[cfg(test)]
mod tests {
    use crate::hir::Hir;
    use crate::testing::{TypeckStage, checker_through, lower_to_hir};

    fn validity(hir: &Hir) -> Vec<String> {
        crate::testing::clear_diagnostics();

        let mut checker = checker_through(hir, TypeckStage::Members);
        checker.check_all_bounds_are_traits();
        checker.check_extend_headers_arity();
        checker.register_extend_header_bounds();

        crate::testing::messages()
    }

    #[test]
    fn a_bound_naming_a_struct_is_reported() {
        let hir = lower_to_hir(
            "struct Foo {}
             fun f<T: Foo>(x: T) {}",
        );

        assert_eq!(validity(&hir), ["`Foo` is not a trait"]);
    }

    #[test]
    fn a_bound_naming_an_enum_is_reported() {
        let hir = lower_to_hir(
            "enum Direction { up, down }
             fun f<T: Direction>(x: T) {}",
        );

        assert_eq!(validity(&hir), ["`Direction` is not a trait"]);
    }

    #[test]
    fn a_bad_bound_on_an_extend_block_is_reported_once() {
        let hir = lower_to_hir(
            "struct Foo {}
             struct Wrap<T> { inner: T }
             extend<T: Foo> Wrap<T> { fun a(&self) {} fun b(&self) {} }",
        );

        assert_eq!(validity(&hir), ["`Foo` is not a trait"]);
    }

    #[test]
    fn a_bound_that_did_not_resolve_reports_nothing_further() {
        let hir = lower_to_hir("fun f<T: Nope>(x: T) {}");

        assert!(
            validity(&hir).is_empty(),
            "name resolution already reported the missing name"
        );
    }

    #[test]
    fn a_bound_naming_a_trait_is_accepted() {
        let hir = lower_to_hir(
            "trait Show { fun show(&self); }
             fun f<T: Show>(x: T) {}",
        );

        assert!(validity(&hir).is_empty());
    }

    #[test]
    fn a_with_clause_missing_the_traits_arguments_is_reported() {
        let hir = lower_to_hir(
            "trait Index<K, V> { fun get(&self, key: K) -> V; }
             struct Map {}
             extend Map with Index { fun get(&self, key: i32) -> bool {} }",
        );

        assert_eq!(
            validity(&hir),
            ["`Index` takes 2 generic arguments but 0 were supplied"]
        );
    }

    #[test]
    fn a_primitive_with_clause_missing_the_traits_arguments_is_reported() {
        let hir = lower_to_hir(
            "trait Index<K, V> { fun get(&self, key: K) -> V; }
             extend i32 with Index { fun get(&self, key: i32) -> bool { return true; } }",
        );

        assert_eq!(
            validity(&hir),
            ["`Index` takes 2 generic arguments but 0 were supplied"]
        );
    }

    #[test]
    fn a_with_clause_with_the_right_number_of_arguments_is_accepted() {
        let hir = lower_to_hir(
            "trait Index<K, V> { fun get(&self, key: K) -> V; }
             struct Map {}
             extend Map with Index<i32, bool> { fun get(&self, key: i32) -> bool {} }",
        );

        assert!(validity(&hir).is_empty());
    }

    #[test]
    fn an_extend_block_applying_the_wrong_number_of_arguments_is_reported() {
        let hir = lower_to_hir(
            "struct Wrap<T> { inner: T }
             extend Wrap<i32, bool> { fun get(&self) {} }",
        );

        assert_eq!(
            validity(&hir),
            ["`Wrap` takes 1 generic argument but 2 were supplied"]
        );
    }

    #[test]
    fn a_dyn_naming_a_trait_with_parameters_is_reported() {
        let hir = lower_to_hir(
            "trait Index<K, V> { fun get(&self, key: K) -> V; }
             fun f(x: &dyn Index) {}",
        );

        assert_eq!(
            validity(&hir),
            ["`Index` takes 2 generic arguments but 0 were supplied"]
        );
    }
}
