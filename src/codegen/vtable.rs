use inkwell::module::Linkage;
use inkwell::types::FunctionType;
use inkwell::values::{BasicMetadataValueEnum, BasicValueEnum, CallSiteValue, PointerValue};

use super::ctx::CodegenCtx;
use super::layout;
use crate::hir::DefId;
use crate::mir::mangle::mangle;
use crate::mir::{Instance, Mir};
use crate::typeck::ty::Ty;
use crate::typeck::tyctx::TyCtx;

const HEADER_SLOTS: usize = 3;

pub(super) const DROP_SLOT: usize = 2;

fn two_word_struct_type<'ctx>(cx: &CodegenCtx<'ctx>) -> inkwell::types::StructType<'ctx> {
    cx.llvm.struct_type(
        &[
            cx.llvm.ptr_type(Default::default()).into(),
            cx.llvm.i64_type().into(),
        ],
        false,
    )
}

pub fn vtable_for<'ctx>(
    cx: &mut CodegenCtx<'ctx>,
    tcx: &mut TyCtx,
    mir: &Mir,
    concrete: Ty,
    trait_: DefId,
) -> PointerValue<'ctx> {
    if let Some(&ptr) = cx.vtables.borrow().get(&(concrete, trait_)) {
        return ptr;
    }

    let layout = layout::layout_of(tcx, mir, concrete);
    let ptr_ty = cx.llvm.ptr_type(Default::default());
    let i64_ty = cx.llvm.i64_type();

    let drop_glue = match super::drop::glue_pointer(cx, tcx, mir, concrete) {
        Some(glue) => glue,
        None => ptr_ty.const_null(),
    };
    let mut fields: Vec<BasicValueEnum<'ctx>> = vec![
        i64_ty.const_int(layout.size, false).into(),
        i64_ty.const_int(layout.align, false).into(),
        drop_glue.into(),
    ];

    let info = mir.vtables.get(&(concrete, trait_)).unwrap_or_else(|| {
        panic!(
            "vtable_for: no `extend .. with Trait` block implementing {trait_:?} was found for \
             {concrete:?} -- either no such impl exists, or it's a generic `extend<T> ..` block \
             whose self type doesn't resolve to a concrete `Ty`"
        )
    });
    for (i, impl_method) in info.methods.iter().enumerate() {
        let impl_method = impl_method.unwrap_or_else(|| {
            panic!(
                "vtable_for: {concrete:?}'s `extend .. with {trait_:?}` block never defines \
                 trait method index {i}"
            )
        });
        let instance = Instance {
            def: impl_method,
            any_mode: None,
            args: Vec::new(),
        };
        let name = mangle(mir, tcx, &instance);
        let function = *cx.functions.get(&name).unwrap_or_else(|| {
            panic!(
                "vtable_for: no declared function named {name:?} for instance {instance:?} -- \
                 every instance a vtable references must already be in cx.functions, same as any \
                 other call target"
            )
        });
        fields.push(function.as_global_value().as_pointer_value().into());
    }

    let const_struct = cx.llvm.const_struct(&fields, false);
    let global_name = format!("vt.{}.{}", trait_.index(), concrete.index());
    let global = cx
        .module
        .add_global(const_struct.get_type(), None, &global_name);
    global.set_initializer(&const_struct);
    global.set_linkage(Linkage::Private);
    global.set_constant(true);
    global.set_unnamed_addr(true);
    let vtable_ptr = global.as_pointer_value();

    cx.vtables
        .borrow_mut()
        .insert((concrete, trait_), vtable_ptr);
    vtable_ptr
}

pub fn unsize<'ctx>(
    cx: &mut CodegenCtx<'ctx>,
    tcx: &mut TyCtx,
    mir: &Mir,
    data_ptr: PointerValue<'ctx>,
    concrete: Ty,
    trait_: DefId,
) -> BasicValueEnum<'ctx> {
    let vtable_ptr = vtable_for(cx, tcx, mir, concrete, trait_);
    let vtable_word = cx
        .builder
        .build_ptr_to_int(vtable_ptr, cx.llvm.i64_type(), "vtable_word")
        .unwrap();

    let two_word_ty = two_word_struct_type(cx);
    let mut agg = two_word_ty.get_undef();
    agg = cx
        .builder
        .build_insert_value(agg, data_ptr, 0, "dyn.data")
        .unwrap()
        .into_struct_value();
    agg = cx
        .builder
        .build_insert_value(agg, vtable_word, 1, "dyn.vtable")
        .unwrap()
        .into_struct_value();
    agg.into()
}

pub fn call_dyn_method<'ctx>(
    cx: &mut CodegenCtx<'ctx>,
    dyn_value: BasicValueEnum<'ctx>,
    method_index: usize,
    fn_type: FunctionType<'ctx>,
    rest_args: &[BasicMetadataValueEnum<'ctx>],
) -> CallSiteValue<'ctx> {
    let struct_val = dyn_value.into_struct_value();
    let data_ptr = cx
        .builder
        .build_extract_value(struct_val, 0, "self.data")
        .unwrap()
        .into_pointer_value();
    let vtable_word = cx
        .builder
        .build_extract_value(struct_val, 1, "self.vtable_word")
        .unwrap()
        .into_int_value();
    let ptr_ty = cx.llvm.ptr_type(Default::default());
    let vtable_ptr = cx
        .builder
        .build_int_to_ptr(vtable_word, ptr_ty, "self.vtable")
        .unwrap();
    let slot = cx
        .llvm
        .i64_type()
        .const_int((HEADER_SLOTS + method_index) as u64, false);
    let slot_ptr = unsafe {
        cx.builder
            .build_gep(ptr_ty, vtable_ptr, &[slot], "method_slot")
            .unwrap()
    };
    let method_ptr = cx
        .builder
        .build_load(ptr_ty, slot_ptr, "method_ptr")
        .unwrap()
        .into_pointer_value();

    let mut args: Vec<BasicMetadataValueEnum<'ctx>> = vec![data_ptr.into()];
    args.extend_from_slice(rest_args);
    cx.builder
        .build_indirect_call(fn_type, method_ptr, &args, "dyn_call")
        .unwrap()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::hir::{Hir, OwnerNode};

    fn scratch_function<'ctx>(cx: &CodegenCtx<'ctx>) -> inkwell::values::FunctionValue<'ctx> {
        let function =
            cx.module
                .add_function("scratch", cx.llvm.void_type().fn_type(&[], false), None);
        let entry = cx.llvm.append_basic_block(function, "entry");
        cx.builder.position_at_end(entry);
        function
    }

    fn find_struct_def(hir: &Hir, name: &str) -> DefId {
        for def_id in hir.def_ids() {
            if let OwnerNode::Struct(struct_) = hir.def(def_id)
                && crate::ast::interner::Interner::resolve(struct_.name.text) == name
            {
                return def_id;
            }
        }
        panic!("no struct named {name:?} found");
    }

    fn find_trait_def(hir: &Hir, name: &str) -> DefId {
        for def_id in hir.def_ids() {
            if let OwnerNode::Trait(trait_) = hir.def(def_id)
                && crate::ast::interner::Interner::resolve(trait_.name.text) == name
            {
                return def_id;
            }
        }
        panic!("no trait named {name:?} found");
    }

    fn declare_all<'ctx>(
        cx: &mut CodegenCtx<'ctx>,
        tcx: &mut TyCtx,
        mir: &Mir,
        instances: &std::collections::HashMap<Instance, crate::mir::Body>,
    ) {
        for (instance, body) in instances {
            let name = mangle(mir, tcx, instance);
            let fn_type = super::super::ty::function_type(cx, tcx, mir, body);
            let function = cx.module.add_function(&name, fn_type, None);
            cx.functions.insert(name, function);
        }
    }

    const TRAIT_AND_IMPL_SRC: &str = "trait Greet { fun greet(&self) -> i32; }
struct Point { x: i32, y: i32 }
extend Point with Greet { fun greet(&self) -> i32 { return self.x; } }
fun f() {}";

    #[test]
    fn vtable_global_has_size_align_null_drop_and_method_pointer() {
        let (hir, mut tcx, _types, mir, instances) =
            crate::testing::lower_to_mir(TRAIT_AND_IMPL_SRC);
        let llvm = inkwell::context::Context::create();
        let mut cx = CodegenCtx::new(&llvm, "t");
        declare_all(&mut cx, &mut tcx, &mir, &instances);

        let point_def = find_struct_def(&hir, "Point");
        let trait_def = find_trait_def(&hir, "Greet");
        let point_ty = tcx.mk_adt(point_def, Vec::new());

        let vtable_ptr = vtable_for(&mut cx, &mut tcx, &mir, point_ty, trait_def);
        let vtable_ptr_again = vtable_for(&mut cx, &mut tcx, &mir, point_ty, trait_def);
        assert_eq!(vtable_ptr, vtable_ptr_again);

        let ir = cx.module.print_to_string().to_string();
        assert!(ir.contains("@vt."), "expected a @vt. global:\n{ir}");
        assert!(
            ir.contains("i64 8, i64 4, ptr null"),
            "expected size 8, align 4, and a null drop-glue slot in that order:\n{ir}"
        );
        let impl_method =
            mir.vtables[&(point_ty, trait_def)].methods[0].expect("Point implements greet");
        let greet_name = mangle(
            &mir,
            &tcx,
            &Instance {
                def: impl_method,
                any_mode: None,
                args: Vec::new(),
            },
        );
        assert!(
            ir.contains(&format!("@{greet_name}")),
            "expected the vtable to reference {greet_name:?}:\n{ir}"
        );
    }

    #[test]
    fn vtable_of_a_type_that_owns_something_carries_its_drop_glue() {
        let (hir, mut tcx, _types, mir, instances) = crate::testing::lower_to_mir(
            "trait Greet { fun greet(&self) -> i32; }
             struct Owner { h: iso i32 }
             extend Owner with Greet { fun greet(&self) -> i32 { return 1; } }
             fun f() {}",
        );
        let llvm = inkwell::context::Context::create();
        let mut cx = CodegenCtx::new(&llvm, "t");
        declare_all(&mut cx, &mut tcx, &mir, &instances);

        let owner_def = find_struct_def(&hir, "Owner");
        let trait_def = find_trait_def(&hir, "Greet");
        let owner_ty = tcx.mk_adt(owner_def, Vec::new());
        vtable_for(&mut cx, &mut tcx, &mir, owner_ty, trait_def);

        let ir = cx.module.print_to_string().to_string();
        assert!(
            ir.contains(&format!("ptr @drop.glue.{}", owner_ty.index())),
            "expected `Owner`'s own drop glue in the vtable's drop slot:\n{ir}"
        );
    }

    #[test]
    fn unsize_pairs_data_pointer_with_the_matching_vtable() {
        let (hir, mut tcx, _types, mir, instances) =
            crate::testing::lower_to_mir(TRAIT_AND_IMPL_SRC);
        let llvm = inkwell::context::Context::create();
        let mut cx = CodegenCtx::new(&llvm, "t");
        declare_all(&mut cx, &mut tcx, &mir, &instances);
        scratch_function(&cx);

        let point_def = find_struct_def(&hir, "Point");
        let trait_def = find_trait_def(&hir, "Greet");
        let point_ty = tcx.mk_adt(point_def, Vec::new());

        let data_ptr = cx.builder.build_alloca(cx.llvm.i32_type(), "data").unwrap();
        let dyn_value = unsize(&mut cx, &mut tcx, &mir, data_ptr, point_ty, trait_def);
        cx.builder.build_return(None).unwrap();

        assert!(matches!(dyn_value, BasicValueEnum::StructValue(_)));
        let ir = cx.module.print_to_string().to_string();
        assert!(
            ir.contains("ptrtoint"),
            "expected a ptrtoint into the dyn value:\n{ir}"
        );
        assert!(
            ir.contains("@vt."),
            "expected the paired vtable global:\n{ir}"
        );
    }

    #[test]
    fn call_dyn_method_loads_the_vtable_slot_and_calls_through_it() {
        let (hir, mut tcx, _types, mir, instances) =
            crate::testing::lower_to_mir(TRAIT_AND_IMPL_SRC);
        let llvm = inkwell::context::Context::create();
        let mut cx = CodegenCtx::new(&llvm, "t");
        declare_all(&mut cx, &mut tcx, &mir, &instances);
        let function =
            cx.module
                .add_function("scratch", cx.llvm.i32_type().fn_type(&[], false), None);
        let entry = cx.llvm.append_basic_block(function, "entry");
        cx.builder.position_at_end(entry);

        let point_def = find_struct_def(&hir, "Point");
        let trait_def = find_trait_def(&hir, "Greet");
        let point_ty = tcx.mk_adt(point_def, Vec::new());

        let data_ptr = cx.builder.build_alloca(cx.llvm.i32_type(), "data").unwrap();
        let dyn_value = unsize(&mut cx, &mut tcx, &mir, data_ptr, point_ty, trait_def);

        let fn_type = cx
            .llvm
            .i32_type()
            .fn_type(&[cx.llvm.ptr_type(Default::default()).into()], false);
        let call_site = call_dyn_method(&mut cx, dyn_value, 0, fn_type, &[]);
        let ret_val = call_site.try_as_basic_value().unwrap_basic();
        cx.builder.build_return(Some(&ret_val)).unwrap();

        assert!(function.verify(true), "{:?}", cx.module.print_to_string());

        let ir = cx.module.print_to_string().to_string();
        assert!(
            ir.contains("getelementptr"),
            "expected a GEP to the method's vtable slot:\n{ir}"
        );
        assert!(
            ir.contains("call i32"),
            "expected an indirect call through the loaded method pointer:\n{ir}"
        );
    }

    #[test]
    fn resolve_impl_method_disambiguates_by_concrete_type_arguments() {
        let (hir, mut tcx, _types, mir, instances) = crate::testing::lower_to_mir(
            "trait Show { fun show(&self) -> i32; }
struct Wrap<T> { value: T }
extend Wrap<i32> with Show { fun show(&self) -> i32 { return 1; } }
extend Wrap<bool> with Show { fun show(&self) -> i32 { return 2; } }
fun f() {}",
        );
        let llvm = inkwell::context::Context::create();
        let mut cx = CodegenCtx::new(&llvm, "t");
        declare_all(&mut cx, &mut tcx, &mir, &instances);

        let wrap_def = find_struct_def(&hir, "Wrap");
        let trait_def = find_trait_def(&hir, "Show");
        let i32_ty = tcx.mk_prim(crate::nameres::PrimTy::I32);
        let bool_ty = tcx.mk_prim(crate::nameres::PrimTy::Bool);
        let wrap_i32 = tcx.mk_adt(wrap_def, vec![i32_ty]);
        let wrap_bool = tcx.mk_adt(wrap_def, vec![bool_ty]);

        let show_index = 0;
        let impl_for_i32 = mir.vtables[&(wrap_i32, trait_def)].methods[show_index]
            .expect("Wrap<i32> implements show");
        let impl_for_bool = mir.vtables[&(wrap_bool, trait_def)].methods[show_index]
            .expect("Wrap<bool> implements show");

        assert_ne!(
            impl_for_i32, impl_for_bool,
            "Wrap<i32> and Wrap<bool> have distinct `show` impls; resolving both requests to the \
             same DefId means the type-argument check isn't actually discriminating"
        );

        let vt_i32 = vtable_for(&mut cx, &mut tcx, &mir, wrap_i32, trait_def);
        let vt_bool = vtable_for(&mut cx, &mut tcx, &mir, wrap_bool, trait_def);
        assert_ne!(
            vt_i32, vt_bool,
            "distinct concrete instantiations get distinct vtables"
        );

        let name_for = |impl_def: DefId| {
            mangle(
                &mir,
                &tcx,
                &Instance {
                    def: impl_def,
                    any_mode: None,
                    args: Vec::new(),
                },
            )
        };
        let name_i32 = name_for(impl_for_i32);
        let name_bool = name_for(impl_for_bool);
        let ir = cx.module.print_to_string().to_string();
        assert!(
            ir.contains(&format!("@{name_i32}")),
            "expected Wrap<i32>'s vtable to reference its own impl {name_i32:?}:\n{ir}"
        );
        assert!(
            ir.contains(&format!("@{name_bool}")),
            "expected Wrap<bool>'s vtable to reference its own impl {name_bool:?}:\n{ir}"
        );
    }
}
