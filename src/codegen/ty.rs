use inkwell::types::{BasicMetadataTypeEnum, BasicType, BasicTypeEnum, FunctionType};

use super::ctx::CodegenCtx;
use super::layout::{self, is_unsized};
use crate::mir::{Body, Mir};
use crate::nameres::PrimTy;
use crate::typeck::ty::{Ty, TyKind};
use crate::typeck::tyctx::TyCtx;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AbiClass {
    Scalar,
    Fat,
    Indirect,
    Void,
}

pub fn abi_class(tcx: &TyCtx, ty: Ty) -> AbiClass {
    match tcx.kind(ty) {
        TyKind::Unit | TyKind::Never => AbiClass::Void,
        TyKind::Ref { base, .. } | TyKind::Iso(base) if is_unsized(tcx, *base) => AbiClass::Fat,
        TyKind::Dyn { .. } | TyKind::Primitive(PrimTy::Str) | TyKind::Fun { .. } => AbiClass::Fat,
        TyKind::Tuple(_) | TyKind::Array { .. } | TyKind::Adt { .. } => AbiClass::Indirect,
        _ => AbiClass::Scalar,
    }
}

pub fn llvm_type<'ctx>(
    cx: &CodegenCtx<'ctx>,
    tcx: &mut TyCtx,
    mir: &Mir,
    ty: Ty,
) -> BasicTypeEnum<'ctx> {
    if let Some(cached) = cx.types.borrow().get(&ty) {
        return *cached;
    }

    let computed = match tcx.kind(ty).clone() {
        TyKind::Primitive(prim) => primitive_llvm_type(cx, prim),
        TyKind::Unit => cx.llvm.struct_type(&[], false).into(),
        TyKind::Ref { base, .. } | TyKind::Iso(base) if is_unsized(tcx, base) => two_word_type(cx),
        TyKind::Ref { .. } | TyKind::Iso(_) => cx.llvm.ptr_type(Default::default()).into(),
        TyKind::Fun { .. } => two_word_type(cx),
        TyKind::Tuple(elems) => struct_llvm_type(cx, tcx, mir, &elems),
        TyKind::Array {
            elem,
            len: Some(len),
        } => array_llvm_type(cx, tcx, mir, elem, len),
        TyKind::Adt { def, args } => adt_llvm_type(cx, tcx, mir, def, &args),
        TyKind::Dyn { .. } => two_word_type(cx),
        TyKind::Never => cx.llvm.struct_type(&[], false).into(),
        other => unreachable!("no LLVM representation for {other:?} at codegen time"),
    };

    cx.types.borrow_mut().insert(ty, computed);
    computed
}

fn primitive_llvm_type<'ctx>(cx: &CodegenCtx<'ctx>, prim: PrimTy) -> BasicTypeEnum<'ctx> {
    match prim {
        PrimTy::I8 | PrimTy::U8 => cx.llvm.i8_type().into(),
        PrimTy::I16 | PrimTy::U16 => cx.llvm.i16_type().into(),
        PrimTy::I32 | PrimTy::U32 | PrimTy::Char => cx.llvm.i32_type().into(),
        PrimTy::I64 | PrimTy::U64 | PrimTy::Usize => cx.llvm.i64_type().into(),
        PrimTy::F32 => cx.llvm.f32_type().into(),
        PrimTy::F64 => cx.llvm.f64_type().into(),
        PrimTy::Bool => cx.llvm.bool_type().into(),
        PrimTy::Str => two_word_type(cx),
    }
}

fn two_word_type<'ctx>(cx: &CodegenCtx<'ctx>) -> BasicTypeEnum<'ctx> {
    cx.llvm
        .struct_type(
            &[
                cx.llvm.ptr_type(Default::default()).into(),
                cx.llvm.i64_type().into(),
            ],
            false,
        )
        .into()
}

fn struct_llvm_type<'ctx>(
    cx: &CodegenCtx<'ctx>,
    tcx: &mut TyCtx,
    mir: &Mir,
    elems: &[Ty],
) -> BasicTypeEnum<'ctx> {
    let fields: Vec<BasicTypeEnum<'ctx>> = elems
        .iter()
        .map(|&elem| llvm_type(cx, tcx, mir, elem))
        .collect();
    cx.llvm.struct_type(&fields, false).into()
}

fn array_llvm_type<'ctx>(
    cx: &CodegenCtx<'ctx>,
    tcx: &mut TyCtx,
    mir: &Mir,
    elem: Ty,
    len: u64,
) -> BasicTypeEnum<'ctx> {
    let elem_ty = llvm_type(cx, tcx, mir, elem);
    elem_ty.array_type(len as u32).into()
}

fn adt_llvm_type<'ctx>(
    cx: &CodegenCtx<'ctx>,
    tcx: &mut TyCtx,
    mir: &Mir,
    def: crate::hir::DefId,
    args: &[Ty],
) -> BasicTypeEnum<'ctx> {
    let adt_ty = tcx_adt_ty(tcx, def, args);
    match tcx.enum_variant_count(def) {
        Some(_) => {
            let layout = layout::layout_of(tcx, mir, adt_ty);
            let tag_ty = match layout
                .tag_ty
                .expect("an enum's own layout always has a tag")
            {
                layout::TagTy::I8 => cx.llvm.i8_type(),
                layout::TagTy::I16 => cx.llvm.i16_type(),
                layout::TagTy::I32 => cx.llvm.i32_type(),
            };
            let tag_size = tag_ty.get_bit_width() as u64 / 8;
            let pad_bytes = layout.payload_offset - tag_size;
            let payload_bytes = layout.size - layout.payload_offset;
            let name = format!("enum.{}", def.index());
            let opaque = cx.llvm.opaque_struct_type(&name);
            let pad = cx.llvm.i8_type().array_type(pad_bytes as u32);
            let payload = cx.llvm.i8_type().array_type(payload_bytes as u32);
            opaque.set_body(&[tag_ty.into(), pad.into(), payload.into()], false);
            opaque.into()
        }
        None => {
            let layout = layout::layout_of(tcx, mir, adt_ty);
            let name = format!("struct.{}", def.index());
            let opaque = cx.llvm.opaque_struct_type(&name);
            let fields: Vec<BasicTypeEnum<'ctx>> = layout
                .fields
                .iter()
                .map(|field| llvm_type(cx, tcx, mir, field.ty))
                .collect();
            opaque.set_body(&fields, false);
            opaque.into()
        }
    }
}

fn tcx_adt_ty(tcx: &mut TyCtx, def: crate::hir::DefId, args: &[Ty]) -> Ty {
    tcx.mk_adt(def, args.to_vec())
}

pub fn function_type<'ctx>(
    cx: &CodegenCtx<'ctx>,
    tcx: &mut TyCtx,
    mir: &Mir,
    body: &Body,
) -> FunctionType<'ctx> {
    let ret_ty = body.local_decls[0].ty;
    let param_tys: Vec<Ty> = body.local_decls[1..=body.param_count]
        .iter()
        .map(|decl| decl.ty)
        .collect();

    let mut param_llvm = Vec::new();
    for &param in &param_tys {
        push_param_types(cx, tcx, mir, param, &mut param_llvm);
    }

    match abi_class(tcx, ret_ty) {
        AbiClass::Void => cx.llvm.void_type().fn_type(&param_llvm, false),
        AbiClass::Indirect => {
            let mut params = vec![cx.llvm.ptr_type(Default::default()).into()];
            params.extend(param_llvm);
            cx.llvm.void_type().fn_type(&params, false)
        }
        _ => llvm_type(cx, tcx, mir, ret_ty).fn_type(&param_llvm, false),
    }
}

fn push_param_types<'ctx>(
    cx: &CodegenCtx<'ctx>,
    tcx: &mut TyCtx,
    mir: &Mir,
    ty: Ty,
    out: &mut Vec<BasicMetadataTypeEnum<'ctx>>,
) {
    match abi_class(tcx, ty) {
        AbiClass::Void => {}
        AbiClass::Fat => {
            out.push(cx.llvm.ptr_type(Default::default()).into());
            out.push(cx.llvm.i64_type().into());
        }
        AbiClass::Indirect => out.push(cx.llvm.ptr_type(Default::default()).into()),
        AbiClass::Scalar => out.push(llvm_type(cx, tcx, mir, ty).into()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn primitive_ints_map_directly() {
        let llvm = inkwell::context::Context::create();
        let (_hir, mut tcx, _types, mir, _instances) = crate::testing::lower_to_mir("fun f() {}");
        let cx = super::super::ctx::CodegenCtx::new(&llvm, "t");
        let i32_ty = tcx.mk_prim(crate::nameres::PrimTy::I32);
        assert_eq!(
            llvm_type(&cx, &mut tcx, &mir, i32_ty),
            llvm.i32_type().into()
        );
    }

    #[test]
    fn bool_is_i1_in_registers() {
        let llvm = inkwell::context::Context::create();
        let (_hir, mut tcx, _types, mir, _instances) = crate::testing::lower_to_mir("fun f() {}");
        let cx = super::super::ctx::CodegenCtx::new(&llvm, "t");
        let bool_ty = tcx.mk_prim(crate::nameres::PrimTy::Bool);
        assert_eq!(
            llvm_type(&cx, &mut tcx, &mir, bool_ty),
            llvm.bool_type().into()
        );
    }

    #[test]
    fn slice_reference_is_ptr_and_len() {
        let llvm = inkwell::context::Context::create();
        let (_hir, mut tcx, _types, mir, _instances) = crate::testing::lower_to_mir("fun f() {}");
        let cx = super::super::ctx::CodegenCtx::new(&llvm, "t");
        let u8_ty = tcx.mk_prim(crate::nameres::PrimTy::U8);
        let arr = tcx.mk_array(u8_ty, None);
        let slice_ref = tcx.mk_ref(arr, crate::ast::Mutability::Immutable);
        let got = llvm_type(&cx, &mut tcx, &mir, slice_ref);
        let expected = llvm.struct_type(
            &[
                llvm.ptr_type(Default::default()).into(),
                llvm.i64_type().into(),
            ],
            false,
        );
        assert_eq!(got, expected.into());
    }

    #[test]
    fn unit_is_an_empty_struct() {
        let llvm = inkwell::context::Context::create();
        let (_hir, mut tcx, _types, mir, _instances) = crate::testing::lower_to_mir("fun f() {}");
        let cx = super::super::ctx::CodegenCtx::new(&llvm, "t");
        let unit = tcx.unit();
        assert_eq!(
            llvm_type(&cx, &mut tcx, &mir, unit),
            llvm.struct_type(&[], false).into()
        );
    }

    #[test]
    fn tuple_is_a_struct_of_its_elements() {
        let llvm = inkwell::context::Context::create();
        let (_hir, mut tcx, _types, mir, _instances) = crate::testing::lower_to_mir("fun f() {}");
        let cx = super::super::ctx::CodegenCtx::new(&llvm, "t");
        let i32_ty = tcx.mk_prim(crate::nameres::PrimTy::I32);
        let bool_ty = tcx.mk_prim(crate::nameres::PrimTy::Bool);
        let tuple = tcx.mk_tuple(vec![i32_ty, bool_ty]);
        let got = llvm_type(&cx, &mut tcx, &mir, tuple);
        let expected = llvm.struct_type(&[llvm.i32_type().into(), llvm.bool_type().into()], false);
        assert_eq!(got, expected.into());
    }

    #[test]
    fn fixed_array_is_an_llvm_array() {
        let llvm = inkwell::context::Context::create();
        let (_hir, mut tcx, _types, mir, _instances) =
            crate::testing::lower_to_mir("fun f(a: [i32; 4]) {}");
        let cx = super::super::ctx::CodegenCtx::new(&llvm, "t");
        let i32_ty = tcx.mk_prim(crate::nameres::PrimTy::I32);
        let arr = tcx.mk_array(i32_ty, Some(4));
        let got = llvm_type(&cx, &mut tcx, &mir, arr);
        assert_eq!(got, llvm.i32_type().array_type(4).into());
    }

    #[test]
    fn struct_adt_becomes_a_named_llvm_struct_of_its_fields() {
        let (hir, mut tcx, _types, mir, _instances) =
            crate::testing::lower_to_mir("struct Point { x: i32, y: i32 }\nfun f() {}");
        let llvm = inkwell::context::Context::create();
        let cx = super::super::ctx::CodegenCtx::new(&llvm, "t");
        let def = find_struct_def(&hir, "Point");
        let adt = tcx.mk_adt(def, Vec::new());
        let got = llvm_type(&cx, &mut tcx, &mir, adt);
        let BasicTypeEnum::StructType(struct_ty) = got else {
            panic!("expected a struct type, got {got:?}");
        };
        assert_eq!(struct_ty.count_fields(), 2);
        assert_eq!(
            struct_ty.get_field_type_at_index(0),
            Some(llvm.i32_type().into())
        );
        assert_eq!(
            struct_ty.get_field_type_at_index(1),
            Some(llvm.i32_type().into())
        );
    }

    #[test]
    fn enum_payload_is_padded_out_to_its_own_alignment() {
        let (hir, mut tcx, _types, mir, _instances) =
            crate::testing::lower_to_mir("enum E { A, C: i64 }\nfun f() {}");
        let llvm = inkwell::context::Context::create();
        let cx = super::super::ctx::CodegenCtx::new(&llvm, "t");
        let def = find_enum_def(&hir, "E");
        let adt = tcx.mk_adt(def, Vec::new());
        let got = llvm_type(&cx, &mut tcx, &mir, adt);
        let BasicTypeEnum::StructType(struct_ty) = got else {
            panic!("expected a struct type, got {got:?}");
        };
        assert_eq!(struct_ty.count_fields(), 3, "{{tag, pad, payload}}");
        assert_eq!(
            struct_ty.get_field_type_at_index(0),
            Some(llvm.i8_type().into()),
            "tag is i8 for a 2-variant enum"
        );
        assert_eq!(
            struct_ty.get_field_type_at_index(1),
            Some(llvm.i8_type().array_type(7).into()),
            "7 bytes of padding to round the 1-byte tag up to the payload's 8-byte alignment"
        );
        assert_eq!(
            struct_ty.get_field_type_at_index(2),
            Some(llvm.i8_type().array_type(8).into()),
            "payload is field index 2, sized to the widest (i64) variant"
        );
    }

    #[test]
    fn indirect_return_prepends_a_hidden_pointer_parameter() {
        let (hir, mut tcx, _types, mir, instances) = crate::testing::lower_to_mir(
            "struct Pair { a: i32, b: i32 }\nfun make() -> Pair { return Pair { a: 1, b: 2 }; }",
        );
        let llvm = inkwell::context::Context::create();
        let cx = super::super::ctx::CodegenCtx::new(&llvm, "t");
        let body = find_body(&hir, &instances, "make");
        let fn_ty = function_type(&cx, &mut tcx, &mir, body);
        assert_eq!(fn_ty.get_return_type(), None, "indirect return is void");
        assert_eq!(fn_ty.count_param_types(), 1, "one hidden out-pointer param");
    }

    #[test]
    fn scalar_return_and_params_map_directly() {
        let (hir, mut tcx, _types, mir, instances) =
            crate::testing::lower_to_mir("fun add(a: i32, b: i32) -> i32 { return a; }");
        let llvm = inkwell::context::Context::create();
        let cx = super::super::ctx::CodegenCtx::new(&llvm, "t");
        let body = find_body(&hir, &instances, "add");
        let fn_ty = function_type(&cx, &mut tcx, &mir, body);
        assert_eq!(fn_ty.count_param_types(), 2);
        assert_eq!(fn_ty.get_return_type(), Some(llvm.i32_type().into()));
    }

    fn find_struct_def(hir: &crate::hir::Hir, name: &str) -> crate::hir::DefId {
        for def_id in hir.def_ids() {
            if let crate::hir::OwnerNode::Struct(struct_) = hir.def(def_id)
                && crate::ast::interner::Interner::resolve(struct_.name.text) == name
            {
                return def_id;
            }
        }
        panic!("no struct named {name:?} found");
    }

    fn find_enum_def(hir: &crate::hir::Hir, name: &str) -> crate::hir::DefId {
        for def_id in hir.def_ids() {
            if let crate::hir::OwnerNode::Enum(enum_) = hir.def(def_id)
                && crate::ast::interner::Interner::resolve(enum_.name.text) == name
            {
                return def_id;
            }
        }
        panic!("no enum named {name:?} found");
    }

    fn find_body<'a>(
        hir: &crate::hir::Hir,
        instances: &'a std::collections::HashMap<crate::mir::Instance, Body>,
        name: &str,
    ) -> &'a Body {
        instances
            .iter()
            .find(|(instance, _)| {
                if let crate::hir::OwnerNode::Function(function) = hir.def(instance.def) {
                    crate::ast::interner::Interner::resolve(function.name.text) == name
                } else {
                    false
                }
            })
            .map(|(_, body)| body)
            .unwrap_or_else(|| panic!("no instance named {name:?} found"))
    }
}
