use inkwell::context::Context;
use inkwell::module::Module;
use inkwell::values::FunctionValue;

pub struct Libc<'ctx> {
    pub malloc: FunctionValue<'ctx>,
    pub free: FunctionValue<'ctx>,
    pub realloc: FunctionValue<'ctx>,
    pub memcpy: FunctionValue<'ctx>,
    pub memset: FunctionValue<'ctx>,
    pub memcmp: FunctionValue<'ctx>,
    pub read: FunctionValue<'ctx>,
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
    let realloc = module.add_function(
        "realloc",
        ptr.fn_type(&[ptr.into(), i64_ty.into()], false),
        None,
    );
    let memcpy = module.add_function(
        "memcpy",
        ptr.fn_type(&[ptr.into(), ptr.into(), i64_ty.into()], false),
        None,
    );
    let memset = module.add_function(
        "memset",
        ptr.fn_type(&[ptr.into(), i32_ty.into(), i64_ty.into()], false),
        None,
    );
    let memcmp = module.add_function(
        "memcmp",
        i32_ty.fn_type(&[ptr.into(), ptr.into(), i64_ty.into()], false),
        None,
    );
    let read = module.add_function(
        "read",
        i64_ty.fn_type(&[i32_ty.into(), ptr.into(), i64_ty.into()], false),
        None,
    );

    Libc {
        malloc,
        free,
        realloc,
        memcpy,
        memset,
        memcmp,
        read,
        write,
        abort,
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn module_declares_the_libc_functions() {
        let (hir, mut tcx, _types, mir, instances) = crate::testing::lower_to_mir("fun f() {}");
        let llvm = inkwell::context::Context::create();
        let module = crate::codegen::codegen(
            crate::testing::session(),
            &llvm,
            &hir,
            &mut tcx,
            &mir,
            &instances,
            "t",
        )
        .unwrap();
        let ir = module.print_to_string().to_string();
        for sig in [
            "declare ptr @malloc(i64)",
            "declare void @free(ptr)",
            "declare i64 @write(i32, ptr, i64)",
            "declare void @abort()",
            "declare ptr @realloc(ptr, i64)",
            "declare ptr @memcpy(ptr, ptr, i64)",
            "declare ptr @memset(ptr, i32, i64)",
            "declare i32 @memcmp(ptr, ptr, i64)",
            "declare i64 @read(i32, ptr, i64)",
        ] {
            assert!(ir.contains(sig), "missing `{sig}` in:\n{ir}");
        }
    }

    #[test]
    fn a_libc_write_call_becomes_a_direct_write_call() {
        let (hir, mut tcx, _types, mir, instances) = crate::testing::lower_mir_src_as_core(
            r#"module core::libc;
            public fun write(fd: i32, buffer: &[u8], count: usize) -> i64;
            public fun main() { write(1, "hello" as &[u8], 5); }"#,
        );
        let llvm = inkwell::context::Context::create();
        let module = crate::codegen::codegen(
            crate::testing::session(),
            &llvm,
            &hir,
            &mut tcx,
            &mir,
            &instances,
            "t",
        )
        .unwrap();
        module.verify().unwrap();
        let ir = module.print_to_string().to_string();
        assert!(ir.contains("call i64 @write("), "{ir}");
    }

    #[test]
    fn the_memory_and_io_libc_calls_become_direct_calls() {
        let (hir, mut tcx, _types, mir, instances) =
            crate::testing::lower_mir_files_as_core(&[
                "module core::mem;
                 public struct MaybeUninit<T> { }",
                "module core::libc;
                 import core::mem::MaybeUninit;
                 public fun memcpy<T>(dst: &mut [MaybeUninit<T>], src: &[MaybeUninit<T>], count: usize);
                 public fun memset<T>(dst: &mut [MaybeUninit<T>], value: u8, count: usize);
                 public fun memcmp(left: &[u8], right: &[u8], count: usize) -> i32;
                 public fun read(fd: i32, buffer: &mut [u8], count: usize) -> i64;",
                "module core::app;
                 import core::mem::MaybeUninit;
                 import core::libc::memcpy;
                 import core::libc::memset;
                 import core::libc::memcmp;
                 import core::libc::read;
                 public fun main(
                     dst: &mut [MaybeUninit<u8>],
                     src: &[MaybeUninit<u8>],
                     left: &[u8],
                     right: &[u8],
                     buffer: &mut [u8],
                 ) {
                     memcpy(dst, src, 4);
                     memset(dst, 0, 4);
                     let _order = memcmp(left, right, 4);
                     let _read = read(0, buffer, 4);
                 }",
            ]);
        let llvm = inkwell::context::Context::create();
        let module = crate::codegen::codegen(
            crate::testing::session(),
            &llvm,
            &hir,
            &mut tcx,
            &mir,
            &instances,
            "t",
        )
        .unwrap();
        module.verify().unwrap();
        let ir = module.print_to_string().to_string();
        for sig in [
            "call ptr @memcpy(",
            "call ptr @memset(",
            "call i32 @memcmp(",
            "call i64 @read(",
        ] {
            assert!(ir.contains(sig), "missing `{sig}` in:\n{ir}");
        }
    }

    const REAL_CORE_FILES: &[&str] = &[
        include_str!("../../lib/core/mem.phi"),
        include_str!("../../lib/core/llvm.phi"),
        include_str!("../../lib/core/libc.phi"),
    ];

    fn lower_with_real_core(
        app: &str,
    ) -> (
        crate::hir::Hir,
        crate::typeck::ty::ctx::TyCtx,
        crate::mir::Mir,
        std::collections::HashMap<crate::mir::Instance, crate::mir::Body>,
    ) {
        let (hir, tcx, _types, mir, instances) =
            crate::testing::lower_mir_files_as_core(&[REAL_CORE_FILES, &[app]].concat());
        (hir, tcx, mir, instances)
    }

    #[test]
    fn realloc_and_free_become_direct_calls() {
        let (hir, mut tcx, mir, instances) = lower_with_real_core(
            r#"module core::app;
            import core::libc::free;
            import core::libc::malloc;
            import core::libc::realloc;
            import core::mem::MaybeUninit;
            public fun main() {
                let buffer: iso [MaybeUninit<u8>] = malloc(4);
                let grown: iso [MaybeUninit<u8>] = realloc(buffer, 8);
                free(grown);
            }"#,
        );
        let llvm = inkwell::context::Context::create();
        let module = crate::codegen::codegen(
            crate::testing::session(),
            &llvm,
            &hir,
            &mut tcx,
            &mir,
            &instances,
            "t",
        )
        .unwrap();
        module.verify().unwrap();
        let ir = module.print_to_string().to_string();
        assert!(ir.contains("call ptr @realloc("), "{ir}");
        assert!(ir.contains("call void @free("), "{ir}");
    }

    #[test]
    fn puts_writes_the_text_and_a_newline() {
        let (hir, mut tcx, mir, instances) = lower_with_real_core(
            r#"module core::app;
            import core::libc::puts;
            public fun main() {
                puts("hi");
            }"#,
        );
        let llvm = inkwell::context::Context::create();
        let module = crate::codegen::codegen(
            crate::testing::session(),
            &llvm,
            &hir,
            &mut tcx,
            &mir,
            &instances,
            "t",
        )
        .unwrap();
        module.verify().unwrap();
        let ir = module.print_to_string().to_string();
        assert_eq!(ir.matches("call i64 @write(").count(), 2, "{ir}");
    }
}
