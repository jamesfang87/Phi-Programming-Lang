use inkwell::types::{FloatType, IntType};
use inkwell::values::BasicValueEnum;

use super::ctx::CodegenCtx;
use crate::ast::Symbol;
use crate::mir::{ConstKind, Constant, FunRef, Instance};
use crate::nameres::PrimTy;
use crate::typeck::ty::Ty;
use crate::typeck::ty::TyKind;
use crate::typeck::ty::ctx::TyCtx;

/// Whether an integer constant's value should be sign-extended into its LLVM type.
#[derive(Clone, Copy)]
enum IntegerSignedness {
    Signed,
    Unsigned,
}

impl IntegerSignedness {
    fn sign_extend(self) -> bool {
        matches!(self, IntegerSignedness::Signed)
    }
}

pub fn lower_constant<'ctx>(
    cx: &mut CodegenCtx<'ctx>,
    tcx: &mut TyCtx,
    constant: &Constant,
) -> BasicValueEnum<'ctx> {
    match &constant.kind {
        ConstKind::Int(value) => lower_integer_constant(cx, tcx, constant.ty, *value),
        ConstKind::Float(value) => select_float_llvm_type(cx, tcx, constant.ty)
            .const_float(*value)
            .into(),
        ConstKind::Bool(value) => cx.llvm.bool_type().const_int(*value as u64, false).into(),
        ConstKind::Char(value) => cx.llvm.i32_type().const_int(*value as u64, false).into(),
        ConstKind::Str(sym) => lower_string_literal(cx, *sym),
        ConstKind::FunDef(fun) => lower_function_reference(cx, tcx, constant.ty, fun),
    }
}

fn lower_integer_constant<'ctx>(
    cx: &CodegenCtx<'ctx>,
    tcx: &mut TyCtx,
    ty: Ty,
    value: i128,
) -> BasicValueEnum<'ctx> {
    if matches!(tcx.kind(ty), TyKind::Primitive(PrimTy::F32 | PrimTy::F64)) {
        return select_float_llvm_type(cx, tcx, ty)
            .const_float(value as f64)
            .into();
    }
    let (int_ty, signedness) = select_int_llvm_type(cx, tcx, ty);
    int_ty
        .const_int(value as u64, signedness.sign_extend())
        .into()
}

/// Returns the value of a function constant: a fat-pointer closure pointing at `fun`'s reified
/// thunk.
fn lower_function_reference<'ctx>(
    cx: &mut CodegenCtx<'ctx>,
    tcx: &mut TyCtx,
    fn_ty: Ty,
    fun: &FunRef,
) -> BasicValueEnum<'ctx> {
    let instance = Instance {
        def: fun.def,
        any_mode: fun.any_mode,
        args: fun.args.clone(),
        self_ty: fun.self_ty,
    };
    let name = cx.mangle(tcx, &instance);
    let function = cx.functions[&name];
    let sret = matches!(
        tcx.kind(fn_ty).clone(),
        TyKind::Fun { ret: Some(ret), .. }
            if matches!(super::ty::classify_abi(tcx, ret), super::ty::AbiClass::Indirect)
    );
    super::closure::reify(cx, function, sret)
}

fn select_int_llvm_type<'ctx>(
    cx: &CodegenCtx<'ctx>,
    tcx: &mut TyCtx,
    ty: Ty,
) -> (IntType<'ctx>, IntegerSignedness) {
    let prim = match tcx.kind(ty) {
        TyKind::Primitive(prim) => *prim,
        other => unreachable!("ConstKind::Int has a non-integer type {other:?}"),
    };
    let signedness = if matches!(prim, PrimTy::I8 | PrimTy::I16 | PrimTy::I32 | PrimTy::I64) {
        IntegerSignedness::Signed
    } else {
        IntegerSignedness::Unsigned
    };
    let int_ty = match prim {
        PrimTy::I8 | PrimTy::U8 => cx.llvm.i8_type(),
        PrimTy::I16 | PrimTy::U16 => cx.llvm.i16_type(),
        PrimTy::I32 | PrimTy::U32 => cx.llvm.i32_type(),
        PrimTy::I64 | PrimTy::U64 | PrimTy::Usize => cx.llvm.i64_type(),
        other => unreachable!("ConstKind::Int has a non-integer primitive {other:?}"),
    };
    (int_ty, signedness)
}

fn select_float_llvm_type<'ctx>(cx: &CodegenCtx<'ctx>, tcx: &TyCtx, ty: Ty) -> FloatType<'ctx> {
    if matches!(tcx.kind(ty), TyKind::Primitive(PrimTy::F32)) {
        cx.llvm.f32_type()
    } else {
        cx.llvm.f64_type()
    }
}

fn lower_string_literal<'ctx>(cx: &mut CodegenCtx<'ctx>, sym: Symbol) -> BasicValueEnum<'ctx> {
    let bytes = cx.session.resolve(sym).as_bytes();
    let next_index = cx.strings.borrow().len();
    let ptr = *cx.strings.borrow_mut().entry(sym).or_insert_with(|| {
        let global = cx.module.add_global(
            cx.llvm.i8_type().array_type(bytes.len() as u32),
            None,
            &format!(".str.{next_index}"),
        );
        global.set_initializer(&cx.llvm.const_string(bytes, false));
        global.set_linkage(inkwell::module::Linkage::Private);
        global.set_unnamed_addr(true);
        global.set_constant(true);
        global.as_pointer_value()
    });
    let len = cx.llvm.i64_type().const_int(bytes.len() as u64, false);
    cx.llvm
        .const_struct(&[ptr.into(), len.into()], false)
        .into()
}

#[cfg(test)]
mod tests {
    use super::super::body::lower_operand;
    use super::super::ctx::CodegenCtx;
    use crate::mir::{Body, Operand, Rvalue, StatementKind};

    #[test]
    fn equal_string_literals_share_one_global_distinct_ones_dont() {
        let (hir, mut tcx, _types, mir, instances) = crate::testing::lower_to_mir(
            r#"fun f() { let a = "hi"; let b = "hi"; let c = "bye"; }"#,
        );
        let body = instances.values().next().expect("one instance");
        let string_constants = find_string_constants(body);
        assert_eq!(
            string_constants.len(),
            3,
            "expected three string-literal operands in {body:?}"
        );

        let llvm = inkwell::context::Context::create();
        let mut cx = CodegenCtx::new(crate::testing::session(), &hir, &llvm, "t");
        let locals = std::collections::HashMap::new();
        for operand in &string_constants {
            lower_operand(&mut cx, &mut tcx, &mir, &locals, &body.local_decls, operand);
        }

        let ir = cx.module.print_to_string().to_string();
        assert_eq!(
            ir.matches("@.str.").count(),
            2,
            "two equal literals should intern to one global, and the distinct third literal to \
             a second, separate one:\n{ir}"
        );
    }

    fn find_string_constants(body: &Body) -> Vec<Operand> {
        let mut found = Vec::new();
        for block in &body.basic_blocks {
            for statement in &block.statements {
                if let StatementKind::Assign(_, Rvalue::Use(operand)) = &statement.kind
                    && let Operand::Constant(constant) = operand
                    && matches!(constant.kind, super::ConstKind::Str(_))
                {
                    found.push(operand.clone());
                }
            }
        }
        found
    }
}
