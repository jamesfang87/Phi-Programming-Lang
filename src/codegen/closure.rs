use inkwell::types::BasicType;
use inkwell::values::{BasicValueEnum, FunctionValue, PointerValue, StructValue};

use super::ctx::CodegenCtx;
use super::{drop, layout, ty as llvm_ty};
use crate::hir::DefId;
use crate::mir::mangle::mangle;
use crate::mir::{Instance, Mir};
use crate::typeck::ty::Ty;
use crate::typeck::tyctx::TyCtx;

const GLUE_FIELD: u32 = 0;

pub fn environment_ty(tcx: &mut TyCtx, captures: &[Ty]) -> Ty {
    let mut fields = vec![tcx.mk_prim(crate::nameres::PrimTy::Usize)];
    fields.extend_from_slice(captures);
    tcx.mk_tuple(fields)
}

pub fn build_value<'ctx>(
    cx: &mut CodegenCtx<'ctx>,
    tcx: &mut TyCtx,
    mir: &Mir,
    def: DefId,
    args: &[Ty],
    captures: &[(BasicValueEnum<'ctx>, Ty)],
) -> BasicValueEnum<'ctx> {
    let instance = Instance {
        def,
        any_mode: None,
        args: args.to_vec(),
    };
    let name = mangle(mir, tcx, &instance);
    let code = cx
        .functions
        .get(&name)
        .unwrap_or_else(|| {
            panic!(
                "closure::build_value: no declared function named {name:?} for {instance:?} -- \
                 every closure `mir::monomorphize` reached is declared alongside every other \
                 instance"
            )
        })
        .as_global_value()
        .as_pointer_value();

    let env = match captures.is_empty() {
        true => cx.llvm.i64_type().const_zero(),
        false => {
            let capture_tys: Vec<Ty> = captures.iter().map(|&(_, ty)| ty).collect();
            let env_ty = environment_ty(tcx, &capture_tys);
            let env_llvm_ty = llvm_ty::llvm_type(cx, tcx, mir, env_ty).into_struct_type();
            let size = layout::layout_of(tcx, mir, env_ty).size;
            let env_ptr = cx
                .builder
                .build_call(
                    cx.libc.malloc,
                    &[cx.llvm.i64_type().const_int(size, false).into()],
                    "closure.env",
                )
                .unwrap()
                .try_as_basic_value()
                .unwrap_basic()
                .into_pointer_value();

            let glue = match drop::glue_pointer(cx, tcx, mir, env_ty) {
                Some(glue) => cx
                    .builder
                    .build_ptr_to_int(glue, cx.llvm.i64_type(), "closure.glue")
                    .unwrap(),
                None => cx.llvm.i64_type().const_zero(),
            };
            store_field(cx, env_llvm_ty, env_ptr, GLUE_FIELD, glue.into());
            for (index, &(value, _)) in captures.iter().enumerate() {
                store_field(cx, env_llvm_ty, env_ptr, index as u32 + 1, value);
            }

            cx.builder
                .build_ptr_to_int(env_ptr, cx.llvm.i64_type(), "closure.env_word")
                .unwrap()
        }
    };

    pair(cx, code, env.into())
}

fn store_field<'ctx>(
    cx: &mut CodegenCtx<'ctx>,
    struct_ty: inkwell::types::StructType<'ctx>,
    ptr: PointerValue<'ctx>,
    index: u32,
    value: BasicValueEnum<'ctx>,
) {
    let field = cx
        .builder
        .build_struct_gep(struct_ty, ptr, index, "closure.field")
        .unwrap();
    cx.builder.build_store(field, value).unwrap();
}

fn pair<'ctx>(
    cx: &CodegenCtx<'ctx>,
    code: PointerValue<'ctx>,
    env: BasicValueEnum<'ctx>,
) -> BasicValueEnum<'ctx> {
    let value = cx
        .llvm
        .struct_type(
            &[
                cx.llvm.ptr_type(Default::default()).into(),
                cx.llvm.i64_type().into(),
            ],
            false,
        )
        .get_undef();
    let value = cx
        .builder
        .build_insert_value(value, code, 0, "fun.code")
        .unwrap()
        .into_struct_value();
    cx.builder
        .build_insert_value(value, env, 1, "fun.env")
        .unwrap()
        .into_struct_value()
        .into()
}

pub fn reify<'ctx>(
    cx: &mut CodegenCtx<'ctx>,
    function: FunctionValue<'ctx>,
    sret: bool,
) -> BasicValueEnum<'ctx> {
    let thunk = thunk_for(cx, function, sret)
        .as_global_value()
        .as_pointer_value();
    let null_env = cx.llvm.i64_type().const_zero();
    pair(cx, thunk, null_env.into())
}

pub fn unpack<'ctx>(
    cx: &CodegenCtx<'ctx>,
    value: StructValue<'ctx>,
) -> (PointerValue<'ctx>, PointerValue<'ctx>) {
    let code = cx
        .builder
        .build_extract_value(value, 0, "call.code")
        .unwrap()
        .into_pointer_value();
    let env_word = cx
        .builder
        .build_extract_value(value, 1, "call.env_word")
        .unwrap()
        .into_int_value();
    let env = cx
        .builder
        .build_int_to_ptr(env_word, cx.llvm.ptr_type(Default::default()), "call.env")
        .unwrap();
    (code, env)
}

fn thunk_for<'ctx>(
    cx: &mut CodegenCtx<'ctx>,
    function: FunctionValue<'ctx>,
    sret: bool,
) -> FunctionValue<'ctx> {
    let name = format!("fun.thunk.{}", function.get_name().to_string_lossy());
    if let Some(&thunk) = cx.fun_thunks.borrow().get(&name) {
        return thunk;
    }

    let target_ty = function.get_type();
    let mut params: Vec<inkwell::types::BasicMetadataTypeEnum<'ctx>> = target_ty.get_param_types();
    let env_at = usize::from(sret);
    params.insert(env_at, cx.llvm.ptr_type(Default::default()).into());
    let thunk_ty = match target_ty.get_return_type() {
        Some(ret) => ret.fn_type(&params, false),
        None => cx.llvm.void_type().fn_type(&params, false),
    };
    let thunk = cx
        .module
        .add_function(&name, thunk_ty, Some(inkwell::module::Linkage::Internal));
    cx.fun_thunks.borrow_mut().insert(name, thunk);

    let resume_at = cx.builder.get_insert_block();
    let entry = cx.llvm.append_basic_block(thunk, "entry");
    cx.builder.position_at_end(entry);
    let forwarded: Vec<inkwell::values::BasicMetadataValueEnum<'ctx>> = thunk
        .get_param_iter()
        .enumerate()
        .filter(|&(index, _)| index != env_at)
        .map(|(_, param)| param.into())
        .collect();
    let call = cx.builder.build_call(function, &forwarded, "").unwrap();
    match call.try_as_basic_value().basic() {
        Some(value) => cx.builder.build_return(Some(&value)).unwrap(),
        None => cx.builder.build_return(None).unwrap(),
    };
    if let Some(block) = resume_at {
        cx.builder.position_at_end(block);
    }

    thunk
}

#[cfg(test)]
mod tests {
    fn ir_for(src: &str) -> String {
        let (_hir, mut tcx, _types, mir, instances) = crate::testing::lower_to_mir(src);
        let instances = crate::mir::drop_elaboration::elaborate_drops(&mut tcx, instances);
        let llvm = inkwell::context::Context::create();
        let module = super::super::codegen(&llvm, &mut tcx, &mir, &instances, "t").unwrap();
        module.verify().unwrap();
        module.print_to_string().to_string()
    }

    fn function_body<'a>(ir: &'a str, name: &str) -> &'a str {
        let prefix = format!("{name}_h");
        ir.split("define ")
            .find(|chunk| {
                chunk
                    .split_once('@')
                    .is_some_and(|(_, rest)| rest.starts_with(&prefix))
            })
            .unwrap_or_else(|| panic!("no function named {name:?} in:\n{ir}"))
    }

    #[test]
    fn a_capturing_closure_allocates_an_environment_and_stores_what_it_captured() {
        let ir = ir_for("fun f() -> i32 { let n = 7; let g = || n; return g(); }");
        let body = function_body(&ir, "crate_f");
        assert!(
            body.contains("call ptr @malloc"),
            "the captures outlive the expression that built them, so they need storage of their \
             own:\n{body}"
        );
        assert!(
            body.contains("closure.field"),
            "expected the capture to be stored into the environment:\n{body}"
        );
    }

    #[test]
    fn a_closure_that_captures_nothing_allocates_no_environment() {
        let ir = ir_for("fun f() -> i32 { let g = || 1; return g(); }");
        let body = function_body(&ir, "crate_f");
        assert!(
            !body.contains("call ptr @malloc"),
            "there is nothing to keep, so there is nothing to allocate:\n{body}"
        );
        assert!(
            body.contains("i64 0 }") || body.contains("i64 0,"),
            "expected a null environment word paired with the closure's code:\n{body}"
        );
    }

    #[test]
    fn calling_a_closure_passes_its_environment_to_its_code() {
        let ir = ir_for(
            "fun apply(g: fun(i32) -> i32, x: i32) -> i32 { return g(x); }
             fun f() -> i32 { let n = 7; return apply(|x: i32| -> i32 { return n; }, 1); }",
        );
        let body = function_body(&ir, "crate_apply");
        assert!(
            body.contains("call.code") && body.contains("call.env"),
            "a `fun` value is split into the callee and the environment it is called with:\n{body}"
        );
        assert!(
            body.contains("%call.code(ptr %call.env"),
            "the environment is the callee's first argument:\n{body}"
        );
    }

    #[test]
    fn a_closure_owning_what_it_captured_releases_it_when_the_value_is_dropped() {
        let ir = ir_for("fun f() -> i32 { let owned = new 7; let g = || *owned; return g(); }");
        let body = function_body(&ir, "crate_f");
        assert!(
            body.contains("call void @drop.glue."),
            "expected the closure value to be dropped:\n{body}"
        );
        assert!(
            ir.contains("closure.has_env") && ir.contains("closure.drops_anything"),
            "expected glue that asks the value whether it has an environment, and the \
             environment whether it owns anything:\n{ir}"
        );
    }
}
