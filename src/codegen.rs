//! LLVM codegen backend: lowers monomorphized MIR instances into an inkwell `Module`.
//!
//! `codegen()` is the entry point. It runs in two passes over `instances`: first it
//! declares every instance's function signature (via `ty::function_type`) so that a
//! call to any instance resolves regardless of iteration or definition order, then
//! it lowers each instance's body (via `body::lower_body`) into its already-declared
//! `FunctionValue`. Verification and object/IR emission happen later, in `emit.rs`
//! (Task 11).

mod body;
mod ctx;
mod drop;
pub mod emit;
mod intrinsic;
mod konst;
mod layout;
mod place;
mod ty;
mod vtable;

use std::collections::HashMap;

use inkwell::context::Context;
use inkwell::module::Module;

use crate::mir::mangle::mangle;
use crate::mir::{Body, Instance, Mir};
use crate::typeck::tyctx::TyCtx;
use ctx::CodegenCtx;

#[derive(Debug)]
pub enum CodegenError {
    Verification(String),
    Link(String),
}

pub fn codegen<'ctx>(
    llvm: &'ctx Context,
    tcx: &mut TyCtx,
    mir: &Mir,
    instances: &HashMap<Instance, Body>,
    module_name: &str,
) -> Result<Module<'ctx>, CodegenError> {
    let mut cx = CodegenCtx::new(llvm, module_name);

    for (instance, body) in instances {
        let name = mangle(mir, tcx, instance);
        let fn_type = ty::function_type(&cx, tcx, mir, body);
        let function = cx.module.add_function(&name, fn_type, None);
        cx.functions.insert(name, function);
    }

    for (instance, body) in instances {
        let name = mangle(mir, tcx, instance);
        let function = cx.functions[&name];
        body::lower_body(&mut cx, mir, tcx, body, function);
    }

    emit_c_main_trampoline(&mut cx, tcx, mir, instances);

    Ok(cx.module)
}

fn emit_c_main_trampoline(
    cx: &mut CodegenCtx,
    tcx: &TyCtx,
    mir: &Mir,
    instances: &HashMap<Instance, Body>,
) {
    let Some(main_def) = mir.main else {
        return;
    };
    let Some(phi_main_name) = instances
        .keys()
        .find(|instance| instance.def == main_def)
        .map(|instance| mangle(mir, tcx, instance))
    else {
        return;
    };
    let phi_main = cx.functions[&phi_main_name];

    let llvm = cx.llvm;
    let c_main_type = llvm.i32_type().fn_type(&[], false);
    let c_main = cx.module.add_function("main", c_main_type, None);
    let entry = llvm.append_basic_block(c_main, "entry");
    cx.builder.position_at_end(entry);
    let call = cx
        .builder
        .build_call(phi_main, &[], "call_phi_main")
        .unwrap();
    let ret_val = call
        .try_as_basic_value()
        .basic()
        .filter(|v| v.is_int_value() && v.into_int_value().get_type() == llvm.i32_type())
        .map(|v| v.into_int_value())
        .unwrap_or_else(|| llvm.i32_type().const_int(0, false));
    cx.builder.build_return(Some(&ret_val)).unwrap();
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::hir::OwnerNode;

    #[test]
    fn declares_every_instance_before_lowering_any_body() {
        let (_hir, mut tcx, _types, mir, instances) =
            crate::testing::lower_to_mir("fun f() {}\nfun g() { f(); }");
        let llvm = inkwell::context::Context::create();
        let module = codegen(&llvm, &mut tcx, &mir, &instances, "test").expect("codegen succeeds");
        assert_eq!(module.get_functions().count(), instances.len() + 4);
    }

    fn tempdir_for_test(name: &str) -> std::path::PathBuf {
        use std::sync::atomic::{AtomicU64, Ordering};
        static COUNTER: AtomicU64 = AtomicU64::new(0);
        let n = COUNTER.fetch_add(1, Ordering::Relaxed);
        let dir = std::env::temp_dir().join(format!(
            "phi-codegen-test-{}-{name}-{}-{n}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn find_crate_root_main_picks_the_root_not_a_nested_module() {
        let (hir, _tcx, _types, mir, _instances) = crate::testing::lower_mir_src_files(&[
            "module app;\n\nfun main() {\n}\n",
            "module app::inner;\n\nfun main() {\n}\n",
        ]);
        let found = mir.main.expect("a root-level `main` exists");
        let is_in_app_module = hir.root().items.iter().any(
            |&child| matches!(hir.def(child), OwnerNode::Module(m) if m.items.contains(&found)),
        );
        assert!(
            is_in_app_module,
            "expected the crate-root `app` module's `main`, not the nested `app::inner` one"
        );
    }

    #[test]
    fn c_main_trampoline_calls_the_crate_root_main_not_a_nested_one() {
        let (_hir, mut tcx, _types, mir, instances) = crate::testing::lower_mir_src_files(&[
            "module app;\n\nfun main() -> i32 {\n    return 1;\n}\n",
            "module app::inner;\n\nfun main() -> i32 {\n    return 2;\n}\n",
        ]);
        let llvm = inkwell::context::Context::create();
        let module = codegen(&llvm, &mut tcx, &mir, &instances, "t").expect("codegen succeeds");

        let dir = tempdir_for_test("root-main");
        let exe = emit::emit(
            &module,
            &emit::EmitOptions {
                output_path: dir.join("t"),
                release: false,
            },
        )
        .expect("emit succeeds");

        let status = std::process::Command::new(&exe)
            .status()
            .expect("linked binary runs");
        assert_eq!(
            status.code(),
            Some(1),
            "expected the crate-root `main`'s exit code (1), not the nested one's (2)"
        );

        let _ = std::fs::remove_dir_all(&dir);
    }
}
