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

// -----------------------------------------------------------------
// The environment, before the index
// -----------------------------------------------------------------

/// `BoundsEnv`, and step 2 of [`implements`](crate::typeck::Typeck::implements).
///
/// Nothing is known about `U` except what `f` declares, and no `extend` block will ever match a
/// bare type parameter, so the index cannot answer `U: Show` at all. Without the environment step
/// this program is rejected and no generic function could ever pass its own parameter on.
#[test]
fn a_bound_in_scope_is_what_discharges_a_bound_on_a_parameter() {
    typeck_accepts(&src("fun f<U: Show>(x: Sorted<U>) {}"));
}

/// The same program with the bound taken off the declaration, which is what makes the test above
/// an experiment rather than a tautology: the environment answers `U: Show` because `f` declared
/// it, not because a parameter is unanswerable and waved through.
#[test]
fn a_parameter_without_the_bound_is_still_rejected() {
    typeck_rejects(
        &src("fun f<U>(x: Sorted<U>) {}"),
        "the trait bound `U: Show` is not satisfied",
    );
}

/// The `DefId` `register_bound_obligations` files each obligation under.
///
/// Two instantiations of the same `Sorted<U>`, in two functions that assume different things. The
/// goal is stored long after the declaration it was written in is off the stack, so without the
/// owner recorded alongside it there is one environment for both and the two answers cannot
/// differ: proving them against `g`'s assumptions rejects `f` as well (two diagnostics), and
/// proving them against `f`'s accepts `g` (none).
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

/// `bounds_env` walking the parent chain.
///
/// `get` declares no parameters of its own, so the `T: Show` its parameter type needs can only
/// come from the block enclosing it. Without the walk the method sees an empty environment and
/// its own signature is rejected.
#[test]
fn a_method_is_proved_against_its_extend_blocks_bounds() {
    typeck_accepts(&src(
        "extend<T: Show> Wrap<T> { fun get(&self, x: Sorted<T>) {} }",
    ));
}

/// The implicit `Self: ThisTrait` that `bounds_env` adds inside a trait.
///
/// `show_twice` calls `show` on a `Self` whose only description is the trait being declared. Method
/// resolution finds `show` through the environment, so without that one entry the call has no
/// candidate at all and a default method cannot call anything the trait declares.
#[test]
fn a_traits_default_method_can_call_the_trait_it_is_declared_in() {
    typeck_accepts(
        "trait Show {
             fun show(&self);
             fun show_twice(&self) { self.show(); self.show(); }
         }",
    );
}

// -----------------------------------------------------------------
// What may implement what
// -----------------------------------------------------------------

/// Step 6, the recursion into the selected block's own obligations.
///
/// `Wrap<Foo>: Show` holds only because `Foo: Show` does, and the second question is not written
/// anywhere in this program: it exists because the block that answered the first one declares
/// `<T: Show>`. Without the recursion the block's bound is never demanded of anything and the
/// rejection below disappears.
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

/// Step 4: only a struct, an enum, or a `dyn` implements anything.
///
/// `Foo` implements `Show` and `&Foo` does not, which is the whole of the rule. The tempting
/// simplification is to let the query see through a reference to what it points at, since a
/// method call on a `&Foo` reaches `Foo`'s methods perfectly well; a solver that peeled the
/// reference accepts this program, and `Holds<&Foo>` would then bind a parameter whose type
/// satisfies no bound. Peeling belongs to receiver adjustment in method resolution, where the
/// call knows how many layers it took off, and not here.
///
/// This is exercised through a `dyn`'s own generic argument rather than through `Sorted<U>`,
/// the struct this module's other tests instantiate: a struct or enum can no longer be
/// instantiated with a reference argument at all, so `Sorted<&Foo>` is rejected before its
/// bound is even considered, and could no longer isolate step 4 on its own.
#[test]
fn a_reference_to_an_implementing_type_does_not_itself_implement() {
    typeck_rejects(
        &src("trait Holds<T: Show> {}
              fun f(x: &dyn Holds<&Foo>) {}"),
        "the trait bound `&Foo: Show` is not satisfied",
    );
}

/// Step 3: a `dyn Show` satisfies `Show`, and only `Show`.
///
/// There is no `extend` block behind a `dyn`, so the index has nothing to match and nothing could
/// ever answer this goal; the trait a `dyn` names is the whole of what it implements, and that is
/// a rule rather than an entry in a table. Without step 3 the first line below is rejected, and a
/// trait object could never be passed anywhere its own trait is required.
#[test]
fn a_dyn_satisfies_the_trait_it_names_and_no_other() {
    let messages = typeck_src(
        "trait Show { fun show(&self); }
         trait Other { fun other(&self); }
         struct Sorted<T: Show> { inner: T }
         fun f(x: Sorted<dyn Show>) {}
         fun g(x: Sorted<dyn Other>) {}",
    );
    assert_eq!(
        messages,
        ["the trait bound `dyn Other: Show` is not satisfied"],
        "{messages:?}"
    );
}

// -----------------------------------------------------------------
// Matching, not unification
// -----------------------------------------------------------------

/// [`match_ty`](super::solve::match_ty) matching one way instead of unifying.
///
/// `v + v` is asked on the spot, before `v = w` settles what `v` is, so the goal the solver sees
/// is `Wrap<_>: Add` with an inference variable inside it. Both blocks in the index would unify
/// with that goal, each by binding `_` to a different type, so a solver that unified would answer
/// this program according to which block happens to be declared first, accepting it under one
/// order and rejecting the assignment under the other. Matching binds nothing on the goal's side,
/// so neither block applies to a type that is not known yet, the variable comes out of the query
/// exactly as it went in, and the two orders are the same program.
#[test]
fn an_unsettled_goal_is_answered_the_same_way_whatever_the_declaration_order() {
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

// -----------------------------------------------------------------
// Deferral
// -----------------------------------------------------------------

/// [`Solution::Ambiguous`](super::solve::Solution::Ambiguous), and registering an obligation
/// instead of proving it where it is raised.
///
/// At `let mut b = make()` the type `make` was instantiated at is an inference variable, so
/// `_: Show` can be neither proved nor disproved; `b = a` settles it two tokens later. Proved on
/// the spot the goal is ambiguous and the program is rejected as needing an annotation it does
/// not need. Answering "no" to an ambiguous goal instead would reject it outright.
#[test]
fn a_bound_is_proved_after_the_body_that_settles_its_type() {
    typeck_accepts(&src("fun make<T: Show>() -> T { return make(); }
         fun f(a: Foo) { let mut b = make(); b = a; }"));
}

/// The other half of deferral: a goal that is *still* ambiguous when the drain reaches it is
/// reported, rather than passed silently.
///
/// Nothing in `f` says what `sort` was instantiated at, so `_: Show` has no answer at the end of
/// the body either. Discarding ambiguous goals at the drain would let this compile with a type
/// nothing downstream could lower.
#[test]
fn a_bound_that_never_settles_is_reported() {
    typeck_rejects(
        &src("fun sort<T: Show>() -> T { return sort(); }
             fun f() { let x = sort(); }"),
        "type annotations needed",
    );
}

/// [`Solution::Error`], and the resolve step that finds one.
///
/// `Sorted<Nope>` raises a goal about a type that failed to resolve, which name resolution has
/// already reported. Without the `Error` answer the goal reads as an ordinary failure and the
/// program collects a second diagnostic blaming a type it never had.
#[test]
fn a_bound_about_an_unresolvable_type_is_not_reported_twice() {
    let messages = typeck_src(&src("fun f(x: Sorted<Nope>) {}"));
    assert!(messages.is_empty(), "{messages:?}");
}

/// `Obligation::declared_at`.
///
/// The error is at the instantiation, and the reason it is an error is written on the
/// declaration. Without the second span the diagnostic points only at `Sorted<Bare>` and never
/// says which bound it failed, which for a struct declared in another file is the whole of the
/// explanation.
#[test]
fn an_unmet_bound_points_at_the_bound_that_requires_it() {
    use crate::diagnostics::DiagCtx;

    let hir = crate::testing::resolve_src(&src("fun f(x: Sorted<Bare>) {}"));
    DiagCtx::clear();
    crate::typeck::check(&hir);

    let diagnostics = DiagCtx::diagnostics();
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

    // `Sorted`'s declaration is above the use in `f` that failed it.
    let primary = unmet.span.expect("an unmet bound names a place");
    assert!(declared.span.get_begin() < primary.get_begin());
}

// -----------------------------------------------------------------
// A block's own bounds
// -----------------------------------------------------------------

/// [`Candidate::extend_block_origin`](super::method::Candidate), and the registration it feeds
/// in [`check_chosen_method`](crate::typeck::Typeck::check_chosen_method).
///
/// Method resolution picks a candidate by matching the block's header, and a header says only
/// which types the block *matches*. `extend<T: Show> Wrap<T> with Show` matches `Wrap<Bare>`
/// perfectly well, so without the block's own bound raised at the call site, `Bare` gets a `show`
/// that no `extend Bare with Show` block ever wrote, and the identical requirement is enforced
/// two lines away, where the same type is passed to something that declares `T: Show`.
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

// -----------------------------------------------------------------
// Selecting among matches
// -----------------------------------------------------------------

/// [`Typeck::implements`](crate::typeck::Typeck::implements) taking the first match rather than
/// asserting there is only one.
///
/// Two identical blocks are a coherence error, and coherence reports it at the declarations. Both
/// are still in the index when `Sorted<Foo>` asks whether `Foo: Show`, and both match. Asserting
/// uniqueness there panics the compiler on this program; reporting the second match adds a
/// diagnostic about a mistake already reported, once per use site.
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
