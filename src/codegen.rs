mod body;
mod closure;
mod ctx;
mod drop;
pub mod emit;
mod intrinsic;
mod konst;
mod layout;
pub(crate) mod mangle;
mod place;
mod ty;
mod vtable;

use std::collections::HashMap;

use inkwell::context::Context;
use inkwell::module::Module;

use crate::hir::Hir;
use crate::mir::{Body, Instance, Mir};
use crate::session::Session;
use crate::typeck::ty::ctx::TyCtx;
use ctx::CodegenCtx;

#[derive(Debug)]
pub enum CodegenError {
    Verification(String),
    Link(String),
}

pub fn codegen<'ctx>(
    session: &'ctx Session,
    llvm: &'ctx Context,
    hir: &'ctx Hir,
    tcx: &mut TyCtx,
    mir: &Mir,
    instances: &HashMap<Instance, Body>,
    module_name: &str,
) -> Result<Module<'ctx>, CodegenError> {
    let mut cx = CodegenCtx::new(session, hir, llvm, module_name);

    for (instance, body) in instances {
        let name = cx.mangle(tcx, instance);
        let fn_type = ty::function_type(&cx, tcx, mir, body);
        let function = cx.module.add_function(&name, fn_type, None);
        cx.functions.insert(name, function);
    }

    for (instance, body) in instances {
        let name = cx.mangle(tcx, instance);
        let function = cx.functions[&name];
        body::lower_body(&mut cx, mir, tcx, body, function);
    }

    emit_c_main_trampoline(&mut cx, tcx, instances);

    Ok(cx.module)
}

// TODO: why is this called trampoline?? what is that?
fn emit_c_main_trampoline(cx: &mut CodegenCtx, tcx: &TyCtx, instances: &HashMap<Instance, Body>) {
    let main = match crate::checks::entry_point::crate_root_main_candidates(cx.session, cx.hir)
        .as_slice()
    {
        [one] => Some(*one),
        _ => None,
    };
    let phi_main = main.map(|main_def| {
        let instance = instances
            .keys()
            .find(|instance| instance.def == main_def)
            .unwrap_or_else(|| {
                panic!(
                    "codegen: `main` is declared but `mir::monomorphize` collected no instance \
                     for it, though the entry point is seeded as a root unconditionally"
                )
            });
        cx.functions[&cx.mangle(tcx, instance)]
    });

    let llvm = cx.llvm;
    let c_main_type = llvm.i32_type().fn_type(&[], false);
    let c_main = cx.module.add_function("main", c_main_type, None);
    let entry = llvm.append_basic_block(c_main, "entry");
    cx.builder.position_at_end(entry);

    let zero = llvm.i32_type().const_int(0, false);
    let ret_val = match phi_main {
        Some(phi_main) => {
            let call = cx
                .builder
                .build_call(phi_main, &[], "call_phi_main")
                .unwrap();
            call.try_as_basic_value()
                .basic()
                .filter(|v| v.is_int_value() && v.into_int_value().get_type() == llvm.i32_type())
                .map(|v| v.into_int_value())
                .unwrap_or(zero)
        }
        None => zero,
    };
    cx.builder.build_return(Some(&ret_val)).unwrap();
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::hir::OwnerNode;

    #[test]
    fn declares_every_instance_before_lowering_any_body() {
        let (hir, mut tcx, _types, mir, instances) =
            crate::testing::lower_to_mir("fun f() {}\nfun g() { f(); }");
        let llvm = inkwell::context::Context::create();
        let module = codegen(
            crate::testing::session(),
            &llvm,
            &hir,
            &mut tcx,
            &mir,
            &instances,
            "test",
        )
        .expect("codegen succeeds");
        // The extras are the runtime declarations plus the C `main` trampoline, which is now
        // emitted for every crate -- this fixture declares no `main`, so that trampoline is the
        // do-nothing one.
        assert_eq!(module.get_functions().count(), instances.len() + 5);
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
        let (hir, _tcx, _types, _mir, _instances) = crate::testing::lower_mir_src_files(&[
            "module app;\n\nfun main() {\n}\n",
            "module app::inner;\n\nfun main() {\n}\n",
        ]);
        let found = crate::checks::entry_point::check(crate::testing::session(), &hir)
            .expect("a root-level `main` exists");
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
        // `main` returns nothing (the entry-point check enforces that), so the two candidates
        // are distinguished by which panic message ends up on stderr instead of by exit code.
        let (hir, mut tcx, _types, mir, instances) = crate::testing::lower_mir_src_files(&[
            "module app;\n\nfun main() { panic(\"the crate-root main ran\"); }\n",
            "module app::inner;\n\nfun main() { panic(\"the nested main ran\"); }\n",
        ]);
        let llvm = inkwell::context::Context::create();
        let module = codegen(
            crate::testing::session(),
            &llvm,
            &hir,
            &mut tcx,
            &mir,
            &instances,
            "t",
        )
        .expect("codegen succeeds");

        let dir = tempdir_for_test("root-main");
        let exe = emit::emit(
            &module,
            &emit::EmitOptions {
                output_path: dir.join("t"),
                release: false,
            },
        )
        .expect("emit succeeds");

        let output = std::process::Command::new(&exe)
            .output()
            .expect("linked binary runs");
        let stderr = String::from_utf8_lossy(&output.stderr);
        assert!(
            stderr.contains("the crate-root main ran"),
            "expected the crate-root `main` to be called; stderr: {stderr:?}"
        );
        assert!(
            !stderr.contains("the nested main ran"),
            "the nested module's `main` should not have been called; stderr: {stderr:?}"
        );

        let _ = std::fs::remove_dir_all(&dir);
    }
}
