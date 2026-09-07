use inkwell::values::BasicValueEnum;

use super::ctx::CodegenCtx;
use crate::ast::Symbol;
use crate::ast::interner::Interner;
use crate::mir::mangle::mangle;
use crate::mir::{ConstKind, Constant, Instance, Mir};
use crate::nameres::PrimTy;
use crate::typeck::ty::TyKind;
use crate::typeck::tyctx::TyCtx;

pub fn lower_constant<'ctx>(
    cx: &mut CodegenCtx<'ctx>,
    tcx: &mut TyCtx,
    mir: &Mir,
    constant: &Constant,
) -> BasicValueEnum<'ctx> {
    match &constant.kind {
        ConstKind::Int(v) => {
            let (int_ty, is_signed) = int_llvm_type(cx, tcx, constant.ty);
            int_ty.const_int(*v as u64, is_signed).into()
        }
        ConstKind::Float(v) => match tcx.kind(constant.ty) {
            TyKind::Primitive(PrimTy::F32) => cx.llvm.f32_type().const_float(*v).into(),
            _ => cx.llvm.f64_type().const_float(*v).into(),
        },
        ConstKind::Bool(b) => cx.llvm.bool_type().const_int(*b as u64, false).into(),
        ConstKind::Char(c) => cx.llvm.i32_type().const_int(*c as u64, false).into(),
        ConstKind::Str(sym) => str_operand(cx, *sym),
        ConstKind::FunDef(def, args, any_mode) => {
            let instance = Instance {
                def: *def,
                any_mode: *any_mode,
                args: args.clone(),
            };
            let name = mangle(mir, tcx, &instance);
            cx.functions[&name]
                .as_global_value()
                .as_pointer_value()
                .into()
        }
    }
}

fn int_llvm_type<'ctx>(
    cx: &CodegenCtx<'ctx>,
    tcx: &mut TyCtx,
    ty: crate::typeck::ty::Ty,
) -> (inkwell::types::IntType<'ctx>, bool) {
    let prim = match tcx.kind(ty) {
        TyKind::Primitive(prim) => *prim,
        other => unreachable!("ConstKind::Int has a non-integer type {other:?}"),
    };
    let is_signed = matches!(prim, PrimTy::I8 | PrimTy::I16 | PrimTy::I32 | PrimTy::I64);
    let int_ty = match prim {
        PrimTy::I8 | PrimTy::U8 => cx.llvm.i8_type(),
        PrimTy::I16 | PrimTy::U16 => cx.llvm.i16_type(),
        PrimTy::I32 | PrimTy::U32 => cx.llvm.i32_type(),
        PrimTy::I64 | PrimTy::U64 | PrimTy::Usize => cx.llvm.i64_type(),
        other => unreachable!("ConstKind::Int has a non-integer primitive {other:?}"),
    };
    (int_ty, is_signed)
}

fn str_operand<'ctx>(cx: &mut CodegenCtx<'ctx>, sym: Symbol) -> BasicValueEnum<'ctx> {
    let bytes = Interner::resolve(sym).as_bytes();
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
        let (_hir, mut tcx, _types, mir, instances) = crate::testing::lower_to_mir(
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
        let mut cx = CodegenCtx::new(&llvm, "t");
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
