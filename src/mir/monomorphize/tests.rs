//! Unit tests for `mir::monomorphize`.

use crate::mir::{Rvalue, StatementKind};
use crate::testing::lower_mir_src;

fn monomorphized(
    src: &str,
) -> (
    crate::typeck::tyctx::TyCtx,
    std::collections::HashMap<crate::mir::Instance, crate::mir::Body>,
) {
    let (_hir, tcx, _types, instances) = lower_mir_src(src);
    (tcx, instances)
}

#[test]
fn a_non_generic_body_monomorphizes_to_exactly_itself() {
    let (_tcx, instances) = monomorphized("fun add(x: i32, y: i32) -> i32 { return x + y; }");
    assert_eq!(instances.len(), 1);
    let (instance, _) = instances.iter().next().unwrap();
    assert!(instance.args.is_empty());
    assert!(instance.any_mode.is_none());
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
    let (_tcx, instances) = monomorphized(
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
                            kind: crate::mir::CastKind::ReifyFnPointer,
                            ..
                        }
                    )
                )
            })
    });
    assert!(reifies, "`double` used as a value is reified somewhere");
}
