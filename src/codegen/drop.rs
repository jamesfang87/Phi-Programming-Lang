use inkwell::module::Linkage;
use inkwell::values::{FunctionValue, IntValue, PointerValue};

use super::ctx::CodegenCtx;
use super::layout::is_unsized;
use super::ty as llvm_ty;
use crate::mir::Mir;
use crate::typeck::ty::{Ty, TyKind};
use crate::typeck::tyctx::TyCtx;

pub fn drop_glue<'ctx>(
    cx: &mut CodegenCtx<'ctx>,
    tcx: &mut TyCtx,
    mir: &Mir,
    ptr: PointerValue<'ctx>,
    ty: Ty,
) {
    if !tcx.needs_drop(ty) {
        return;
    }
    let glue = glue_function(cx, tcx, mir, ty);
    cx.builder.build_call(glue, &[ptr.into()], "").unwrap();
}

pub fn glue_pointer<'ctx>(
    cx: &mut CodegenCtx<'ctx>,
    tcx: &mut TyCtx,
    mir: &Mir,
    ty: Ty,
) -> Option<PointerValue<'ctx>> {
    tcx.needs_drop(ty).then(|| {
        glue_function(cx, tcx, mir, ty)
            .as_global_value()
            .as_pointer_value()
    })
}

fn glue_function<'ctx>(
    cx: &mut CodegenCtx<'ctx>,
    tcx: &mut TyCtx,
    mir: &Mir,
    ty: Ty,
) -> FunctionValue<'ctx> {
    if let Some(&glue) = cx.drop_glues.borrow().get(&ty) {
        return glue;
    }

    let fn_ty = cx
        .llvm
        .void_type()
        .fn_type(&[cx.llvm.ptr_type(Default::default()).into()], false);
    let glue = cx.module.add_function(
        &format!("drop.glue.{}", ty.index()),
        fn_ty,
        Some(Linkage::Internal),
    );
    cx.drop_glues.borrow_mut().insert(ty, glue);

    let resume_at = cx.builder.get_insert_block();
    let entry = cx.llvm.append_basic_block(glue, "entry");
    cx.builder.position_at_end(entry);
    let value = glue
        .get_nth_param(0)
        .expect("drop glue takes the value's address")
        .into_pointer_value();
    emit_body(cx, tcx, mir, value, ty);
    cx.builder.build_return(None).unwrap();
    if let Some(block) = resume_at {
        cx.builder.position_at_end(block);
    }

    glue
}

fn emit_body<'ctx>(
    cx: &mut CodegenCtx<'ctx>,
    tcx: &mut TyCtx,
    mir: &Mir,
    ptr: PointerValue<'ctx>,
    ty: Ty,
) {
    match tcx.kind(ty).clone() {
        TyKind::Iso(base) if is_unsized(tcx, base) => {
            let (data, meta) = load_fat(cx, ptr);
            match tcx.kind(base).clone() {
                TyKind::Array { elem, .. } => emit_element_loop(cx, tcx, mir, data, elem, meta),
                TyKind::Dyn { .. } => emit_virtual_drop(cx, data, meta),
                other => unreachable!("no unsized type but a slice or a `dyn`: {other:?}"),
            }
            free_allocation(cx, data);
        }
        TyKind::Iso(base) => {
            let data = load_thin(cx, ptr);
            drop_glue(cx, tcx, mir, data, base);
            free_allocation(cx, data);
        }
        TyKind::Fun { .. } => emit_environment_drop(cx, ptr),
        TyKind::Tuple(elems) => emit_field_drops(cx, tcx, mir, ptr, ty, &elems),
        TyKind::Adt { def, args } => match tcx.enum_variant_count(def) {
            None => {
                let fields = tcx.struct_field_tys(def, &args);
                emit_field_drops(cx, tcx, mir, ptr, ty, &fields);
            }
            Some(variant_count) => {
                emit_variant_drops(cx, tcx, mir, ptr, ty, def, &args, variant_count)
            }
        },
        TyKind::Array {
            elem,
            len: Some(len),
        } => {
            let len = cx.llvm.i64_type().const_int(len, false);
            emit_element_loop(cx, tcx, mir, ptr, elem, len);
        }
        other => {
            unreachable!("no drop glue for {other:?}, which `needs_drop` reported owns something")
        }
    }
}

fn emit_environment_drop<'ctx>(cx: &mut CodegenCtx<'ctx>, ptr: PointerValue<'ctx>) {
    let (_, env_word) = load_fat(cx, ptr);
    let env = cx
        .builder
        .build_int_to_ptr(
            env_word,
            cx.llvm.ptr_type(Default::default()),
            "closure.env",
        )
        .unwrap();

    let function = cx
        .builder
        .get_insert_block()
        .and_then(|block| block.get_parent())
        .expect("drop glue is emitted into a function");
    let owned = cx.llvm.append_basic_block(function, "closure.owned");
    let done = cx.llvm.append_basic_block(function, "closure.done");
    let has_env = cx
        .builder
        .build_is_not_null(env, "closure.has_env")
        .unwrap();
    cx.builder
        .build_conditional_branch(has_env, owned, done)
        .unwrap();

    cx.builder.position_at_end(owned);
    emit_glue_word_call(cx, env);
    free_allocation(cx, env);
    cx.builder.build_unconditional_branch(done).unwrap();

    cx.builder.position_at_end(done);
}

fn emit_glue_word_call<'ctx>(cx: &mut CodegenCtx<'ctx>, env: PointerValue<'ctx>) {
    let ptr_ty = cx.llvm.ptr_type(Default::default());
    let glue_word = cx
        .builder
        .build_load(cx.llvm.i64_type(), env, "closure.glue_word")
        .unwrap()
        .into_int_value();
    let glue = cx
        .builder
        .build_int_to_ptr(glue_word, ptr_ty, "closure.glue")
        .unwrap();

    let function = cx
        .builder
        .get_insert_block()
        .and_then(|block| block.get_parent())
        .expect("drop glue is emitted into a function");
    let call = cx.llvm.append_basic_block(function, "closure.drop.call");
    let done = cx.llvm.append_basic_block(function, "closure.drop.done");
    let has_glue = cx
        .builder
        .build_is_not_null(glue, "closure.drops_anything")
        .unwrap();
    cx.builder
        .build_conditional_branch(has_glue, call, done)
        .unwrap();

    cx.builder.position_at_end(call);
    let glue_ty = cx.llvm.void_type().fn_type(&[ptr_ty.into()], false);
    cx.builder
        .build_indirect_call(glue_ty, glue, &[env.into()], "")
        .unwrap();
    cx.builder.build_unconditional_branch(done).unwrap();

    cx.builder.position_at_end(done);
}

pub fn free_iso_shallow<'ctx>(
    cx: &mut CodegenCtx<'ctx>,
    tcx: &mut TyCtx,
    ptr: PointerValue<'ctx>,
    ty: Ty,
) {
    let TyKind::Iso(base) = tcx.kind(ty).clone() else {
        panic!("free_iso_shallow: {ty:?} is not an `iso`, so it holds no allocation of its own");
    };
    let data = if is_unsized(tcx, base) {
        load_fat(cx, ptr).0
    } else {
        load_thin(cx, ptr)
    };
    free_allocation(cx, data);
}

fn load_thin<'ctx>(cx: &CodegenCtx<'ctx>, ptr: PointerValue<'ctx>) -> PointerValue<'ctx> {
    cx.builder
        .build_load(cx.llvm.ptr_type(Default::default()), ptr, "iso.ptr")
        .unwrap()
        .into_pointer_value()
}

fn load_fat<'ctx>(
    cx: &CodegenCtx<'ctx>,
    ptr: PointerValue<'ctx>,
) -> (PointerValue<'ctx>, IntValue<'ctx>) {
    let ptr_ty = cx.llvm.ptr_type(Default::default());
    let i64_ty = cx.llvm.i64_type();
    let two_word_ty = cx.llvm.struct_type(&[ptr_ty.into(), i64_ty.into()], false);
    let data_field = cx
        .builder
        .build_struct_gep(two_word_ty, ptr, 0, "iso.data_ptr")
        .unwrap();
    let data = cx
        .builder
        .build_load(ptr_ty, data_field, "iso.data")
        .unwrap()
        .into_pointer_value();
    let meta_field = cx
        .builder
        .build_struct_gep(two_word_ty, ptr, 1, "iso.meta_ptr")
        .unwrap();
    let meta = cx
        .builder
        .build_load(i64_ty, meta_field, "iso.meta")
        .unwrap()
        .into_int_value();
    (data, meta)
}

fn free_allocation<'ctx>(cx: &mut CodegenCtx<'ctx>, data: PointerValue<'ctx>) {
    cx.builder
        .build_call(cx.libc.free, &[data.into()], "")
        .unwrap();
}

fn emit_virtual_drop<'ctx>(
    cx: &mut CodegenCtx<'ctx>,
    data: PointerValue<'ctx>,
    vtable: IntValue<'ctx>,
) {
    let ptr_ty = cx.llvm.ptr_type(Default::default());
    let vtable_ptr = cx
        .builder
        .build_int_to_ptr(vtable, ptr_ty, "dyn.vtable")
        .unwrap();
    let slot = unsafe {
        cx.builder
            .build_gep(
                ptr_ty,
                vtable_ptr,
                &[cx.llvm
                    .i64_type()
                    .const_int(super::vtable::DROP_SLOT as u64, false)],
                "dyn.drop_slot",
            )
            .unwrap()
    };
    let glue = cx
        .builder
        .build_load(ptr_ty, slot, "dyn.drop")
        .unwrap()
        .into_pointer_value();

    let function = cx
        .builder
        .get_insert_block()
        .and_then(|block| block.get_parent())
        .expect("drop glue is emitted into a function");
    let call = cx.llvm.append_basic_block(function, "dyn.drop.call");
    let done = cx.llvm.append_basic_block(function, "dyn.drop.done");
    let has_glue = cx
        .builder
        .build_is_not_null(glue, "dyn.drops_anything")
        .unwrap();
    cx.builder
        .build_conditional_branch(has_glue, call, done)
        .unwrap();

    cx.builder.position_at_end(call);
    let glue_ty = cx.llvm.void_type().fn_type(&[ptr_ty.into()], false);
    cx.builder
        .build_indirect_call(glue_ty, glue, &[data.into()], "")
        .unwrap();
    cx.builder.build_unconditional_branch(done).unwrap();

    cx.builder.position_at_end(done);
}

fn emit_field_drops<'ctx>(
    cx: &mut CodegenCtx<'ctx>,
    tcx: &mut TyCtx,
    mir: &Mir,
    ptr: PointerValue<'ctx>,
    aggregate_ty: Ty,
    fields: &[Ty],
) {
    for (index, &field_ty) in fields.iter().enumerate() {
        if !tcx.needs_drop(field_ty) {
            continue;
        }
        let struct_ty = llvm_ty::llvm_type(cx, tcx, mir, aggregate_ty).into_struct_type();
        let field = cx
            .builder
            .build_struct_gep(struct_ty, ptr, index as u32, "drop.field")
            .unwrap();
        drop_glue(cx, tcx, mir, field, field_ty);
    }
}

#[allow(clippy::too_many_arguments)]
fn emit_variant_drops<'ctx>(
    cx: &mut CodegenCtx<'ctx>,
    tcx: &mut TyCtx,
    mir: &Mir,
    ptr: PointerValue<'ctx>,
    enum_ty: Ty,
    def: crate::hir::DefId,
    args: &[Ty],
    variant_count: usize,
) {
    let droppable: Vec<usize> = (0..variant_count)
        .filter(|&variant| {
            tcx.variant_field_tys(def, args, variant)
                .into_iter()
                .any(|field| tcx.needs_drop(field))
        })
        .collect();
    if droppable.is_empty() {
        return;
    }

    let enum_llvm_ty = llvm_ty::llvm_type(cx, tcx, mir, enum_ty).into_struct_type();
    let tag_ptr = cx
        .builder
        .build_struct_gep(enum_llvm_ty, ptr, 0, "drop.tag_ptr")
        .unwrap();
    let tag_llvm_ty = enum_llvm_ty
        .get_field_type_at_index(0)
        .expect("an enum's own layout always leads with its tag")
        .into_int_type();
    let tag = cx
        .builder
        .build_load(tag_llvm_ty, tag_ptr, "drop.tag")
        .unwrap()
        .into_int_value();
    let payload = cx
        .builder
        .build_struct_gep(enum_llvm_ty, ptr, 2, "drop.payload")
        .unwrap();

    let switch_block = cx
        .builder
        .get_insert_block()
        .expect("drop glue is emitted into a block");
    let function = switch_block
        .get_parent()
        .expect("drop glue is emitted into a function");
    let done = cx.llvm.append_basic_block(function, "drop.done");

    let mut arms = Vec::with_capacity(droppable.len());
    for variant in droppable {
        let arm = cx.llvm.append_basic_block(function, "drop.variant");
        cx.builder.position_at_end(arm);
        let fields = tcx.variant_field_tys(def, args, variant);
        let payload_ty = tcx.mk_tuple(fields.clone());
        emit_field_drops(cx, tcx, mir, payload, payload_ty, &fields);
        cx.builder.build_unconditional_branch(done).unwrap();
        arms.push((tag_llvm_ty.const_int(variant as u64, false), arm));
    }

    cx.builder.position_at_end(switch_block);
    cx.builder.build_switch(tag, done, &arms).unwrap();
    cx.builder.position_at_end(done);
}

fn emit_element_loop<'ctx>(
    cx: &mut CodegenCtx<'ctx>,
    tcx: &mut TyCtx,
    mir: &Mir,
    base: PointerValue<'ctx>,
    elem: Ty,
    len: IntValue<'ctx>,
) {
    if !tcx.needs_drop(elem) {
        return;
    }
    let function = cx
        .builder
        .get_insert_block()
        .and_then(|block| block.get_parent())
        .expect("drop glue is emitted into a function");
    let i64_ty = cx.llvm.i64_type();
    let index = cx.builder.build_alloca(i64_ty, "drop.index").unwrap();
    cx.builder.build_store(index, i64_ty.const_zero()).unwrap();

    let head = cx.llvm.append_basic_block(function, "drop.loop");
    let body = cx.llvm.append_basic_block(function, "drop.elem");
    let done = cx.llvm.append_basic_block(function, "drop.done");
    cx.builder.build_unconditional_branch(head).unwrap();

    cx.builder.position_at_end(head);
    let current = cx
        .builder
        .build_load(i64_ty, index, "drop.i")
        .unwrap()
        .into_int_value();
    let more = cx
        .builder
        .build_int_compare(inkwell::IntPredicate::ULT, current, len, "drop.more")
        .unwrap();
    cx.builder
        .build_conditional_branch(more, body, done)
        .unwrap();

    cx.builder.position_at_end(body);
    let elem_llvm_ty = llvm_ty::llvm_type(cx, tcx, mir, elem);
    let element = unsafe {
        cx.builder
            .build_gep(elem_llvm_ty, base, &[current], "drop.elem_ptr")
            .unwrap()
    };
    drop_glue(cx, tcx, mir, element, elem);
    let next = cx
        .builder
        .build_int_add(current, i64_ty.const_int(1, false), "drop.next")
        .unwrap();
    cx.builder.build_store(index, next).unwrap();
    cx.builder.build_unconditional_branch(head).unwrap();

    cx.builder.position_at_end(done);
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
        let (_hir, mut tcx, _types, mir, _instances) = crate::testing::lower_to_mir("fun f() {}");
        let mut cx = super::super::ctx::CodegenCtx::new(&llvm, "t");
        scratch_function(&cx);

        let i32_ty = tcx.mk_prim(crate::nameres::PrimTy::I32);
        let iso_ty = tcx.mk_iso(i32_ty);
        let alloca = cx
            .builder
            .build_alloca(cx.llvm.ptr_type(Default::default()), "x")
            .unwrap();

        drop_glue(&mut cx, &mut tcx, &mir, alloca, iso_ty);
        cx.builder.build_return(None).unwrap();

        let ir = cx.module.print_to_string().to_string();
        assert!(ir.contains("call void @free"), "{ir}");
    }

    #[test]
    fn drop_glue_on_iso_unsized_calls_free_on_the_data_pointer_only() {
        let llvm = inkwell::context::Context::create();
        let (_hir, mut tcx, _types, mir, _instances) = crate::testing::lower_to_mir("fun f() {}");
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

        drop_glue(&mut cx, &mut tcx, &mir, alloca, iso_slice_ty);
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
        let (_hir, mut tcx, _types, mir, _instances) = crate::testing::lower_to_mir("fun f() {}");
        let mut cx = super::super::ctx::CodegenCtx::new(&llvm, "t");
        scratch_function(&cx);

        let i32_ty = tcx.mk_prim(crate::nameres::PrimTy::I32);
        let alloca = cx.builder.build_alloca(cx.llvm.i32_type(), "x").unwrap();

        drop_glue(&mut cx, &mut tcx, &mir, alloca, i32_ty);
        cx.builder.build_return(None).unwrap();

        let ir = cx.module.print_to_string().to_string();
        assert!(
            !ir.contains("call void @free"),
            "a plain i32 must not be freed:\n{ir}"
        );
    }

    fn adt_named(hir: &crate::hir::Hir, tcx: &mut TyCtx, name: &str) -> Ty {
        let def = crate::testing::named_def(hir, name);
        tcx.mk_adt(def, Vec::new())
    }

    fn glue_body(ir: &str, ty: Ty) -> String {
        let header = format!("define internal void @drop.glue.{}(ptr %0) {{", ty.index());
        let start = ir
            .find(&header)
            .unwrap_or_else(|| panic!("no glue function emitted for {ty:?}:\n{ir}"));
        let rest = &ir[start..];
        let end = rest.find("\n}").expect("every function is closed");
        rest[..end].to_string()
    }

    #[test]
    fn dropping_an_iso_frees_what_its_pointee_owns_before_the_allocation_itself() {
        let llvm = inkwell::context::Context::create();
        let (hir, mut tcx, _types, mir, _instances) = crate::testing::lower_to_mir(
            "struct Handle { owned: iso i32, tag: i32 }
             fun f() {}",
        );
        let mut cx = super::super::ctx::CodegenCtx::new(&llvm, "t");
        scratch_function(&cx);

        let handle_ty = adt_named(&hir, &mut tcx, "Handle");
        let iso_handle_ty = tcx.mk_iso(handle_ty);
        let alloca = cx
            .builder
            .build_alloca(cx.llvm.ptr_type(Default::default()), "h")
            .unwrap();
        drop_glue(&mut cx, &mut tcx, &mir, alloca, iso_handle_ty);
        cx.builder.build_return(None).unwrap();

        let ir = cx.module.print_to_string().to_string();
        let body = glue_body(&ir, iso_handle_ty);
        let pointee_glue = format!("call void @drop.glue.{}", handle_ty.index());
        let frees_at = body
            .find("call void @free")
            .unwrap_or_else(|| panic!("the allocation itself is never freed:\n{body}"));
        let drops_at = body
            .find(&pointee_glue)
            .unwrap_or_else(|| panic!("the pointee's own drop glue is never called:\n{body}"));
        assert!(
            drops_at < frees_at,
            "what the pointee owns has to be released while the allocation holding it is still \
             there:\n{body}"
        );
    }

    #[test]
    fn dropping_a_self_referential_type_emits_glue_that_calls_itself() {
        let llvm = inkwell::context::Context::create();
        let (hir, mut tcx, _types, mir, _instances) = crate::testing::lower_to_mir(
            "enum List { cons: iso List, nil }
             fun f() {}",
        );
        let mut cx = super::super::ctx::CodegenCtx::new(&llvm, "t");
        scratch_function(&cx);

        let list_ty = adt_named(&hir, &mut tcx, "List");
        let alloca = cx.builder.build_alloca(cx.llvm.i64_type(), "l").unwrap();
        drop_glue(&mut cx, &mut tcx, &mir, alloca, list_ty);
        cx.builder.build_return(None).unwrap();

        let ir = cx.module.print_to_string().to_string();
        let iso_list_ty = tcx.mk_iso(list_ty);
        assert!(
            glue_body(&ir, list_ty)
                .contains(&format!("call void @drop.glue.{}", iso_list_ty.index())),
            "`List`'s glue drops the payload of its own `cons` variant:\n{ir}"
        );
        assert!(
            glue_body(&ir, iso_list_ty)
                .contains(&format!("call void @drop.glue.{}", list_ty.index())),
            "`iso List`'s glue drops the `List` it points at, closing the cycle at runtime \
             rather than at compile time:\n{ir}"
        );
    }

    #[test]
    fn dropping_an_enum_only_frees_the_active_variants_payload() {
        let llvm = inkwell::context::Context::create();
        let (hir, mut tcx, _types, mir, _instances) = crate::testing::lower_to_mir(
            "enum Box_ { present: iso i32, absent }
             fun f() {}",
        );
        let mut cx = super::super::ctx::CodegenCtx::new(&llvm, "t");
        scratch_function(&cx);

        let box_ty = adt_named(&hir, &mut tcx, "Box_");
        let alloca = cx.builder.build_alloca(cx.llvm.i64_type(), "b").unwrap();
        drop_glue(&mut cx, &mut tcx, &mir, alloca, box_ty);
        cx.builder.build_return(None).unwrap();

        let ir = cx.module.print_to_string().to_string();
        let body = glue_body(&ir, box_ty);
        assert!(
            body.contains("switch"),
            "which fields an enum owns is a runtime question about its tag:\n{body}"
        );
        assert_eq!(
            body.matches("call void @drop.glue").count(),
            1,
            "only `present` owns anything, so exactly one arm drops anything:\n{body}"
        );
    }

    #[test]
    fn dropping_an_iso_slice_frees_every_element_before_the_array_itself() {
        let llvm = inkwell::context::Context::create();
        let (_hir, mut tcx, _types, mir, _instances) = crate::testing::lower_to_mir("fun f() {}");
        let mut cx = super::super::ctx::CodegenCtx::new(&llvm, "t");
        scratch_function(&cx);

        let i32_ty = tcx.mk_prim(crate::nameres::PrimTy::I32);
        let elem_ty = tcx.mk_iso(i32_ty);
        let slice_ty = tcx.mk_array(elem_ty, None);
        let iso_slice_ty = tcx.mk_iso(slice_ty);
        let two_word_ty = cx.llvm.struct_type(
            &[
                cx.llvm.ptr_type(Default::default()).into(),
                cx.llvm.i64_type().into(),
            ],
            false,
        );
        let alloca = cx.builder.build_alloca(two_word_ty, "s").unwrap();
        drop_glue(&mut cx, &mut tcx, &mir, alloca, iso_slice_ty);
        cx.builder.build_return(None).unwrap();

        let ir = cx.module.print_to_string().to_string();
        let body = glue_body(&ir, iso_slice_ty);
        assert!(
            body.contains("icmp ult")
                && body.contains(&format!("call void @drop.glue.{}", elem_ty.index())),
            "expected a loop calling the element type's glue:\n{body}"
        );
    }

    #[test]
    fn dropping_an_iso_dyn_calls_the_glue_its_vtable_points_at() {
        let llvm = inkwell::context::Context::create();
        let (hir, mut tcx, _types, mir, _instances) = crate::testing::lower_to_mir(
            "trait Greet { fun greet(&self) -> i32; }
             fun f() {}",
        );
        let mut cx = super::super::ctx::CodegenCtx::new(&llvm, "t");
        scratch_function(&cx);

        let trait_def = crate::testing::named_def(&hir, "Greet");
        let dyn_ty = tcx.mk_dyn(trait_def, Vec::new());
        let iso_dyn_ty = tcx.mk_iso(dyn_ty);
        let two_word_ty = cx.llvm.struct_type(
            &[
                cx.llvm.ptr_type(Default::default()).into(),
                cx.llvm.i64_type().into(),
            ],
            false,
        );
        let alloca = cx.builder.build_alloca(two_word_ty, "d").unwrap();
        drop_glue(&mut cx, &mut tcx, &mir, alloca, iso_dyn_ty);
        cx.builder.build_return(None).unwrap();

        let ir = cx.module.print_to_string().to_string();
        let body = glue_body(&ir, iso_dyn_ty);
        assert!(
            body.contains("dyn.drop_slot") && body.contains("call void %"),
            "expected an indirect call through the vtable's drop slot:\n{body}"
        );
        assert!(
            body.contains("icmp ne ptr") || body.contains("dyn.drops_anything"),
            "expected the null drop slot to be checked before calling:\n{body}"
        );
        assert!(
            body.contains("call void @free"),
            "the allocation itself is still released:\n{body}"
        );
    }

    #[test]
    fn a_type_that_owns_nothing_gets_no_glue_at_all() {
        let llvm = inkwell::context::Context::create();
        let (hir, mut tcx, _types, mir, _instances) = crate::testing::lower_to_mir(
            "struct Point { x: i32, y: i32 }
             fun f() {}",
        );
        let mut cx = super::super::ctx::CodegenCtx::new(&llvm, "t");
        scratch_function(&cx);

        let point_ty = adt_named(&hir, &mut tcx, "Point");
        let alloca = cx.builder.build_alloca(cx.llvm.i64_type(), "p").unwrap();
        drop_glue(&mut cx, &mut tcx, &mir, alloca, point_ty);
        cx.builder.build_return(None).unwrap();

        let ir = cx.module.print_to_string().to_string();
        assert!(
            !ir.contains("drop.glue"),
            "a struct of plain fields needs no glue emitted for it at all:\n{ir}"
        );
    }
}
