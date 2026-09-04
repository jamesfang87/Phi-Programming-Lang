use std::path::{Path, PathBuf};

use inkwell::OptimizationLevel;
use inkwell::module::Module;
use inkwell::targets::{
    CodeModel, FileType, InitializationConfig, RelocMode, Target, TargetMachine,
};

use super::CodegenError;

// TODO: There are already options in the driver code. Perhaps it could reuse that?
// if not, then this is fine I guess
pub struct EmitOptions {
    pub output_path: PathBuf,
    pub release: bool,
}

pub fn emit(module: &Module, options: &EmitOptions) -> Result<PathBuf, CodegenError> {
    module.verify().map_err(|e| {
        CodegenError::Verification(format!("{e}\n{}", module.print_to_string().to_string()))
    })?;

    Target::initialize_native(&InitializationConfig::default())
        .map_err(CodegenError::Verification)?;

    let triple = TargetMachine::get_default_triple();
    let target =
        Target::from_triple(&triple).map_err(|e| CodegenError::Verification(e.to_string()))?;
    let opt_level = if options.release {
        OptimizationLevel::Aggressive
    } else {
        OptimizationLevel::None
    };
    let target_machine = target
        .create_target_machine(
            &triple,
            "generic",
            "",
            opt_level,
            RelocMode::Default,
            CodeModel::Default,
        )
        .ok_or_else(|| CodegenError::Verification("no target machine for host triple".into()))?;

    let object_path = options.output_path.with_extension("o");
    target_machine
        .write_to_file(module, FileType::Object, &object_path)
        .map_err(|e| CodegenError::Verification(e.to_string()))?;

    link(&object_path, &options.output_path)
}

fn link(object_path: &Path, output_path: &Path) -> Result<PathBuf, CodegenError> {
    for linker in ["cc", "clang"] {
        let status = std::process::Command::new(linker)
            .arg(object_path)
            .arg("-o")
            .arg(output_path)
            .status();
        match status {
            Ok(s) if s.success() => return Ok(output_path.to_path_buf()),
            Ok(s) => return Err(CodegenError::Link(format!("{linker} exited with {s}"))),
            Err(_) => continue,
        }
    }
    Err(CodegenError::Link(format!(
        "object file written to {}; no `cc` or `clang` found on PATH to link it",
        object_path.display()
    )))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::hir::Hir;
    use crate::typeck::tyctx::TyCtx;

    fn tempdir_for_test(name: &str) -> PathBuf {
        use std::sync::atomic::{AtomicU64, Ordering};
        static COUNTER: AtomicU64 = AtomicU64::new(0);
        let n = COUNTER.fetch_add(1, Ordering::Relaxed);
        let dir = std::env::temp_dir().join(format!(
            "phi-emit-test-{}-{name}-{}-{n}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn append_c_main_trampoline<'ctx>(
        llvm: &'ctx inkwell::context::Context,
        module: &Module<'ctx>,
        hir: &Hir,
        tcx: &TyCtx,
        mir: &crate::mir::Mir,
        instances: &std::collections::HashMap<crate::mir::Instance, crate::mir::Body>,
    ) {
        let phi_main_name = instances
            .keys()
            .find(|instance| {
                matches!(
                    hir.def(instance.def),
                    crate::hir::OwnerNode::Function(f)
                        if crate::ast::interner::Interner::resolve(f.name.text) == "main"
                )
            })
            .map(|instance| crate::mir::mangle::mangle(mir, tcx, instance))
            .expect("fixture defines a `main` function");
        let phi_main = module
            .get_function(&phi_main_name)
            .expect("main instance was declared by codegen()");

        let c_main_type = llvm.i32_type().fn_type(&[], false);
        let c_main = module.add_function("main", c_main_type, None);
        let entry = llvm.append_basic_block(c_main, "entry");
        let builder = llvm.create_builder();
        builder.position_at_end(entry);
        builder.build_call(phi_main, &[], "call_phi_main").unwrap();
        builder
            .build_return(Some(&llvm.i32_type().const_int(0, false)))
            .unwrap();
    }

    #[test]
    fn emits_and_links_a_runnable_hello_world() {
        let (hir, mut tcx, _types, mir, instances) = crate::testing::lower_mir_src_as_core(
            r#"module core::io;
            public fun write_bytes(fd: i32, buf: &[u8]) -> i64;
            public fun main() { write_bytes(1, "hello" as &[u8]); }"#,
        );
        let llvm = inkwell::context::Context::create();
        let module = super::super::codegen(&llvm, &mut tcx, &mir, &instances, "t")
            .expect("codegen succeeds");
        append_c_main_trampoline(&llvm, &module, &hir, &tcx, &mir, &instances);

        let dir = tempdir_for_test("hello-world");
        let exe = emit(
            &module,
            &EmitOptions {
                output_path: dir.join("hello"),
                release: false,
            },
        )
        .expect("emit succeeds");

        let output = std::process::Command::new(&exe)
            .output()
            .expect("linked binary runs");
        assert!(
            output.status.success(),
            "process exited: {:?}",
            output.status
        );
        assert_eq!(output.stdout, b"hello");

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn verification_failure_reports_module_ir_alongside_the_error() {
        let llvm = inkwell::context::Context::create();
        let module = llvm.create_module("bad");
        let fn_type = llvm.i32_type().fn_type(&[], false);
        let function = module.add_function("broken", fn_type, None);
        let block = llvm.append_basic_block(function, "entry");
        let builder = llvm.create_builder();
        builder.position_at_end(block);

        let dir = tempdir_for_test("bad-module");
        let err = emit(
            &module,
            &EmitOptions {
                output_path: dir.join("broken"),
                release: false,
            },
        )
        .unwrap_err();

        match err {
            CodegenError::Verification(msg) => {
                assert!(msg.contains("define"), "expected IR dump in: {msg}");
            }
            CodegenError::Link(msg) => panic!("expected Verification, got Link({msg})"),
        }

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn link_reports_missing_linker_actionably() {
        let dir = tempdir_for_test("missing-linker");
        let object_path = dir.join("nothing.o");
        std::fs::write(&object_path, b"").unwrap();
        let output_path = dir.join("nothing");
        let empty_path_dir = dir.join("empty-path");
        std::fs::create_dir_all(&empty_path_dir).unwrap();

        let saved_path = std::env::var_os("PATH");
        unsafe {
            std::env::set_var("PATH", &empty_path_dir);
        }
        let result = super::link(&object_path, &output_path);
        unsafe {
            match &saved_path {
                Some(p) => std::env::set_var("PATH", p),
                None => std::env::remove_var("PATH"),
            }
        }

        match result {
            Err(CodegenError::Link(msg)) => {
                assert!(msg.contains(&object_path.display().to_string()), "{msg}");
                assert!(msg.contains("no `cc` or `clang` found"), "{msg}");
            }
            other => panic!("expected a Link error naming the object file, got {other:?}"),
        }

        let _ = std::fs::remove_dir_all(&dir);
    }
}
