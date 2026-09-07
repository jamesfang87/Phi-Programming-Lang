use inkwell::context::Context;
use inkwell::module::Module;
use inkwell::values::FunctionValue;

pub struct Libc<'ctx> {
    pub malloc: FunctionValue<'ctx>,
    pub free: FunctionValue<'ctx>,
    pub write: FunctionValue<'ctx>,
    pub abort: FunctionValue<'ctx>,
}

pub fn declare_libc<'ctx>(llvm: &'ctx Context, module: &Module<'ctx>) -> Libc<'ctx> {
    let ptr = llvm.ptr_type(Default::default());
    let i64_ty = llvm.i64_type();
    let i32_ty = llvm.i32_type();

    let malloc = module.add_function("malloc", ptr.fn_type(&[i64_ty.into()], false), None);
    let free = module.add_function("free", llvm.void_type().fn_type(&[ptr.into()], false), None);
    let write = module.add_function(
        "write",
        i64_ty.fn_type(&[i32_ty.into(), ptr.into(), i64_ty.into()], false),
        None,
    );
    let abort = module.add_function("abort", llvm.void_type().fn_type(&[], false), None);

    Libc {
        malloc,
        free,
        write,
        abort,
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn module_declares_the_four_libc_functions() {
        let (_hir, mut tcx, _types, mir, instances) = crate::testing::lower_to_mir("fun f() {}");
        let llvm = inkwell::context::Context::create();
        let module = crate::codegen::codegen(&llvm, &mut tcx, &mir, &instances, "t").unwrap();
        let ir = module.print_to_string().to_string();
        for sig in [
            "declare ptr @malloc(i64)",
            "declare void @free(ptr)",
            "declare i64 @write(i32, ptr, i64)",
            "declare void @abort()",
        ] {
            assert!(ir.contains(sig), "missing `{sig}` in:\n{ir}");
        }
    }

    #[test]
    fn write_bytes_call_becomes_direct_libc_write_call() {
        let (_hir, mut tcx, _types, mir, instances) = crate::testing::lower_mir_src_as_core(
            r#"module core::io;
            public fun write_bytes(fd: i32, buf: &[u8]) -> i64;
            public fun main() { write_bytes(1, "hello" as &[u8]); }"#,
        );
        let llvm = inkwell::context::Context::create();
        let module = crate::codegen::codegen(&llvm, &mut tcx, &mir, &instances, "t").unwrap();
        module.verify().unwrap();
        let ir = module.print_to_string().to_string();
        assert!(ir.contains("call i64 @write("), "{ir}");
    }
}
