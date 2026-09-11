use crate::mir::{Rvalue, StatementKind};
use crate::testing::{OPS_PREAMBLE, lower_mir_src_files, lower_to_mir};

fn monomorphized(
    src: &str,
) -> (
    crate::typeck::tyctx::TyCtx,
    std::collections::HashMap<crate::mir::Instance, crate::mir::Body>,
) {
    let (_hir, tcx, _types, _mir, instances) = lower_to_mir(src);
    (tcx, instances)
}

fn monomorphized_with_ops(
    src: &str,
) -> std::collections::HashMap<crate::mir::Instance, crate::mir::Body> {
    let (hir, _tcx, _types, _mir, instances) = lower_mir_src_files(&[OPS_PREAMBLE, src]);
    let top_level: std::collections::HashSet<_> = hir.root().items.iter().copied().collect();
    instances
        .into_iter()
        .filter(|(instance, _)| top_level.contains(&instance.def))
        .collect()
}

#[test]
fn a_non_generic_body_monomorphizes_to_exactly_itself() {
    let instances = monomorphized_with_ops("fun add(x: i32, y: i32) -> i32 { return x + y; }");
    assert_eq!(instances.len(), 1);
    let (instance, _) = instances.iter().next().unwrap();
    assert!(instance.args.is_empty());
    assert!(instance.any_mode.is_none());
}

/// `main` is collected as a root on its own terms, not because its locals happen to be
/// concrete. It carries no type parameters, so it is never specialized -- there is exactly one
/// instance of it, with an empty argument list.
///
/// The receiver here is what makes this worth pinning: a `&self` method from a generic `extend`
/// block used to leave `&Wrap<T>` in `main`'s locals, which held `main` back from the roots and
/// dropped the entry point from the program.
#[test]
fn main_is_always_collected_as_a_root() {
    let (_hir, _tcx, _types, mir, instances) = lower_to_mir(
        "struct Wrap<T> { value: T }\n\
         extend<T> Wrap<T> { fun ping(&self) -> i32 { return 3; } }\n\
         fun main() { let w: Wrap<i32> = Wrap { value: 1 }; let n = w.ping(); }",
    );
    let main_def = mir.main.expect("the fixture declares a crate-root `main`");
    let mains: Vec<_> = instances
        .keys()
        .filter(|instance| instance.def == main_def)
        .collect();
    assert_eq!(
        mains.len(),
        1,
        "expected exactly one `main` instance: {mains:?}"
    );
    assert!(
        mains[0].args.is_empty(),
        "`main` takes no type parameters, so it is never specialized: {:?}",
        mains[0]
    );
}

#[test]
fn a_generic_function_is_instantiated_once_per_call_site_type() {
    let (_tcx, instances) = monomorphized(
        "fun identity<T>(x: T) -> T { return x; }
         fun f() -> i32 {
             let a = identity(1);
             let b = identity(true);
             return a;
         }",
    );
    // `f` itself, plus `identity::<i32>` and `identity::<bool>`.
    assert_eq!(instances.len(), 3);

    let identity_instances: Vec<_> = instances.keys().filter(|i| !i.args.is_empty()).collect();
    assert_eq!(identity_instances.len(), 2);
    for instance in identity_instances {
        assert_eq!(instance.args.len(), 1);
    }
}

#[test]
fn a_generic_bodys_locals_are_fully_concrete_after_monomorphizing() {
    let (tcx, instances) = monomorphized(
        "fun identity<T>(x: T) -> T { return x; }
         fun f() -> i32 { return identity(1); }",
    );
    for (instance, body) in &instances {
        for decl in &body.local_decls {
            assert!(
                !super::subst::mentions_generic(&tcx, decl.ty),
                "instance {instance:?} still has a generic local"
            );
        }
    }
}

#[test]
fn a_recursive_generic_call_with_the_same_argument_does_not_loop_forever() {
    let (_tcx, instances) = monomorphized(
        "fun f<T>(x: T) -> T {
             return f(x);
         }
         fun g() -> i32 { return f(1); }",
    );
    // `g`, plus exactly one instantiation of `f` (the recursive call inside it is the same
    // instance, deduplicated).
    assert_eq!(instances.len(), 2);
}

#[test]
fn calling_through_a_reified_function_pointer_still_monomorphizes_the_callee() {
    let instances = monomorphized_with_ops(
        "fun double(x: i32) -> i32 { return x + x; }
         fun apply(f: fun(i32) -> i32, x: i32) -> i32 { return f(x); }
         fun g() -> i32 { return apply(double, 1); }",
    );
    // `g`, `apply`, and `double` (reified as a value, still its own Body).
    assert_eq!(instances.len(), 3);
    let reifies = instances.values().any(|body| {
        body.basic_blocks
            .iter()
            .flat_map(|b| &b.statements)
            .any(|s| {
                matches!(
                    &s.kind,
                    StatementKind::Assign(
                        _,
                        Rvalue::Cast {
                            kind: crate::mir::CastKind::ReifyFunPointer,
                            ..
                        }
                    )
                )
            })
    });
    assert!(reifies, "`double` used as a value is reified somewhere");
}

#[test]
fn a_generic_extend_method_is_monomorphized_concretely() {
    let (_hir, tcx, _types, _mir, instances) = lower_to_mir(
        "struct Wrap<T> { public value: T }\n\
         extend<T> Wrap<T> { fun get(self) -> T { return self.value; } }\n\
         fun main() { let w: Wrap<i32> = Wrap { value: 1 }; let n = w.get(); }",
    );
    for (instance, body) in &instances {
        for decl in &body.local_decls {
            assert!(
                !crate::mir::monomorphize::subst::mentions_generic(&tcx, decl.ty),
                "instance {instance:?} kept an unsubstituted local"
            );
        }
    }
}

#[test]
fn a_method_returning_self_is_monomorphized_concretely() {
    let (_hir, tcx, _types, _mir, instances) = lower_to_mir(
        "struct Wrap<T> { public value: T }\n\
         extend<T> Wrap<T> { fun same(self) -> Self { return self; } }\n\
         fun main() { let w: Wrap<i32> = Wrap { value: 1 }; let s = w.same(); }",
    );
    for (instance, body) in &instances {
        for decl in &body.local_decls {
            assert!(
                !crate::mir::monomorphize::subst::mentions_generic(&tcx, decl.ty),
                "instance {instance:?} kept an unsubstituted local"
            );
        }
    }
}

#[test]
fn a_closure_parameter_takes_the_type_its_call_site_gives_it() {
    let (_hir, tcx, _types, _mir, instances) = lower_to_mir(
        "fun conv<A, B>(x: A, f: fun(A) -> B) -> B { return f(x); }\n\
         fun main() { let n: i32 = conv(true, |x| 9); }",
    );
    for (instance, body) in &instances {
        for decl in &body.local_decls {
            assert!(
                !matches!(tcx.kind(decl.ty), crate::typeck::ty::TyKind::Var(_)),
                "instance {instance:?} kept an unresolved local {decl:?}"
            );
        }
    }
}

const TRAIT_DEFAULTS_SRC: &str = "trait Cloner {\n\
     fun clone_it(&self) -> Self;\n\
     fun twice(&self) -> Self { return self.clone_it(); }\n\
 }\n\
 struct N { public value: i32 }\n\
 struct M { public value: i32 }\n\
 extend N with Cloner { fun clone_it(&self) -> Self { return N { value: self.value + 1 }; } }\n\
 extend M with Cloner { fun clone_it(&self) -> Self { return M { value: self.value + 2 }; } }\n\
 fun main() {\n\
     let n: N = N { value: 1 };\n\
     let m: M = M { value: 1 };\n\
     let n2: N = n.twice();\n\
     let m2: M = m.twice();\n\
 }";

#[test]
fn a_trait_default_method_body_is_monomorphized_per_implementing_type() {
    let (_hir, tcx, _types, _mir, instances) = lower_to_mir(TRAIT_DEFAULTS_SRC);
    for (instance, body) in &instances {
        for decl in &body.local_decls {
            assert!(
                !crate::mir::monomorphize::subst::mentions_generic(&tcx, decl.ty),
                "instance {instance:?} kept an unsubstituted local {decl:?}"
            );
        }
    }
}

#[test]
fn a_trait_default_body_calls_the_implementing_types_own_method() {
    let (hir, _tcx, _types, _mir, instances) = lower_to_mir(TRAIT_DEFAULTS_SRC);
    for (instance, body) in &instances {
        let _ = instance;
        for block in &body.basic_blocks {
            if let crate::mir::TerminatorKind::Call { func, .. } = &block.terminator.kind
                && let crate::mir::Operand::Constant(constant) = func
                && let crate::mir::ConstKind::FunDef(callee, ..) = &constant.kind
            {
                assert!(
                    hir.function(*callee).block.is_some(),
                    "{callee:?} is abstract; a default body must dispatch to an impl's method"
                );
            }
        }
    }
}

#[test]
fn a_generic_trait_default_body_substitutes_the_trait_arguments() {
    let (_hir, tcx, _types, _mir, instances) = lower_to_mir(
        "trait Sh<T> {\n\
         fun sh(&self) -> T;\n\
         fun go(&self) -> T { return self.sh(); }\n\
         }\n\
         struct W { public value: i32 }\n\
         extend W with Sh<i32> { fun sh(&self) -> i32 { return self.value; } }\n\
         fun main() { let w: W = W { value: 5 }; let n: i32 = w.go(); }",
    );
    for (instance, body) in &instances {
        for decl in &body.local_decls {
            assert!(
                !crate::mir::monomorphize::subst::mentions_generic(&tcx, decl.ty),
                "instance {instance:?} kept an unsubstituted local {decl:?}"
            );
        }
    }
}
