use crate::testing::{typeck_accepts, typeck_rejects, typeck_src};

const PRELUDE: &str = "trait Show { fun show(&self); }
                       struct Foo {}
                       struct Bare {}
                       struct Wrap<T> { inner: T }
                       struct Sorted<T: Show> { inner: T }
                       extend Foo with Show { fun show(&self) {} }";

fn src(rest: &str) -> String {
    format!("{PRELUDE}\n{rest}")
}

#[test]
fn a_bound_in_scope_is_what_discharges_a_bound_on_a_parameter() {
    typeck_accepts(&src("fun f<U: Show>(x: Sorted<U>) {}"));
}

#[test]
fn a_parameter_without_the_bound_is_still_rejected() {
    typeck_rejects(
        &src("fun f<U>(x: Sorted<U>) {}"),
        "the trait bound `U: Show` is not satisfied",
    );
}

#[test]
fn an_obligation_carries_the_definition_whose_assumptions_prove_it() {
    let messages = typeck_src(&src("fun f<U: Show>(x: Sorted<U>) {}
         fun g<V>(x: Sorted<V>) {}"));
    assert_eq!(
        messages,
        ["the trait bound `V: Show` is not satisfied"],
        "{messages:?}"
    );
}

#[test]
fn a_method_is_proved_against_its_extend_blocks_bounds() {
    typeck_accepts(&src(
        "extend<T: Show> Wrap<T> { fun get(&self, x: Sorted<T>) {} }",
    ));
}

#[test]
fn a_traits_default_method_can_call_the_trait_it_is_declared_in() {
    typeck_accepts(
        "trait Show {
             fun show(&self);
             fun show_twice(&self) { self.show(); self.show(); }
         }",
    );
}

#[test]
fn a_conditional_impl_only_applies_when_its_own_bound_holds() {
    let conditional = "extend<T: Show> Wrap<T> with Show { fun show(&self) {} }";
    typeck_accepts(&src(&format!(
        "{conditional} fun f(x: Sorted<Wrap<Foo>>) {{}}"
    )));
    typeck_rejects(
        &src(&format!("{conditional} fun f(x: Sorted<Wrap<Bare>>) {{}}")),
        "the trait bound `Wrap<Bare>: Show` is not satisfied",
    );
}

#[test]
fn a_reference_to_an_implementing_type_does_not_itself_implement() {
    typeck_rejects(
        &src("trait Holds<T: Show> {}
              fun f(x: &dyn Holds<&Foo>) {}"),
        "the trait bound `&Foo: Show` is not satisfied",
    );
}

#[test]
fn a_dyn_satisfies_the_trait_it_names_and_no_other() {
    let messages = typeck_src(
        "trait Show { fun show(&self); }
         trait Other { fun other(&self); }
         trait Container<T: Show> { fun get(&self) -> T; }
         fun f(x: &dyn Container<dyn Show>) {}
         fun g(x: &dyn Container<dyn Other>) {}",
    );
    assert_eq!(
        messages,
        ["the trait bound `dyn Other: Show` is not satisfied"],
        "{messages:?}"
    );
}

#[test]
fn an_unresolved_goal_is_answered_the_same_way_whatever_the_declaration_order() {
    let foo = "extend Wrap<Foo> with Add \
               { fun add(&self, other: &Self) -> Self { return .{ inner: self.inner }; } }";
    let bare = "extend Wrap<Bare> with Add \
                { fun add(&self, other: &Self) -> Self { return .{ inner: self.inner }; } }";
    let program = |blocks: String| {
        format!(
            "module core::ops;
             public trait Add {{ fun add(&self, other: &Self) -> Self; }}
             struct Foo {{}}
             struct Bare {{}}
             struct Wrap<T> {{ inner: T }}
             {blocks}
             fun make<T>() -> Wrap<T> {{ return make(); }}
             fun f(w: Wrap<Foo>) {{ let mut v = make(); let s = v + v; v = w; }}"
        )
    };

    let first = typeck_src(&program(format!("{foo}\n{bare}")));
    let second = typeck_src(&program(format!("{bare}\n{foo}")));
    assert_eq!(first, second, "the answer depends on declaration order");
    assert_eq!(
        first,
        ["`Wrap<_>` does not implement `Add`"],
        "the operator was answered about a type that is still unknown, and neither block was \
         guessed at: `_` is still `_`"
    );
}

#[test]
fn a_bound_is_proved_after_the_body_that_resolves_its_type() {
    typeck_accepts(&src("fun make<T: Show>() -> T { return make(); }
         fun f(a: Foo) { let mut b = make(); b = a; }"));
}

#[test]
fn a_bound_that_never_resolves_is_reported() {
    typeck_rejects(
        &src("fun sort<T: Show>() -> T { return sort(); }
             fun f() { let x = sort(); }"),
        "type annotations needed",
    );
}

#[test]
fn a_bound_about_an_unresolvable_type_is_not_reported_twice() {
    let messages = typeck_src(&src("fun f(x: Sorted<Nope>) {}"));
    assert!(messages.is_empty(), "{messages:?}");
}

#[test]
fn an_unmet_bound_points_at_the_bound_that_requires_it() {
    let hir = crate::testing::lower_to_hir(&src("fun f(x: Sorted<Bare>) {}"));
    crate::testing::clear_diagnostics();
    crate::typeck::check(crate::testing::session(), &hir);

    let diagnostics = crate::testing::diagnostics();
    let [unmet] = diagnostics.as_slice() else {
        panic!("expected exactly one diagnostic, got {diagnostics:?}");
    };
    let [declared] = unmet.secondary.as_slice() else {
        panic!(
            "expected exactly one secondary label, got {:?}",
            unmet.secondary
        );
    };
    assert_eq!(declared.message, "required by this bound");

    let primary = unmet.span.expect("an unmet bound names a place");
    assert!(declared.span.get_begin() < primary.get_begin());
}

#[test]
fn a_method_from_a_conditional_block_needs_that_blocks_bound() {
    let conditional = "extend<T: Show> Wrap<T> with Show { fun show(&self) {} }";
    typeck_accepts(&src(&format!(
        "{conditional} fun f(x: Wrap<Foo>) {{ x.show(); }}"
    )));
    typeck_rejects(
        &src(&format!(
            "{conditional} fun f(x: Wrap<Bare>) {{ x.show(); }}"
        )),
        "the trait bound `Bare: Show` is not satisfied",
    );
}

#[test]
fn two_conflicting_impls_are_reported_once_at_their_declarations() {
    let messages = typeck_src(&src("extend Foo with Show { fun show(&self) {} }
         fun f(x: Sorted<Foo>) {}"));
    assert_eq!(messages.len(), 2, "{messages:?}");
    assert!(
        messages
            .iter()
            .all(|message| message.contains("conflicting") || message.contains("more than once")),
        "{messages:?}"
    );
}
