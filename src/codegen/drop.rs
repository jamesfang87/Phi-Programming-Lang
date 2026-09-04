use inkwell::values::PointerValue;

use super::ctx::CodegenCtx;
use super::layout::is_unsized;
use crate::typeck::ty::{Ty, TyKind};
use crate::typeck::tyctx::TyCtx;

pub fn drop_glue<'ctx>(
    cx: &mut CodegenCtx<'ctx>,
    tcx: &mut TyCtx,
    ptr: PointerValue<'ctx>,
    ty: Ty,
) {
    let ptr_ty = cx.llvm.ptr_type(Default::default());
    match tcx.kind(ty).clone() {
        TyKind::Iso(base) if is_unsized(tcx, base) => {
            let two_word_ty = cx
                .llvm
                .struct_type(&[ptr_ty.into(), cx.llvm.i64_type().into()], false);
            let data_ptr_field = cx
                .builder
                .build_struct_gep(two_word_ty, ptr, 0, "iso.data_ptr")
                .unwrap();
            let data_ptr = cx
                .builder
                .build_load(ptr_ty, data_ptr_field, "iso.data")
                .unwrap();
            cx.builder
                .build_call(cx.libc.free, &[data_ptr.into()], "")
                .unwrap();
        }
        TyKind::Iso(_) => {
            let heap_ptr = cx.builder.build_load(ptr_ty, ptr, "iso.ptr").unwrap();
            cx.builder
                .build_call(cx.libc.free, &[heap_ptr.into()], "")
                .unwrap();
        }
        _ => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scratch_function<'ctx>(cx: &CodegenCtx<'ctx>) -> inkwell::values::FunctionValue<'ctx> {
        let function =
            cx.module
                .add_function("scratch", cx.llvm.void_type().fn_type(&[], false), None);
        let entry = cx.llvm.append_basic_block(function, "entry");
        cx.builder.position_at_end(entry);
        function
    }

    #[test]
    fn drop_glue_on_iso_sized_calls_free() {
        let llvm = inkwell::context::Context::create();
        let (_hir, mut tcx, _types, _mir, _instances) = crate::testing::lower_to_mir("fun f() {}");
        let mut cx = super::super::ctx::CodegenCtx::new(&llvm, "t");
        scratch_function(&cx);

        let i32_ty = tcx.mk_prim(crate::nameres::PrimTy::I32);
        let iso_ty = tcx.mk_iso(i32_ty);
        let alloca = cx
            .builder
            .build_alloca(cx.llvm.ptr_type(Default::default()), "x")
            .unwrap();

        drop_glue(&mut cx, &mut tcx, alloca, iso_ty);
        cx.builder.build_return(None).unwrap();

        let ir = cx.module.print_to_string().to_string();
        assert!(ir.contains("call void @free"), "{ir}");
    }

    #[test]
    fn drop_glue_on_iso_unsized_calls_free_on_the_data_pointer_only() {
        let llvm = inkwell::context::Context::create();
        let (_hir, mut tcx, _types, _mir, _instances) = crate::testing::lower_to_mir("fun f() {}");
        let mut cx = super::super::ctx::CodegenCtx::new(&llvm, "t");
        scratch_function(&cx);

        let u8_ty = tcx.mk_prim(crate::nameres::PrimTy::U8);
        let slice_ty = tcx.mk_array(u8_ty, None);
        let iso_slice_ty = tcx.mk_iso(slice_ty);
        let two_word_ty = cx.llvm.struct_type(
            &[
                cx.llvm.ptr_type(Default::default()).into(),
                cx.llvm.i64_type().into(),
            ],
            false,
        );
        let alloca = cx.builder.build_alloca(two_word_ty, "x").unwrap();

        drop_glue(&mut cx, &mut tcx, alloca, iso_slice_ty);
        cx.builder.build_return(None).unwrap();

        let ir = cx.module.print_to_string().to_string();
        assert!(ir.contains("call void @free"), "{ir}");
        assert!(
            ir.contains("getelementptr"),
            "expected a GEP to the data-pointer field:\n{ir}"
        );
    }

    #[test]
    fn drop_glue_on_non_iso_is_a_no_op() {
        let llvm = inkwell::context::Context::create();
        let (_hir, mut tcx, _types, _mir, _instances) = crate::testing::lower_to_mir("fun f() {}");
        let mut cx = super::super::ctx::CodegenCtx::new(&llvm, "t");
        scratch_function(&cx);

        let i32_ty = tcx.mk_prim(crate::nameres::PrimTy::I32);
        let alloca = cx.builder.build_alloca(cx.llvm.i32_type(), "x").unwrap();

        drop_glue(&mut cx, &mut tcx, alloca, i32_ty);
        cx.builder.build_return(None).unwrap();

        let ir = cx.module.print_to_string().to_string();
        assert!(
            !ir.contains("call void @free"),
            "a plain i32 must not be freed:\n{ir}"
        );
    }
}
