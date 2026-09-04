use std::collections::HashMap;

use inkwell::intrinsics::Intrinsic;
use inkwell::types::{BasicMetadataTypeEnum, BasicType, BasicTypeEnum, FunctionType};
use inkwell::values::{
    BasicMetadataValueEnum, BasicValue, BasicValueEnum, FunctionValue, PointerValue,
};
use inkwell::{FloatPredicate, IntPredicate};

use super::ctx::CodegenCtx;
use super::{konst, layout, place, ty};
use crate::ast::{BinaryOp, UnaryOp};
use crate::hir::DefId;
use crate::langitems::LangItem;
use crate::mir::mangle::mangle;
use crate::mir::{
    AdtDef, AggregateKind, AssertMessage, Body, CastKind, ConstKind, Constant, Instance, Local,
    LocalDecl, Mir, Operand, Place, Projection, Rvalue, Statement, StatementKind, Terminator,
    TerminatorKind, VariantIdx,
};
use crate::nameres::PrimTy;
use crate::typeck::ty::{Ty, TyKind};
use crate::typeck::tyctx::TyCtx;

pub fn lower_body<'ctx>(
    cx: &mut CodegenCtx<'ctx>,
    mir: &Mir,
    tcx: &mut TyCtx,
    body: &Body,
    function: FunctionValue<'ctx>,
) {
    let entry = cx.llvm.append_basic_block(function, "entry");
    cx.builder.position_at_end(entry);

    let sret = matches!(
        ty::abi_class(tcx, body.local_decls[0].ty),
        ty::AbiClass::Indirect
    );
    let locals = alloca_locals(cx, mir, tcx, body, function, sret);
    unpack_params(cx, mir, tcx, body, function, &locals, sret);

    let blocks: Vec<_> = body
        .basic_blocks
        .iter()
        .enumerate()
        .map(|(i, _)| cx.llvm.append_basic_block(function, &format!("bb{i}")))
        .collect();
    cx.builder.build_unconditional_branch(blocks[0]).unwrap();

    for (i, bb) in body.basic_blocks.iter().enumerate() {
        cx.builder.position_at_end(blocks[i]);
        for stmt in &bb.statements {
            lower_statement(cx, mir, tcx, &locals, &body.local_decls, stmt);
        }
        lower_terminator(
            cx,
            mir,
            tcx,
            &locals,
            &body.local_decls,
            &blocks,
            &bb.terminator,
            sret,
        );
    }
}

fn alloca_locals<'ctx>(
    cx: &mut CodegenCtx<'ctx>,
    mir: &Mir,
    tcx: &mut TyCtx,
    body: &Body,
    function: FunctionValue<'ctx>,
    sret: bool,
) -> HashMap<Local, PointerValue<'ctx>> {
    let mut locals = std::collections::HashMap::new();
    let skip = if sret {
        locals.insert(
            Local::from_usize(0),
            function.get_nth_param(0).unwrap().into_pointer_value(),
        );
        1
    } else {
        0
    };
    for (i, decl) in body.local_decls.iter().enumerate().skip(skip) {
        let llvm_ty = ty::llvm_type(cx, tcx, mir, decl.ty);
        let alloca = cx.builder.build_alloca(llvm_ty, &format!("_{i}")).unwrap();
        locals.insert(Local::from_usize(i), alloca);
    }
    locals
}

fn unpack_params<'ctx>(
    cx: &mut CodegenCtx<'ctx>,
    mir: &Mir,
    tcx: &mut TyCtx,
    body: &Body,
    function: FunctionValue<'ctx>,
    locals: &HashMap<Local, PointerValue<'ctx>>,
    sret: bool,
) {
    let param_offset = sret as usize;
    for (param_idx, local_idx) in (1..=body.param_count).enumerate() {
        let decl = &body.local_decls[local_idx];
        let dest = locals[&Local::from_usize(local_idx)];
        match ty::abi_class(tcx, decl.ty) {
            ty::AbiClass::Fat => {
                let word0 = function
                    .get_nth_param((param_offset + param_idx * 2) as u32)
                    .unwrap();
                let word1 = function
                    .get_nth_param((param_offset + param_idx * 2 + 1) as u32)
                    .unwrap();
                let struct_ty = ty::llvm_type(cx, tcx, mir, decl.ty).into_struct_type();
                let field0 = cx
                    .builder
                    .build_struct_gep(struct_ty, dest, 0, "p0")
                    .unwrap();
                cx.builder.build_store(field0, word0).unwrap();
                let field1 = cx
                    .builder
                    .build_struct_gep(struct_ty, dest, 1, "p1")
                    .unwrap();
                cx.builder.build_store(field1, word1).unwrap();
            }
            ty::AbiClass::Indirect => {
                let src = function
                    .get_nth_param((param_offset + param_idx) as u32)
                    .unwrap()
                    .into_pointer_value();
                let size = layout::layout_of(tcx, mir, decl.ty).size;
                cx.builder
                    .build_memcpy(dest, 8, src, 8, cx.llvm.i64_type().const_int(size, false))
                    .unwrap();
            }
            ty::AbiClass::Scalar => {
                let param = function
                    .get_nth_param((param_offset + param_idx) as u32)
                    .unwrap();
                cx.builder.build_store(dest, param).unwrap();
            }
            ty::AbiClass::Void => {}
        }
    }
}

pub fn lower_operand<'ctx>(
    cx: &mut CodegenCtx<'ctx>,
    tcx: &mut TyCtx,
    mir: &Mir,
    locals: &HashMap<Local, PointerValue<'ctx>>,
    local_decls: &[LocalDecl],
    operand: &Operand,
) -> BasicValueEnum<'ctx> {
    match operand {
        Operand::Constant(constant) => konst::lower_constant(cx, tcx, mir, constant),
        Operand::Copy(place_) | Operand::Move(place_) => {
            let (ptr, place_ty) =
                place::lower_place(cx, tcx, mir, locals, local_decls, place_);
            let llvm_ty = ty::llvm_type(cx, tcx, mir, place_ty);
            cx.builder.build_load(llvm_ty, ptr, "load").unwrap()
        }
    }
}

fn lower_statement<'ctx>(
    cx: &mut CodegenCtx<'ctx>,
    mir: &Mir,
    tcx: &mut TyCtx,
    locals: &HashMap<Local, PointerValue<'ctx>>,
    local_decls: &[LocalDecl],
    stmt: &Statement,
) {
    match &stmt.kind {
        StatementKind::Assign(place_, rvalue) => {
            let (ptr, dest_ty) =
                place::lower_place(cx, tcx, mir, locals, local_decls, place_);
            let value = lower_rvalue(cx, mir, tcx, locals, local_decls, dest_ty, rvalue);
            cx.builder.build_store(ptr, value).unwrap();
        }
        StatementKind::SetDiscriminant {
            place: place_,
            variant,
        } => {
            let (ptr, enum_ty) =
                place::lower_place(cx, tcx, mir, locals, local_decls, place_);
            let enum_llvm_ty = ty::llvm_type(cx, tcx, mir, enum_ty).into_struct_type();
            let tag_ptr = cx
                .builder
                .build_struct_gep(enum_llvm_ty, ptr, 0, "tag")
                .unwrap();
            let tag_llvm_ty = enum_llvm_ty
                .get_field_type_at_index(0)
                .unwrap()
                .into_int_type();
            let tag_val = tag_llvm_ty.const_int(variant.index() as u64, false);
            cx.builder.build_store(tag_ptr, tag_val).unwrap();
        }
        StatementKind::StorageLive(_)
        | StatementKind::StorageDead(_)
        | StatementKind::PlaceMention(_)
        | StatementKind::CheckMutable(_)
        | StatementKind::WithLend(_) => {}
    }
}

fn lower_rvalue<'ctx>(
    cx: &mut CodegenCtx<'ctx>,
    mir: &Mir,
    tcx: &mut TyCtx,
    locals: &HashMap<Local, PointerValue<'ctx>>,
    local_decls: &[LocalDecl],
    dest_ty: Ty,
    rvalue: &Rvalue,
) -> BasicValueEnum<'ctx> {
    match rvalue {
        Rvalue::Use(operand) => lower_operand(cx, tcx, mir, locals, local_decls, operand),
        Rvalue::Ref { place: place_, .. } => {
            let ref_place_ty = place_ty(tcx, local_decls, place_);
            if layout::is_unsized(tcx, ref_place_ty) {
                lower_fat_ref(cx, tcx, locals, local_decls, place_)
            } else {
                let (ptr, _) = place::lower_place(cx, tcx, mir, locals, local_decls, place_);
                ptr.as_basic_value_enum()
            }
        }
        Rvalue::BinaryOp(op, lhs, rhs) => {
            let operand_ty = operand_ty(tcx, local_decls, lhs);
            let lhs_val = lower_operand(cx, tcx, mir, locals, local_decls, lhs);
            let rhs_val = lower_operand(cx, tcx, mir, locals, local_decls, rhs);
            lower_binary_op(cx, tcx, *op, operand_ty, lhs_val, rhs_val)
        }
        Rvalue::UnaryOp(op, operand) => {
            let operand_ty = operand_ty(tcx, local_decls, operand);
            let val = lower_operand(cx, tcx, mir, locals, local_decls, operand);
            lower_unary_op(cx, tcx, *op, operand_ty, val)
        }
        Rvalue::Aggregate(kind, operands)
            if matches!(**kind, crate::mir::AggregateKind::Tuple) && operands.is_empty() =>
        {
            cx.llvm.struct_type(&[], false).const_zero().into()
        }
        Rvalue::Aggregate(kind, operands) => lower_aggregate(
            cx,
            mir,
            tcx,
            locals,
            local_decls,
            dest_ty,
            kind,
            operands,
        ),
        Rvalue::CheckedBinaryOp(op, lhs, rhs) => {
            let operand_ty = operand_ty(tcx, local_decls, lhs);
            let lhs_val = lower_operand(cx, tcx, mir, locals, local_decls, lhs);
            let rhs_val = lower_operand(cx, tcx, mir, locals, local_decls, rhs);
            lower_checked_binary_op(cx, tcx, *op, operand_ty, lhs_val, rhs_val)
        }
        Rvalue::Cast {
            operand,
            ty: cast_ty,
            kind,
        } => {
            let val = lower_operand(cx, tcx, mir, locals, local_decls, operand);
            match kind {
                CastKind::ReifyFunPointer => val,
                CastKind::Primitive => {
                    let src_ty = operand_ty(tcx, local_decls, operand);
                    lower_primitive_cast(cx, tcx, mir, src_ty, *cast_ty, val)
                }
            }
        }
        Rvalue::Discriminant(place_) => {
            let (ptr, place_ty) =
                place::lower_place(cx, tcx, mir, locals, local_decls, place_);
            let enum_llvm_ty = ty::llvm_type(cx, tcx, mir, place_ty).into_struct_type();
            let tag_ptr = cx
                .builder
                .build_struct_gep(enum_llvm_ty, ptr, 0, "tag")
                .unwrap();
            let tag_llvm_ty = enum_llvm_ty.get_field_type_at_index(0).unwrap();
            cx.builder
                .build_load(tag_llvm_ty, tag_ptr, "discr")
                .unwrap()
        }
        Rvalue::Len(place_) => lower_len(cx, tcx, mir, locals, local_decls, place_),
        Rvalue::New(operand) => lower_new(cx, tcx, mir, locals, local_decls, operand),
        Rvalue::NewArray { elem, count } => {
            lower_new_array(cx, tcx, mir, locals, local_decls, elem, count)
        }
    }
}

fn operand_ty(tcx: &mut TyCtx, local_decls: &[LocalDecl], operand: &Operand) -> Ty {
    match operand {
        Operand::Constant(constant) => constant.ty,
        Operand::Copy(place_) | Operand::Move(place_) => place_ty(tcx, local_decls, place_),
    }
}

fn place_ty(tcx: &mut TyCtx, local_decls: &[LocalDecl], place_: &crate::mir::Place) -> Ty {
    let mut ty = local_decls[place_.local.index()].ty;
    for proj in &place_.projections {
        match proj {
            crate::mir::Projection::Deref => {
                ty = match tcx.kind(ty).clone() {
                    TyKind::Ref { base, .. } | TyKind::Iso(base) => base,
                    other => panic!("Deref projection on non-reference type {other:?}"),
                };
            }
            crate::mir::Projection::Field(index) => {
                ty = match tcx.kind(ty).clone() {
                    TyKind::Tuple(elems) => elems[*index as usize],
                    other => panic!(
                        "Field projection on non-Tuple type {other:?} in a BinaryOp/UnaryOp \
                         operand"
                    ),
                };
            }
            other => panic!(
                "unexpected projection {other:?} on a BinaryOp/UnaryOp operand -- only Deref \
                 and Tuple-Field are supported"
            ),
        }
    }
    ty
}

fn two_word_struct_type<'ctx>(cx: &CodegenCtx<'ctx>) -> inkwell::types::StructType<'ctx> {
    cx.llvm.struct_type(
        &[
            cx.llvm.ptr_type(Default::default()).into(),
            cx.llvm.i64_type().into(),
        ],
        false,
    )
}

fn lower_fat_place_addr<'ctx>(
    tcx: &mut TyCtx,
    locals: &HashMap<Local, PointerValue<'ctx>>,
    local_decls: &[LocalDecl],
    place_: &Place,
) -> PointerValue<'ctx> {
    match place_.projections.as_slice() {
        [Projection::Deref] => {
            let local_ty = local_decls[place_.local.index()].ty;
            match tcx.kind(local_ty).clone() {
                TyKind::Ref { base, .. } | TyKind::Iso(base) if layout::is_unsized(tcx, base) => {
                    locals[&place_.local]
                }
                other => panic!(
                    "lower_fat_place_addr: local's type {other:?} is not a fat reference/iso"
                ),
            }
        }
        other => panic!(
            "lower_fat_place_addr: only a bare Deref of a fat reference/iso local is supported \
             in v1 codegen, got projections {other:?}"
        ),
    }
}

fn lower_fat_ref<'ctx>(
    cx: &mut CodegenCtx<'ctx>,
    tcx: &mut TyCtx,
    locals: &HashMap<Local, PointerValue<'ctx>>,
    local_decls: &[LocalDecl],
    place_: &Place,
) -> BasicValueEnum<'ctx> {
    let fat_addr = lower_fat_place_addr(tcx, locals, local_decls, place_);
    let two_word_ty = two_word_struct_type(cx);
    let ptr_field = cx
        .builder
        .build_struct_gep(two_word_ty, fat_addr, 0, "data_ptr_slot")
        .unwrap();
    let data_ptr = cx
        .builder
        .build_load(cx.llvm.ptr_type(Default::default()), ptr_field, "data_ptr")
        .unwrap();
    let len_field = cx
        .builder
        .build_struct_gep(two_word_ty, fat_addr, 1, "len_slot")
        .unwrap();
    let len = cx
        .builder
        .build_load(cx.llvm.i64_type(), len_field, "len")
        .unwrap();
    let mut agg = two_word_ty.get_undef();
    agg = cx
        .builder
        .build_insert_value(agg, data_ptr, 0, "fat0")
        .unwrap()
        .into_struct_value();
    agg = cx
        .builder
        .build_insert_value(agg, len, 1, "fat1")
        .unwrap()
        .into_struct_value();
    agg.into()
}

fn lower_len<'ctx>(
    cx: &mut CodegenCtx<'ctx>,
    tcx: &mut TyCtx,
    mir: &Mir,
    locals: &HashMap<Local, PointerValue<'ctx>>,
    local_decls: &[LocalDecl],
    place_: &Place,
) -> BasicValueEnum<'ctx> {
    if place_.projections.is_empty()
        && let TyKind::Array {
            len: Some(len_id), ..
        } = tcx.kind(local_decls[place_.local.index()].ty).clone()
    {
        let n = layout::array_len(mir, len_id);
        return cx.llvm.i64_type().const_int(n, false).into();
    }
    let fat_addr = lower_fat_place_addr(tcx, locals, local_decls, place_);
    let two_word_ty = two_word_struct_type(cx);
    let len_field = cx
        .builder
        .build_struct_gep(two_word_ty, fat_addr, 1, "len_slot")
        .unwrap();
    cx.builder
        .build_load(cx.llvm.i64_type(), len_field, "len")
        .unwrap()
}

fn is_str_or_byte_slice_ref(tcx: &TyCtx, ty: Ty) -> bool {
    match tcx.kind(ty) {
        TyKind::Primitive(PrimTy::Str) => true,
        TyKind::Ref { base, .. } | TyKind::Iso(base) => matches!(
            tcx.kind(*base),
            TyKind::Array { elem, len: None } if matches!(tcx.kind(*elem), TyKind::Primitive(PrimTy::U8))
        ),
        _ => false,
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum NumKind {
    Float,
    Signed,
    Unsigned,
}

fn numeric_kind(tcx: &TyCtx, ty: Ty) -> NumKind {
    match tcx.kind(ty) {
        TyKind::Primitive(prim) => match prim {
            PrimTy::F32 | PrimTy::F64 => NumKind::Float,
            PrimTy::I8 | PrimTy::I16 | PrimTy::I32 | PrimTy::I64 => NumKind::Signed,
            PrimTy::U8
            | PrimTy::U16
            | PrimTy::U32
            | PrimTy::U64
            | PrimTy::Usize
            | PrimTy::Bool
            | PrimTy::Char => NumKind::Unsigned,
            PrimTy::Str => unreachable!(
                "str has no numeric representation; str<->&[u8] casts are handled by \
                 is_str_or_byte_slice_ref before numeric_kind is ever consulted"
            ),
        },
        other => unreachable!("Cast{{kind: Primitive}} operand/destination has type {other:?}"),
    }
}

fn lower_primitive_cast<'ctx>(
    cx: &mut CodegenCtx<'ctx>,
    tcx: &mut TyCtx,
    mir: &Mir,
    src_ty: Ty,
    dst_ty: Ty,
    val: BasicValueEnum<'ctx>,
) -> BasicValueEnum<'ctx> {
    if is_str_or_byte_slice_ref(tcx, src_ty) && is_str_or_byte_slice_ref(tcx, dst_ty) {
        return val;
    }
    let dst_llvm = ty::llvm_type(cx, tcx, mir, dst_ty);
    match (numeric_kind(tcx, src_ty), numeric_kind(tcx, dst_ty)) {
        (NumKind::Float, NumKind::Float) => cx
            .builder
            .build_float_cast(val.into_float_value(), dst_llvm.into_float_type(), "cast")
            .unwrap()
            .into(),
        (NumKind::Float, NumKind::Signed) => cx
            .builder
            .build_float_to_signed_int(val.into_float_value(), dst_llvm.into_int_type(), "cast")
            .unwrap()
            .into(),
        (NumKind::Float, NumKind::Unsigned) => cx
            .builder
            .build_float_to_unsigned_int(val.into_float_value(), dst_llvm.into_int_type(), "cast")
            .unwrap()
            .into(),
        (NumKind::Signed, NumKind::Float) => cx
            .builder
            .build_signed_int_to_float(val.into_int_value(), dst_llvm.into_float_type(), "cast")
            .unwrap()
            .into(),
        (NumKind::Unsigned, NumKind::Float) => cx
            .builder
            .build_unsigned_int_to_float(val.into_int_value(), dst_llvm.into_float_type(), "cast")
            .unwrap()
            .into(),
        (src_kind, _) => cx
            .builder
            .build_int_cast_sign_flag(
                val.into_int_value(),
                dst_llvm.into_int_type(),
                src_kind == NumKind::Signed,
                "cast",
            )
            .unwrap()
            .into(),
    }
}

fn lower_checked_binary_op<'ctx>(
    cx: &mut CodegenCtx<'ctx>,
    tcx: &mut TyCtx,
    op: BinaryOp,
    ty: Ty,
    lhs: BasicValueEnum<'ctx>,
    rhs: BasicValueEnum<'ctx>,
) -> BasicValueEnum<'ctx> {
    let signed = is_signed(tcx, ty);
    let name = match (op, signed) {
        (BinaryOp::Add, true) => "llvm.sadd.with.overflow",
        (BinaryOp::Add, false) => "llvm.uadd.with.overflow",
        (BinaryOp::Sub, true) => "llvm.ssub.with.overflow",
        (BinaryOp::Sub, false) => "llvm.usub.with.overflow",
        (BinaryOp::Mul, true) => "llvm.smul.with.overflow",
        (BinaryOp::Mul, false) => "llvm.umul.with.overflow",
        _ => unreachable!("CheckedBinaryOp only ever carries Add/Sub/Mul, got {op:?}"),
    };
    let int_ty = lhs.into_int_value().get_type();
    let intrinsic =
        Intrinsic::find(name).unwrap_or_else(|| panic!("no such LLVM intrinsic {name}"));
    let function = intrinsic
        .get_declaration(&cx.module, &[int_ty.into()])
        .unwrap_or_else(|| panic!("failed to declare intrinsic {name}"));
    let call = cx
        .builder
        .build_call(function, &[lhs.into(), rhs.into()], "checked")
        .unwrap();
    call.try_as_basic_value().unwrap_basic()
}

fn lower_aggregate<'ctx>(
    cx: &mut CodegenCtx<'ctx>,
    mir: &Mir,
    tcx: &mut TyCtx,
    locals: &HashMap<Local, PointerValue<'ctx>>,
    local_decls: &[LocalDecl],
    dest_ty: Ty,
    kind: &AggregateKind,
    operands: &[Operand],
) -> BasicValueEnum<'ctx> {
    match kind {
        AggregateKind::Tuple | AggregateKind::Array => {
            let agg_llvm_ty = ty::llvm_type(cx, tcx, mir, dest_ty);
            let mut agg: BasicValueEnum<'ctx> = match agg_llvm_ty {
                BasicTypeEnum::StructType(t) => t.get_undef().into(),
                BasicTypeEnum::ArrayType(t) => t.get_undef().into(),
                other => {
                    unreachable!("Tuple/Array aggregate has non-aggregate LLVM type {other:?}")
                }
            };
            for (i, operand) in operands.iter().enumerate() {
                let val = lower_operand(cx, tcx, mir, locals, local_decls, operand);
                agg = insert_aggregate_field(cx, agg, val, i as u32);
            }
            agg
        }
        AggregateKind::Adt { def, variant } => lower_adt_aggregate(
            cx,
            mir,
            tcx,
            locals,
            local_decls,
            dest_ty,
            *def,
            *variant,
            operands,
        ),
        AggregateKind::Closure { .. } => todo!("closures: unspecified, deferred past this plan"),
    }
}

fn insert_aggregate_field<'ctx>(
    cx: &mut CodegenCtx<'ctx>,
    agg: BasicValueEnum<'ctx>,
    val: BasicValueEnum<'ctx>,
    index: u32,
) -> BasicValueEnum<'ctx> {
    match agg {
        BasicValueEnum::StructValue(s) => cx
            .builder
            .build_insert_value(s, val, index, "field")
            .unwrap()
            .into_struct_value()
            .into(),
        BasicValueEnum::ArrayValue(a) => cx
            .builder
            .build_insert_value(a, val, index, "elem")
            .unwrap()
            .into_array_value()
            .into(),
        other => unreachable!("Tuple/Array aggregate produced a non-aggregate value {other:?}"),
    }
}

fn lower_adt_aggregate<'ctx>(
    cx: &mut CodegenCtx<'ctx>,
    mir: &Mir,
    tcx: &mut TyCtx,
    locals: &HashMap<Local, PointerValue<'ctx>>,
    local_decls: &[LocalDecl],
    dest_ty: Ty,
    def: DefId,
    variant: VariantIdx,
    operands: &[Operand],
) -> BasicValueEnum<'ctx> {
    match &mir.adts[&def] {
        AdtDef::Struct { .. } => {
            let struct_llvm_ty = ty::llvm_type(cx, tcx, mir, dest_ty).into_struct_type();
            let mut agg = struct_llvm_ty.get_undef();
            for (i, operand) in operands.iter().enumerate() {
                let val = lower_operand(cx, tcx, mir, locals, local_decls, operand);
                agg = cx
                    .builder
                    .build_insert_value(agg, val, i as u32, "field")
                    .unwrap()
                    .into_struct_value();
            }
            agg.into()
        }
        AdtDef::Enum { .. } => {
            let enum_layout = layout::layout_of(tcx, mir, dest_ty);
            let tag_llvm_ty = match enum_layout
                .tag_ty
                .expect("an enum's own layout always has a tag")
            {
                layout::TagTy::I8 => cx.llvm.i8_type(),
                layout::TagTy::I16 => cx.llvm.i16_type(),
                layout::TagTy::I32 => cx.llvm.i32_type(),
            };
            let tag_size = tag_llvm_ty.get_bit_width() as u64 / 8;
            let pad_bytes = enum_layout.payload_offset - tag_size;
            let tag_val = tag_llvm_ty.const_int(variant.index() as u64, false);

            let field_vals: Vec<BasicValueEnum<'ctx>> = operands
                .iter()
                .map(|op| lower_operand(cx, tcx, mir, locals, local_decls, op))
                .collect();
            let field_types: Vec<BasicTypeEnum<'ctx>> =
                field_vals.iter().map(BasicValueEnum::get_type).collect();
            let payload_ty = cx.llvm.struct_type(&field_types, false);
            let mut payload = payload_ty.get_undef();
            for (i, val) in field_vals.into_iter().enumerate() {
                payload = cx
                    .builder
                    .build_insert_value(payload, val, i as u32, "payload_field")
                    .unwrap()
                    .into_struct_value();
            }

            let pad_ty = cx.llvm.i8_type().array_type(pad_bytes as u32);
            let outer_ty = cx.llvm.struct_type(
                &[tag_llvm_ty.into(), pad_ty.into(), payload_ty.into()],
                false,
            );
            let mut outer = outer_ty.get_undef();
            outer = cx
                .builder
                .build_insert_value(outer, tag_val, 0, "tag")
                .unwrap()
                .into_struct_value();
            outer = cx
                .builder
                .build_insert_value(outer, payload, 2, "payload")
                .unwrap()
                .into_struct_value();
            outer.into()
        }
    }
}

fn lower_new<'ctx>(
    cx: &mut CodegenCtx<'ctx>,
    tcx: &mut TyCtx,
    mir: &Mir,
    locals: &HashMap<Local, PointerValue<'ctx>>,
    local_decls: &[LocalDecl],
    operand: &Operand,
) -> BasicValueEnum<'ctx> {
    let elem_ty = operand_ty(tcx, local_decls, operand);
    let val = lower_operand(cx, tcx, mir, locals, local_decls, operand);
    let elem_llvm_ty = ty::llvm_type(cx, tcx, mir, elem_ty);
    let size = elem_llvm_ty
        .size_of()
        .expect("a value's own type is always sized");
    let call = cx
        .builder
        .build_call(cx.libc.malloc, &[size.into()], "new")
        .unwrap();
    let ptr = call
        .try_as_basic_value()
        .unwrap_basic()
        .into_pointer_value();
    cx.builder.build_store(ptr, val).unwrap();
    ptr.as_basic_value_enum()
}

fn lower_new_array<'ctx>(
    cx: &mut CodegenCtx<'ctx>,
    tcx: &mut TyCtx,
    mir: &Mir,
    locals: &HashMap<Local, PointerValue<'ctx>>,
    local_decls: &[LocalDecl],
    elem: &Operand,
    count: &Operand,
) -> BasicValueEnum<'ctx> {
    let elem_ty = operand_ty(tcx, local_decls, elem);
    let elem_val = lower_operand(cx, tcx, mir, locals, local_decls, elem);
    let elem_llvm_ty = ty::llvm_type(cx, tcx, mir, elem_ty);
    let elem_size = elem_llvm_ty
        .size_of()
        .expect("a value's own type is always sized");
    let count_val = lower_operand(cx, tcx, mir, locals, local_decls, count).into_int_value();
    let total_size = cx
        .builder
        .build_int_mul(elem_size, count_val, "size")
        .unwrap();

    let call = cx
        .builder
        .build_call(cx.libc.malloc, &[total_size.into()], "newarray")
        .unwrap();
    let data_ptr = call
        .try_as_basic_value()
        .unwrap_basic()
        .into_pointer_value();

    build_fill_loop(cx, data_ptr, elem_llvm_ty, count_val, elem_val);

    let two_word_ty = two_word_struct_type(cx);
    let mut agg = two_word_ty.get_undef();
    agg = cx
        .builder
        .build_insert_value(agg, data_ptr, 0, "fat0")
        .unwrap()
        .into_struct_value();
    agg = cx
        .builder
        .build_insert_value(agg, count_val, 1, "fat1")
        .unwrap()
        .into_struct_value();
    agg.into()
}

fn build_fill_loop<'ctx>(
    cx: &mut CodegenCtx<'ctx>,
    data_ptr: PointerValue<'ctx>,
    elem_llvm_ty: BasicTypeEnum<'ctx>,
    count: inkwell::values::IntValue<'ctx>,
    elem_val: BasicValueEnum<'ctx>,
) {
    let function = cx.builder.get_insert_block().unwrap().get_parent().unwrap();
    let header = cx.llvm.append_basic_block(function, "newarray.header");
    let body = cx.llvm.append_basic_block(function, "newarray.body");
    let exit = cx.llvm.append_basic_block(function, "newarray.exit");

    let i64_ty = cx.llvm.i64_type();
    let index_alloca = cx.builder.build_alloca(i64_ty, "newarray.i").unwrap();
    cx.builder
        .build_store(index_alloca, i64_ty.const_zero())
        .unwrap();
    cx.builder.build_unconditional_branch(header).unwrap();

    cx.builder.position_at_end(header);
    let i = cx
        .builder
        .build_load(i64_ty, index_alloca, "i")
        .unwrap()
        .into_int_value();
    let cond = cx
        .builder
        .build_int_compare(IntPredicate::ULT, i, count, "cond")
        .unwrap();
    cx.builder
        .build_conditional_branch(cond, body, exit)
        .unwrap();

    cx.builder.position_at_end(body);
    let elem_ptr = unsafe {
        cx.builder
            .build_gep(elem_llvm_ty, data_ptr, &[i], "elem_ptr")
            .unwrap()
    };
    cx.builder.build_store(elem_ptr, elem_val).unwrap();
    let next = cx
        .builder
        .build_int_add(i, i64_ty.const_int(1, false), "next")
        .unwrap();
    cx.builder.build_store(index_alloca, next).unwrap();
    cx.builder.build_unconditional_branch(header).unwrap();

    cx.builder.position_at_end(exit);
}

fn is_float(tcx: &TyCtx, ty: Ty) -> bool {
    matches!(
        tcx.kind(ty),
        TyKind::Primitive(PrimTy::F32) | TyKind::Primitive(PrimTy::F64)
    )
}

fn is_signed(tcx: &TyCtx, ty: Ty) -> bool {
    match tcx.kind(ty) {
        TyKind::Primitive(prim) => match prim {
            PrimTy::I8 | PrimTy::I16 | PrimTy::I32 | PrimTy::I64 => true,
            PrimTy::U8 | PrimTy::U16 | PrimTy::U32 | PrimTy::U64 | PrimTy::Usize => false,
            other => unreachable!("BinaryOp/UnaryOp operand has non-arithmetic type {other:?}"),
        },
        other => unreachable!("BinaryOp/UnaryOp operand has non-primitive type {other:?}"),
    }
}

fn lower_binary_op<'ctx>(
    cx: &mut CodegenCtx<'ctx>,
    tcx: &mut TyCtx,
    op: BinaryOp,
    ty: Ty,
    lhs: BasicValueEnum<'ctx>,
    rhs: BasicValueEnum<'ctx>,
) -> BasicValueEnum<'ctx> {
    let b = &cx.builder;
    if is_float(tcx, ty) {
        let (l, r) = (lhs.into_float_value(), rhs.into_float_value());
        match op {
            BinaryOp::Add => b.build_float_add(l, r, "add").unwrap().into(),
            BinaryOp::Sub => b.build_float_sub(l, r, "sub").unwrap().into(),
            BinaryOp::Mul => b.build_float_mul(l, r, "mul").unwrap().into(),
            BinaryOp::Div => b.build_float_div(l, r, "div").unwrap().into(),
            BinaryOp::Rem => b.build_float_rem(l, r, "rem").unwrap().into(),
            BinaryOp::Eq => b
                .build_float_compare(FloatPredicate::OEQ, l, r, "eq")
                .unwrap()
                .into(),
            BinaryOp::Ne => b
                .build_float_compare(FloatPredicate::ONE, l, r, "ne")
                .unwrap()
                .into(),
            BinaryOp::Lt => b
                .build_float_compare(FloatPredicate::OLT, l, r, "lt")
                .unwrap()
                .into(),
            BinaryOp::Le => b
                .build_float_compare(FloatPredicate::OLE, l, r, "le")
                .unwrap()
                .into(),
            BinaryOp::Gt => b
                .build_float_compare(FloatPredicate::OGT, l, r, "gt")
                .unwrap()
                .into(),
            BinaryOp::Ge => b
                .build_float_compare(FloatPredicate::OGE, l, r, "ge")
                .unwrap()
                .into(),
            BinaryOp::And | BinaryOp::Or => {
                unreachable!("typeck rejects logical And/Or on a float operand")
            }
        }
    } else {
        let signed = is_signed(tcx, ty);
        let (l, r) = (lhs.into_int_value(), rhs.into_int_value());
        match op {
            BinaryOp::Add => b.build_int_add(l, r, "add").unwrap().into(),
            BinaryOp::Sub => b.build_int_sub(l, r, "sub").unwrap().into(),
            BinaryOp::Mul => b.build_int_mul(l, r, "mul").unwrap().into(),
            BinaryOp::Div if signed => b.build_int_signed_div(l, r, "div").unwrap().into(),
            BinaryOp::Div => b.build_int_unsigned_div(l, r, "div").unwrap().into(),
            BinaryOp::Rem if signed => b.build_int_signed_rem(l, r, "rem").unwrap().into(),
            BinaryOp::Rem => b.build_int_unsigned_rem(l, r, "rem").unwrap().into(),
            BinaryOp::Eq => b
                .build_int_compare(IntPredicate::EQ, l, r, "eq")
                .unwrap()
                .into(),
            BinaryOp::Ne => b
                .build_int_compare(IntPredicate::NE, l, r, "ne")
                .unwrap()
                .into(),
            BinaryOp::Lt if signed => b
                .build_int_compare(IntPredicate::SLT, l, r, "lt")
                .unwrap()
                .into(),
            BinaryOp::Lt => b
                .build_int_compare(IntPredicate::ULT, l, r, "lt")
                .unwrap()
                .into(),
            BinaryOp::Le if signed => b
                .build_int_compare(IntPredicate::SLE, l, r, "le")
                .unwrap()
                .into(),
            BinaryOp::Le => b
                .build_int_compare(IntPredicate::ULE, l, r, "le")
                .unwrap()
                .into(),
            BinaryOp::Gt if signed => b
                .build_int_compare(IntPredicate::SGT, l, r, "gt")
                .unwrap()
                .into(),
            BinaryOp::Gt => b
                .build_int_compare(IntPredicate::UGT, l, r, "gt")
                .unwrap()
                .into(),
            BinaryOp::Ge if signed => b
                .build_int_compare(IntPredicate::SGE, l, r, "ge")
                .unwrap()
                .into(),
            BinaryOp::Ge => b
                .build_int_compare(IntPredicate::UGE, l, r, "ge")
                .unwrap()
                .into(),
            BinaryOp::And => b.build_and(l, r, "and").unwrap().into(),
            BinaryOp::Or => b.build_or(l, r, "or").unwrap().into(),
        }
    }
}

fn lower_unary_op<'ctx>(
    cx: &mut CodegenCtx<'ctx>,
    tcx: &mut TyCtx,
    op: UnaryOp,
    ty: Ty,
    val: BasicValueEnum<'ctx>,
) -> BasicValueEnum<'ctx> {
    match op {
        UnaryOp::Neg if is_float(tcx, ty) => cx
            .builder
            .build_float_neg(val.into_float_value(), "neg")
            .unwrap()
            .into(),
        UnaryOp::Neg => cx
            .builder
            .build_int_neg(val.into_int_value(), "neg")
            .unwrap()
            .into(),
        UnaryOp::Not => cx
            .builder
            .build_not(val.into_int_value(), "not")
            .unwrap()
            .into(),
        UnaryOp::Deref => unreachable!("Rvalue::UnaryOp never carries Deref; see Place::Deref"),
    }
}

fn lower_terminator<'ctx>(
    cx: &mut CodegenCtx<'ctx>,
    mir: &Mir,
    tcx: &mut TyCtx,
    locals: &HashMap<Local, PointerValue<'ctx>>,
    local_decls: &[LocalDecl],
    blocks: &[inkwell::basic_block::BasicBlock<'ctx>],
    terminator: &Terminator,
    sret: bool,
) {
    match &terminator.kind {
        TerminatorKind::Goto { target } => {
            cx.builder
                .build_unconditional_branch(blocks[target.index()])
                .unwrap();
        }
        TerminatorKind::Return => {
            if sret || matches!(ty::abi_class(tcx, local_decls[0].ty), ty::AbiClass::Void) {
                cx.builder.build_return(None).unwrap();
            } else {
                let llvm_ty = ty::llvm_type(cx, tcx, mir, local_decls[0].ty);
                let val = cx
                    .builder
                    .build_load(llvm_ty, locals[&Local::RETURN_PLACE], "ret")
                    .unwrap();
                cx.builder.build_return(Some(&val)).unwrap();
            }
        }
        TerminatorKind::SwitchInt { discr, targets } => {
            let discr_val =
                lower_operand(cx, tcx, mir, locals, local_decls, discr).into_int_value();
            let int_ty = discr_val.get_type();
            let cases: Vec<_> = targets
                .values
                .iter()
                .map(|&(v, target)| (int_ty.const_int(v as u64, false), blocks[target.index()]))
                .collect();
            cx.builder
                .build_switch(discr_val, blocks[targets.otherwise.index()], &cases)
                .unwrap();
        }
        TerminatorKind::Call {
            func,
            args,
            destination,
            target,
        } => {
            lower_call(
                cx,
                mir,
                tcx,
                locals,
                local_decls,
                blocks,
                func,
                args,
                destination,
                *target,
            );
        }
        TerminatorKind::Assert {
            cond,
            expected,
            msg,
            target,
        } => {
            let cond_val =
                lower_operand(cx, tcx, mir, locals, local_decls, cond).into_int_value();
            let function = cx.builder.get_insert_block().unwrap().get_parent().unwrap();
            let ok_block = blocks[target.index()];
            let fail_block = cx.llvm.append_basic_block(function, "assert.fail");
            let (then_block, else_block) = if *expected {
                (ok_block, fail_block)
            } else {
                (fail_block, ok_block)
            };
            cx.builder
                .build_conditional_branch(cond_val, then_block, else_block)
                .unwrap();

            cx.builder.position_at_end(fail_block);
            lower_assert_failure(cx, msg);
        }
        TerminatorKind::Unreachable => {
            cx.builder.build_unreachable().unwrap();
        }
        TerminatorKind::Drop { place, target } => {
            let (place_ptr, place_ty) =
                place::lower_place(cx, tcx, mir, locals, local_decls, place);
            super::drop::drop_glue(cx, tcx, place_ptr, place_ty);
            cx.builder
                .build_unconditional_branch(blocks[target.index()])
                .unwrap();
        }
    }
}

fn lower_call<'ctx>(
    cx: &mut CodegenCtx<'ctx>,
    mir: &Mir,
    tcx: &mut TyCtx,
    locals: &HashMap<Local, PointerValue<'ctx>>,
    local_decls: &[LocalDecl],
    blocks: &[inkwell::basic_block::BasicBlock<'ctx>],
    func: &Operand,
    args: &[Operand],
    destination: &Place,
    target: Option<crate::mir::BasicBlock>,
) {
    let (dest_ptr, dest_ty) =
        place::lower_place(cx, tcx, mir, locals, local_decls, destination);
    let indirect_return = matches!(ty::abi_class(tcx, dest_ty), ty::AbiClass::Indirect);

    let mut arg_vals: Vec<BasicMetadataValueEnum<'ctx>> = Vec::new();
    if indirect_return {
        arg_vals.push(dest_ptr.into());
    }
    for arg in args {
        let arg_ty = operand_ty(tcx, local_decls, arg);
        push_call_arg(
            cx,
            mir,
            tcx,
            locals,
            local_decls,
            arg,
            arg_ty,
            &mut arg_vals,
        );
    }

    let call_site = if let Operand::Constant(Constant {
        kind: ConstKind::FunDef(def, targs, any_mode),
        ..
    }) = func
    {
        if Some(*def) == mir.lang_items.get(LangItem::WriteBytes) {
            cx.builder
                .build_call(cx.libc.write, &arg_vals, "call")
                .unwrap()
        } else {
            let instance = Instance {
                def: *def,
                any_mode: *any_mode,
                args: targs.clone(),
            };
            let name = mangle(mir, tcx, &instance);
            let function = cx.functions[&name];
            cx.builder.build_call(function, &arg_vals, "call").unwrap()
        }
    } else {
        let fn_ptr =
            lower_operand(cx, tcx, mir, locals, local_decls, func).into_pointer_value();
        let func_ty = operand_ty(tcx, local_decls, func);
        let fn_type = indirect_fn_type(cx, tcx, mir, func_ty);
        cx.builder
            .build_indirect_call(fn_type, fn_ptr, &arg_vals, "call")
            .unwrap()
    };

    if !indirect_return && let Some(ret_val) = call_site.try_as_basic_value().basic() {
        cx.builder.build_store(dest_ptr, ret_val).unwrap();
    }

    match target {
        Some(t) => {
            cx.builder
                .build_unconditional_branch(blocks[t.index()])
                .unwrap();
        }
        None => {
            cx.builder.build_unreachable().unwrap();
        }
    }
}

fn push_call_arg<'ctx>(
    cx: &mut CodegenCtx<'ctx>,
    mir: &Mir,
    tcx: &mut TyCtx,
    locals: &HashMap<Local, PointerValue<'ctx>>,
    local_decls: &[LocalDecl],
    operand: &Operand,
    arg_ty: Ty,
    out: &mut Vec<BasicMetadataValueEnum<'ctx>>,
) {
    match ty::abi_class(tcx, arg_ty) {
        ty::AbiClass::Void => {}
        ty::AbiClass::Fat => {
            let val = lower_operand(cx, tcx, mir, locals, local_decls, operand)
                .into_struct_value();
            let word0 = cx.builder.build_extract_value(val, 0, "arg0").unwrap();
            let word1 = cx.builder.build_extract_value(val, 1, "arg1").unwrap();
            out.push(word0.into());
            out.push(word1.into());
        }
        ty::AbiClass::Indirect => match operand {
            Operand::Copy(place_) | Operand::Move(place_) => {
                let (ptr, _) = place::lower_place(cx, tcx, mir, locals, local_decls, place_);
                out.push(ptr.into());
            }
            Operand::Constant(_) => panic!(
                "an Indirect-classed Call argument is always a place in current MIR lowering, \
                 never a bare Constant"
            ),
        },
        ty::AbiClass::Scalar => {
            let val = lower_operand(cx, tcx, mir, locals, local_decls, operand);
            out.push(val.into());
        }
    }
}

fn indirect_fn_type<'ctx>(
    cx: &CodegenCtx<'ctx>,
    tcx: &mut TyCtx,
    mir: &Mir,
    fn_ty: Ty,
) -> FunctionType<'ctx> {
    let (params, ret) = match tcx.kind(fn_ty).clone() {
        TyKind::Fun { params, ret } => (params, ret),
        other => panic!("indirect call target has non-function type {other:?}"),
    };
    let ret_ty = ret.unwrap_or_else(|| tcx.unit());

    let mut param_llvm: Vec<BasicMetadataTypeEnum<'ctx>> = Vec::new();
    for &param in &params {
        push_call_param_type(cx, tcx, mir, param, &mut param_llvm);
    }

    match ty::abi_class(tcx, ret_ty) {
        ty::AbiClass::Void => cx.llvm.void_type().fn_type(&param_llvm, false),
        ty::AbiClass::Indirect => {
            let mut params_llvm = vec![cx.llvm.ptr_type(Default::default()).into()];
            params_llvm.extend(param_llvm);
            cx.llvm.void_type().fn_type(&params_llvm, false)
        }
        _ => ty::llvm_type(cx, tcx, mir, ret_ty).fn_type(&param_llvm, false),
    }
}

fn push_call_param_type<'ctx>(
    cx: &CodegenCtx<'ctx>,
    tcx: &mut TyCtx,
    mir: &Mir,
    ty: Ty,
    out: &mut Vec<BasicMetadataTypeEnum<'ctx>>,
) {
    match ty::abi_class(tcx, ty) {
        ty::AbiClass::Void => {}
        ty::AbiClass::Fat => {
            out.push(cx.llvm.ptr_type(Default::default()).into());
            out.push(cx.llvm.i64_type().into());
        }
        ty::AbiClass::Indirect => out.push(cx.llvm.ptr_type(Default::default()).into()),
        ty::AbiClass::Scalar => out.push(ty::llvm_type(cx, tcx, mir, ty).into()),
    }
}

fn lower_assert_failure<'ctx>(cx: &mut CodegenCtx<'ctx>, msg: &AssertMessage) {
    let text = assert_message_text(msg);
    let bytes = text.as_bytes();
    let global = cx.module.add_global(
        cx.llvm.i8_type().array_type(bytes.len() as u32),
        None,
        "assert.msg",
    );
    global.set_initializer(&cx.llvm.const_string(bytes, false));
    global.set_linkage(inkwell::module::Linkage::Private);
    global.set_constant(true);
    let ptr = global.as_pointer_value();

    let fd = cx.llvm.i32_type().const_int(2, false);
    let len = cx.llvm.i64_type().const_int(bytes.len() as u64, false);
    cx.builder
        .build_call(cx.libc.write, &[fd.into(), ptr.into(), len.into()], "write")
        .unwrap();

    cx.builder.build_call(cx.libc.abort, &[], "abort").unwrap();
    cx.builder.build_unreachable().unwrap();
}

fn assert_message_text(msg: &AssertMessage) -> &'static str {
    match msg {
        AssertMessage::Overflow(..) => "arithmetic overflow\n",
        AssertMessage::DivisionByZero(_) => "attempt to divide by zero\n",
        AssertMessage::RemainderByZero(_) => {
            "attempt to calculate the remainder with a divisor of zero\n"
        }
        AssertMessage::BoundsCheck { .. } => "index out of bounds\n",
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn empty_function_returns_unit_as_ret_void() {
        let (_hir, mut tcx, _types, mir, instances) = crate::testing::lower_to_mir("fun f() {}");
        let llvm = inkwell::context::Context::create();
        let module = super::super::codegen(&llvm, &mut tcx, &mir, &instances, "t").unwrap();
        assert!(
            module.verify().is_ok(),
            "{}",
            module.print_to_string().to_string()
        );
        let ir = module.print_to_string().to_string();
        assert!(ir.contains("ret void"), "{ir}");
    }

    #[test]
    fn arithmetic_lowers_to_int_add() {
        let (_hir, mut tcx, _types, mir, instances) =
            lower_mir_src_release("fun f(a: i32, b: i32) -> i32 { a + b }");
        let llvm = inkwell::context::Context::create();
        let module = super::super::codegen(&llvm, &mut tcx, &mir, &instances, "t").unwrap();
        module.verify().unwrap();
        let ir = module.print_to_string().to_string();
        assert!(ir.contains("add i32"), "{ir}");
    }

    fn lower_mir_src_release(
        src: &str,
    ) -> (
        crate::hir::Hir,
        crate::typeck::tyctx::TyCtx,
        crate::typeck::results::TypeResolutions,
        crate::mir::Mir,
        std::collections::HashMap<crate::mir::Instance, crate::mir::Body>,
    ) {
        crate::diagnostics::DiagCtx::clear();
        crate::ast::interner::Interner::clear();
        let files: Vec<crate::ast::ParsedSrcFile> = [crate::testing::OPS_PREAMBLE, src]
            .iter()
            .map(|src| {
                let chars: Vec<char> = src.chars().collect();
                let offset = crate::driver::source::SrcMap::add_file(
                    "<test>".to_string(),
                    chars.clone(),
                    crate::driver::source::FileOrigin::User,
                );
                let tokens = crate::lexer::Lexer::new(&chars, offset).tokenize();
                crate::parser::Parser::new().parse(&tokens, offset)
            })
            .collect();
        let ast = crate::ast::Ast::new(files);
        let res = crate::nameres::resolve(&ast);
        let hir = crate::hir::lower::lower_ast(&ast, &res);
        crate::diagnostics::DiagCtx::clear();
        let checked = crate::typeck::check(&hir);
        let diagnostics = crate::diagnostics::DiagCtx::diagnostics();
        assert!(
            diagnostics.is_empty(),
            "unexpected diagnostics for {src:?}: {diagnostics:?}"
        );
        let crate::typeck::TypeckOutput { mut tcx, types } = checked;
        let program =
            crate::mir::lower::lower(&hir, &mut tcx, &types, crate::driver::cli::Mode::Release);
        let instances = crate::mir::monomorphize::monomorphize(&hir, &mut tcx, &program);
        (hir, tcx, types, program, instances)
    }

    #[test]
    fn goto_terminator_branches_forward_to_a_block_created_up_front() {
        let (hir, mut tcx, _types, mir, instances) = crate::testing::lower_to_mir("fun f() {}");
        let def_id = instances.values().next().unwrap().def_id;
        let unit = tcx.unit();
        let dummy_span = crate::driver::source::SrcSpan::new(0, 0);
        let body = crate::mir::Body {
            def_id,
            local_decls: vec![crate::mir::LocalDecl {
                ty: unit,
                mutability: crate::ast::Mutability::Immutable,
                name: None,
                span: dummy_span,
            }],
            param_count: 0,
            basic_blocks: vec![
                crate::mir::BasicBlockData {
                    statements: vec![],
                    terminator: crate::mir::Terminator {
                        kind: crate::mir::TerminatorKind::Goto {
                            target: crate::mir::BasicBlock::from_usize(1),
                        },
                        span: dummy_span,
                    },
                },
                crate::mir::BasicBlockData {
                    statements: vec![],
                    terminator: crate::mir::Terminator {
                        kind: crate::mir::TerminatorKind::Return,
                        span: dummy_span,
                    },
                },
            ],
            span: dummy_span,
        };

        let llvm = inkwell::context::Context::create();
        let mut cx = super::super::ctx::CodegenCtx::new(&llvm, "t");
        let fn_ty = cx.llvm.void_type().fn_type(&[], false);
        let function = cx.module.add_function("f", fn_ty, None);
        super::lower_body(&mut cx, &mir, &mut tcx, &body, function);

        assert!(
            cx.module.verify().is_ok(),
            "{}",
            cx.module.print_to_string().to_string()
        );
        let ir = cx.module.print_to_string().to_string();
        assert!(ir.contains("br label %bb1"), "{ir}");
        assert!(ir.contains("ret void"), "{ir}");
    }

    #[test]
    fn overflow_checked_add_yields_pair() {
        let (hir, mut tcx, _types, mir, instances) = crate::testing::lower_mir_src_files(&[
            crate::testing::OPS_PREAMBLE,
            "fun f(a: i32, b: i32) -> i32 { a + b }",
        ]);
        let llvm = inkwell::context::Context::create();
        let module = super::super::codegen(&llvm, &mut tcx, &mir, &instances, "t").unwrap();
        module.verify().unwrap();
        assert!(
            module
                .print_to_string()
                .to_string()
                .contains("llvm.sadd.with.overflow")
        );
    }

    #[test]
    fn switch_int_lowers_to_switch_instruction() {
        let (hir, mut tcx, _types, mir, instances) =
            crate::testing::lower_to_mir("fun f(x: bool) -> i32 { if x { 1 } else { 2 } }");
        let llvm = inkwell::context::Context::create();
        let module = super::super::codegen(&llvm, &mut tcx, &mir, &instances, "t").unwrap();
        module.verify().unwrap();
        assert!(module.print_to_string().to_string().contains("switch"));
    }

    #[test]
    fn direct_call_with_a_target_lowers_to_a_call_and_branch() {
        let (hir, mut tcx, _types, mir, instances) = crate::testing::lower_to_mir(
            "fun f() -> i32 { return g(); }\nfun g() -> i32 { return 1; }",
        );
        let llvm = inkwell::context::Context::create();
        let module = super::super::codegen(&llvm, &mut tcx, &mir, &instances, "t").unwrap();
        module.verify().unwrap();
        let ir = module.print_to_string().to_string();
        assert!(ir.contains("call i32"), "{ir}");
    }

    #[test]
    fn a_reified_function_pointer_resolves_to_the_functions_own_global() {
        let (hir, mut tcx, _types, mir, instances) = crate::testing::lower_mir_src_files(&[
            crate::testing::OPS_PREAMBLE,
            "fun f() -> fun(i32, i32) -> i32 { return add; }
             fun add(x: i32, y: i32) -> i32 { return x + y; }",
        ]);
        let llvm = inkwell::context::Context::create();
        let module = super::super::codegen(&llvm, &mut tcx, &mir, &instances, "t").unwrap();
        module.verify().unwrap();
        let ir = module.print_to_string().to_string();
        assert!(ir.contains("store ptr @crate_add"), "{ir}");
        assert!(!ir.contains("inttoptr") && !ir.contains("bitcast"), "{ir}");
    }

    #[test]
    fn struct_literal_builds_via_insert_value() {
        let (hir, mut tcx, _types, mir, instances) = crate::testing::lower_to_mir(
            "struct Point { public x: i32, public y: i32 }
             fun f(a: i32, b: i32) -> Point { return Point { x: a, y: b }; }",
        );
        let llvm = inkwell::context::Context::create();
        let module = super::super::codegen(&llvm, &mut tcx, &mir, &instances, "t").unwrap();
        module.verify().unwrap();
        let ir = module.print_to_string().to_string();
        assert!(ir.contains("insertvalue"), "{ir}");
    }

    #[test]
    fn enum_variant_construction_stores_its_own_tag_and_payload() {
        let (hir, mut tcx, _types, mir, instances) = crate::testing::lower_to_mir(
            "enum E { A, B: i32 }
             fun f(n: i32) -> i32 {
                 let e: E = .B(n);
                 return match e { .A => 0, .B(x) => x };
             }",
        );
        let llvm = inkwell::context::Context::create();
        let module = super::super::codegen(&llvm, &mut tcx, &mir, &instances, "t").unwrap();
        assert!(
            module.verify().is_ok(),
            "{}",
            module.print_to_string().to_string()
        );
        let ir = module.print_to_string().to_string();
        assert!(ir.contains("insertvalue"), "{ir}");
        assert!(ir.contains("switch"), "{ir}");
    }

    #[test]
    fn tuple_literal_builds_via_insert_value() {
        let (hir, mut tcx, _types, mir, instances) =
            crate::testing::lower_to_mir("fun f(a: i32, b: i32) -> (i32, i32) { return (a, b); }");
        let llvm = inkwell::context::Context::create();
        let module = super::super::codegen(&llvm, &mut tcx, &mir, &instances, "t").unwrap();
        module.verify().unwrap();
        let ir = module.print_to_string().to_string();
        assert!(ir.contains("insertvalue"), "{ir}");
    }

    #[test]
    fn widening_int_cast_sign_extends() {
        let (hir, mut tcx, _types, mir, instances) =
            crate::testing::lower_to_mir("fun f() -> i64 { let x: i8 = 1; return x as i64; }");
        let llvm = inkwell::context::Context::create();
        let module = super::super::codegen(&llvm, &mut tcx, &mir, &instances, "t").unwrap();
        module.verify().unwrap();
        let ir = module.print_to_string().to_string();
        assert!(ir.contains("sext"), "{ir}");
    }

    #[test]
    fn int_to_float_cast_converts() {
        let (hir, mut tcx, _types, mir, instances) =
            crate::testing::lower_to_mir("fun f() -> f64 { let x: i32 = 1; return x as f64; }");
        let llvm = inkwell::context::Context::create();
        let module = super::super::codegen(&llvm, &mut tcx, &mir, &instances, "t").unwrap();
        module.verify().unwrap();
        let ir = module.print_to_string().to_string();
        assert!(ir.contains("sitofp"), "{ir}");
    }

    #[test]
    fn mixed_signedness_widening_cast_zero_extends_the_unsigned_source() {
        let (hir, mut tcx, _types, mir, instances) =
            crate::testing::lower_to_mir("fun f(x: u8) -> i16 { return x as i16; }");
        let llvm = inkwell::context::Context::create();
        let module = super::super::codegen(&llvm, &mut tcx, &mir, &instances, "t").unwrap();
        module.verify().unwrap();
        let ir = module.print_to_string().to_string();
        assert!(ir.contains("zext"), "{ir}");
        assert!(!ir.contains("sext"), "{ir}");
    }

    #[test]
    fn new_allocates_via_malloc_and_stores_the_operand() {
        let (hir, mut tcx, _types, mir, instances) =
            crate::testing::lower_to_mir("fun f() -> iso i32 { return new 1; }");
        let llvm = inkwell::context::Context::create();
        let module = super::super::codegen(&llvm, &mut tcx, &mir, &instances, "t").unwrap();
        module.verify().unwrap();
        let ir = module.print_to_string().to_string();
        assert!(ir.contains("call ptr @malloc"), "{ir}");
    }

    #[test]
    fn new_array_allocates_via_malloc_and_fills_every_slot() {
        let (hir, mut tcx, _types, mir, instances) =
            crate::testing::lower_to_mir("fun f(n: usize) -> iso [u8] { return new [0_u8; n]; }");
        let llvm = inkwell::context::Context::create();
        let module = super::super::codegen(&llvm, &mut tcx, &mir, &instances, "t").unwrap();
        module.verify().unwrap();
        let ir = module.print_to_string().to_string();
        assert!(ir.contains("call ptr @malloc"), "{ir}");
        assert!(ir.contains("newarray.header"), "{ir}");
    }

    #[test]
    fn malloc_is_declared_once_across_multiple_new_sites_in_one_module() {
        let (hir, mut tcx, _types, mir, instances) = crate::testing::lower_to_mir(
            "fun f() -> iso i32 { return new 1; }
             fun g() -> iso i32 { return new 2; }",
        );
        let llvm = inkwell::context::Context::create();
        let module = super::super::codegen(&llvm, &mut tcx, &mir, &instances, "t").unwrap();
        module.verify().unwrap();
        let ir = module.print_to_string().to_string();
        assert_eq!(
            ir.matches("declare ptr @malloc").count(),
            1,
            "malloc must be declared exactly once no matter how many `new` sites reference it:\n{ir}"
        );
    }

    #[test]
    fn division_by_zero_assert_writes_and_aborts_on_failure() {
        let (hir, mut tcx, _types, mir, instances) = crate::testing::lower_mir_src_files(&[
            crate::testing::OPS_PREAMBLE,
            "fun f(a: i32, b: i32) -> i32 { return a / b; }",
        ]);
        let llvm = inkwell::context::Context::create();
        let module = super::super::codegen(&llvm, &mut tcx, &mir, &instances, "t").unwrap();
        module.verify().unwrap();
        let ir = module.print_to_string().to_string();
        assert!(ir.contains("call i64 @write"), "{ir}");
        assert!(ir.contains("call void @abort"), "{ir}");
        assert!(ir.contains("unreachable"), "{ir}");
    }

    #[test]
    fn narrow_enum_variant_construction_pads_to_the_widest_variants_alignment() {
        let (hir, mut tcx, _types, mir, instances) = crate::testing::lower_to_mir(
            "enum E { A, B: i32, C: i64 }
             fun f(n: i32) -> i32 {
                 let e: E = .B(n);
                 return match e { .A => 0, .B(x) => x, .C(y) => 0 };
             }",
        );
        let llvm = inkwell::context::Context::create();
        let module = super::super::codegen(&llvm, &mut tcx, &mir, &instances, "t").unwrap();
        assert!(
            module.verify().is_ok(),
            "{}",
            module.print_to_string().to_string()
        );
        let ir = module.print_to_string().to_string();
        assert!(
            ir.contains("%enum.1 = type { i8, [7 x i8], [8 x i8] }"),
            "{ir}"
        );
        assert!(
            ir.contains("{ i8, [7 x i8], { i32 } }"),
            "constructed value's padding doesn't match the enum's real, globally-computed \
             layout:\n{ir}"
        );
        assert!(
            ir.contains("getelementptr inbounds nuw %enum.1, ptr %_3, i32 0, i32 2"),
            "{ir}"
        );
    }

    #[test]
    fn calling_through_a_function_pointer_parameter_emits_an_indirect_call() {
        let (hir, mut tcx, _types, mir, instances) = crate::testing::lower_to_mir(
            "fun apply(f: fun(i32) -> i32, x: i32) -> i32 { return f(x); }",
        );
        let llvm = inkwell::context::Context::create();
        let module = super::super::codegen(&llvm, &mut tcx, &mir, &instances, "t").unwrap();
        module.verify().unwrap();
        let ir = module.print_to_string().to_string();
        assert!(ir.contains("call i32 %"), "{ir}");
        assert!(!ir.contains("call i32 @"), "{ir}");
    }

    #[test]
    fn array_aggregate_builds_via_insert_value() {
        let (hir, mut tcx, _types, mir, _instances) =
            crate::testing::lower_to_mir("fun f(a: [i32; 2]) {}");
        let array_len_id = find_array_len_id(&hir);
        let i32_ty = tcx.mk_prim(crate::nameres::PrimTy::I32);
        let array_ty = tcx.mk_array(i32_ty, Some(array_len_id));
        let unit = tcx.unit();
        let dummy_span = crate::driver::source::SrcSpan::new(0, 0);

        let local_decls = vec![
            crate::mir::LocalDecl {
                ty: unit,
                mutability: crate::ast::Mutability::Immutable,
                name: None,
                span: dummy_span,
            },
            crate::mir::LocalDecl {
                ty: i32_ty,
                mutability: crate::ast::Mutability::Immutable,
                name: None,
                span: dummy_span,
            },
            crate::mir::LocalDecl {
                ty: array_ty,
                mutability: crate::ast::Mutability::Mutable,
                name: None,
                span: dummy_span,
            },
        ];
        let param_operand = crate::mir::Operand::Copy(crate::mir::Place {
            local: crate::mir::Local::from_usize(1),
            projections: vec![],
        });
        let body = crate::mir::Body {
            def_id: instances_first_def(&hir),
            local_decls,
            param_count: 1,
            basic_blocks: vec![crate::mir::BasicBlockData {
                statements: vec![crate::mir::Statement {
                    id: crate::mir::StatementId::from_usize(0),
                    kind: crate::mir::StatementKind::Assign(
                        crate::mir::Place {
                            local: crate::mir::Local::from_usize(2),
                            projections: vec![],
                        },
                        crate::mir::Rvalue::Aggregate(
                            Box::new(crate::mir::AggregateKind::Array),
                            vec![param_operand.clone(), param_operand],
                        ),
                    ),
                    span: dummy_span,
                }],
                terminator: crate::mir::Terminator {
                    kind: crate::mir::TerminatorKind::Return,
                    span: dummy_span,
                },
            }],
            span: dummy_span,
        };

        let llvm = inkwell::context::Context::create();
        let mut cx = super::super::ctx::CodegenCtx::new(&llvm, "t");
        let i32_llvm_ty = cx.llvm.i32_type();
        let fn_ty = cx.llvm.void_type().fn_type(&[i32_llvm_ty.into()], false);
        let function = cx.module.add_function("f", fn_ty, None);
        super::lower_body(&mut cx, &mir, &mut tcx, &body, function);

        assert!(
            cx.module.verify().is_ok(),
            "{}",
            cx.module.print_to_string().to_string()
        );
        let ir = cx.module.print_to_string().to_string();
        assert!(ir.contains("insertvalue"), "{ir}");
        assert!(ir.contains("[2 x i32]"), "{ir}");
    }

    #[test]
    fn len_of_a_sized_array_place_is_a_compile_time_constant() {
        let (hir, mut tcx, _types, mir, _instances) =
            crate::testing::lower_to_mir("fun f(a: [i32; 2]) {}");
        let array_len_id = find_array_len_id(&hir);
        let i32_ty = tcx.mk_prim(crate::nameres::PrimTy::I32);
        let array_ty = tcx.mk_array(i32_ty, Some(array_len_id));
        let dummy_span = crate::driver::source::SrcSpan::new(0, 0);

        let llvm = inkwell::context::Context::create();
        let mut cx = super::super::ctx::CodegenCtx::new(&llvm, "t");
        let fn_ty = cx.llvm.void_type().fn_type(&[], false);
        let function = cx.module.add_function("f", fn_ty, None);
        let entry = cx.llvm.append_basic_block(function, "entry");
        cx.builder.position_at_end(entry);

        let local = crate::mir::Local::from_usize(0);
        let locals = std::collections::HashMap::new();
        let local_decls = vec![crate::mir::LocalDecl {
            ty: array_ty,
            mutability: crate::ast::Mutability::Immutable,
            name: None,
            span: dummy_span,
        }];
        let place = crate::mir::Place {
            local,
            projections: vec![],
        };

        let value = super::lower_len(&mut cx, &mut tcx, &mir, &locals, &local_decls, &place);
        let int_val = value.into_int_value();
        assert!(int_val.is_const());
        assert_eq!(int_val.get_sign_extended_constant(), Some(2));
    }

    #[test]
    fn ref_over_a_fat_place_rebuilds_the_two_word_pair() {
        let (_hir, mut tcx, _types, mir, _instances) = crate::testing::lower_to_mir("fun f() {}");
        let llvm = inkwell::context::Context::create();
        let mut cx = super::super::ctx::CodegenCtx::new(&llvm, "t");
        let fn_ty = cx.llvm.void_type().fn_type(&[], false);
        let function = cx.module.add_function("f", fn_ty, None);
        let entry = cx.llvm.append_basic_block(function, "entry");
        cx.builder.position_at_end(entry);

        let i32_ty = tcx.mk_prim(crate::nameres::PrimTy::I32);
        let unsized_arr = tcx.mk_array(i32_ty, None);
        let ref_ty = tcx.mk_ref(unsized_arr, crate::ast::Mutability::Immutable);

        let two_word_ty = super::two_word_struct_type(&cx);
        let alloca = cx.builder.build_alloca(two_word_ty, "s").unwrap();
        let field0 = cx
            .builder
            .build_struct_gep(two_word_ty, alloca, 0, "p0")
            .unwrap();
        cx.builder
            .build_store(field0, cx.llvm.ptr_type(Default::default()).const_null())
            .unwrap();
        let field1 = cx
            .builder
            .build_struct_gep(two_word_ty, alloca, 1, "p1")
            .unwrap();
        cx.builder
            .build_store(field1, cx.llvm.i64_type().const_int(7, false))
            .unwrap();

        let local = crate::mir::Local::from_usize(0);
        let mut locals = std::collections::HashMap::new();
        locals.insert(local, alloca);
        let local_decls = vec![crate::mir::LocalDecl {
            ty: ref_ty,
            mutability: crate::ast::Mutability::Immutable,
            name: None,
            span: crate::driver::source::SrcSpan::new(0, 0),
        }];
        let place = crate::mir::Place {
            local,
            projections: vec![crate::mir::Projection::Deref],
        };
        let rvalue = crate::mir::Rvalue::Ref {
            mutability: crate::ast::Mutability::Immutable,
            place,
        };

        let value = super::lower_rvalue(
            &mut cx,
            &mir,
            &mut tcx,
            &locals,
            &local_decls,
            ref_ty,
            &rvalue,
        );
        cx.builder.build_return(None).unwrap();

        assert!(
            cx.module.verify().is_ok(),
            "{}",
            cx.module.print_to_string().to_string()
        );
        assert!(value.is_struct_value());
    }

    fn find_array_len_id(hir: &crate::hir::Hir) -> crate::hir::HirId {
        for def_id in hir.def_ids() {
            if let crate::hir::OwnerNode::Function(function) = hir.def(def_id) {
                for &param_id in &function.params {
                    let param = hir.param(param_id);
                    if let crate::hir::TyKind::Array {
                        len: Some(len_id), ..
                    } = &hir.ty(param.ty).kind
                    {
                        return *len_id;
                    }
                }
            }
        }
        panic!("no array-typed parameter found");
    }

    fn instances_first_def(hir: &crate::hir::Hir) -> crate::hir::DefId {
        for def_id in hir.def_ids() {
            if let crate::hir::OwnerNode::Function(_) = hir.def(def_id) {
                return def_id;
            }
        }
        panic!("no function found");
    }
}
