use crate::diagnostics::typeck::traits::validity::{
    report_arg_count_mismatch, report_bound_is_not_a_trait,
};
use crate::driver::source::SrcSpan;
use crate::hir::{DefId, Hir, HirId, OwnerNode, Path, Res, TyDef, Type};
use crate::typeck::ty::TyKind;
use crate::typeck::Typeck;

impl<'hir> Typeck<'hir> {
    // -----------------------------------------------------------------
    // Validity: is this a bound at all?
    // -----------------------------------------------------------------

    /// Checks that every bound written anywhere in the program names a trait.
    pub fn check_declared_bounds(&mut self) {
        let hir = self.hir;
        for def in hir.def_ids() {
            for &generic in self.declared_generics(def) {
                for path in &hir.generic(generic).bounds {
                    Self::check_declared_bound(path);
                }
            }
        }
    }

    fn check_declared_bound(path: &Path) {
        match path.res {
            Res::Type(Type::Def(TyDef::Trait(_))) => {}
            Res::Err => {}
            _ => report_bound_is_not_a_trait(path),
        }
    }

    // -----------------------------------------------------------------
    // Validity: does this argument list fit?
    // -----------------------------------------------------------------

    /// Checks arity of the argument lists of every `extend` block, and registers what they have to satisfy.
    pub fn check_extend_headers(&mut self) {
        for block in self.extends.all() {
            let (self_ty, trait_ref) = (
                self.adt_of_with_args(block),
                self.extends.trait_of(block).cloned(),
            );

            let TyKind::Adt { def, args } = self.tcx.kind(self_ty).clone() else {
                unreachable!("an indexed extend block's self type is always an ADT");
            };
            let node = self.hir.extend(block);
            let (adt_path, trait_path) = (&node.adt_path, node.trait_path.as_ref());

            if self.check_arg_count(def, args.len(), adt_path.span) {
                self.register_bound_obligations(def, &args, adt_path.span, block);
            }

            if let Some(trait_ref) = trait_ref {
                let span = trait_path.map_or(node.span, |path| path.span);
                if self.check_arg_count(trait_ref.def, trait_ref.args.len(), span) {
                    self.register_bound_obligations(trait_ref.def, &trait_ref.args, span, block);
                }
            }
        }
    }

    pub fn check_arg_count(&self, def: DefId, found: usize, span: SrcSpan) -> bool {
        let declared = self.declared_generics(def).len();
        if declared == found {
            return true;
        }

        report_arg_count_mismatch(self.hir, def, declared, found, span);
        false
    }

    pub(crate) fn declared_generics(&self, def: DefId) -> &'hir [HirId] {
        let hir: &'hir Hir = self.hir;
        match hir.def(def) {
            OwnerNode::Function(f) => &f.generics,
            OwnerNode::Struct(s) => &s.generics,
            OwnerNode::Enum(e) => &e.generics,
            OwnerNode::Trait(t) => &t.generics,
            OwnerNode::Extend(e) => &e.extend_generics,
            OwnerNode::Module(_) | OwnerNode::Closure(_) => &[],
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::diagnostics::DiagCtx;
    use crate::testing::{checker_through, messages, resolve_src, Stage};

    /// Runs everything up to and including header/bound validity checking over `src`, and hands
    /// back everything type checking reported.
    ///
    /// The clear comes *before* collection rather than after it, which is where `coherence`'s and
    /// `members`'s helpers put theirs. `check_arg_count` reports immediately rather than
    /// deferring, and one of its call sites is in `lower_ty`, which runs during collection, so
    /// clearing afterwards would hide exactly what that site says. What is cleared is name
    /// resolution's own output: a fixture is resolved without the core library, so every one of
    /// them reports the whole set of missing lang items first.
    ///
    /// Bodies are deliberately not checked, so that what is exercised here is the program-level
    /// context on its own; a fixture that needs the per-body one instead goes through
    /// [`crate::testing::typeck_rejects`], which runs the whole pipeline.
    fn validity(hir: &Hir) -> Vec<String> {
        DiagCtx::clear();

        let mut checker = checker_through(hir, Stage::Members);
        checker.check_declared_bounds();
        checker.check_extend_headers();

        messages()
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

        assert_eq!(validity(&hir), ["`Foo` is not a trait"]);
    }

    /// The other nominal thing a bound can name. A primitive cannot be written in bound position
    /// at all, since a bound is parsed as a path of identifiers and a primitive is a keyword,
    /// which is why the check is phrased over what the path *resolved* to rather than over a
    /// list of kinds it might have been.
    #[test]
    fn a_bound_naming_an_enum_is_reported() {
        let hir = resolve_src(
            "enum Direction { up, down }
             fun f<T: Direction>(x: T) {}",
        );

        assert_eq!(validity(&hir), ["`Direction` is not a trait"]);
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

        assert_eq!(validity(&hir), ["`Foo` is not a trait"]);
    }

    #[test]
    fn a_bound_that_did_not_resolve_reports_nothing_further() {
        let hir = resolve_src("fun f<T: Nope>(x: T) {}");

        assert!(
            validity(&hir).is_empty(),
            "name resolution already reported the missing name"
        );
    }

    #[test]
    fn a_bound_naming_a_trait_is_accepted() {
        let hir = resolve_src(
            "trait Show { fun show(&self); }
             fun f<T: Show>(x: T) {}",
        );

        assert!(validity(&hir).is_empty());
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
            validity(&hir),
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

        assert!(validity(&hir).is_empty());
    }

    /// Unlike a written annotation, the type an `extend` block names is built without counting,
    /// so this is the first place that count is ever checked.
    #[test]
    fn an_extend_block_applying_the_wrong_number_of_arguments_is_reported() {
        let hir = resolve_src(
            "struct Wrap<T> { inner: T }
             extend Wrap<i32, bool> { fun get(&self) {} }",
        );

        assert_eq!(
            validity(&hir),
            ["`Wrap` takes 1 generic argument but 2 were supplied"]
        );
    }

    /// A `dyn` is an application of the trait's parameters like any other, so leaving them off a
    /// trait that declares some is the same mistake as leaving them off a struct, and, since
    /// `dyn` carries its own argument list, one with a spelling that fixes it.
    #[test]
    fn a_dyn_naming_a_trait_with_parameters_is_reported() {
        let hir = resolve_src(
            "trait Index<K, V> { fun get(&self, key: K) -> V; }
             fun f(x: &dyn Index) {}",
        );

        assert_eq!(
            validity(&hir),
            ["`Index` takes 2 generic arguments but 0 were supplied"]
        );
    }
}
