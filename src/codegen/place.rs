use std::collections::HashMap;

use inkwell::values::PointerValue;

use super::ctx::CodegenCtx;
use super::{layout, ty};
use crate::hir::DefId;
use crate::mir::{Local, LocalDecl, Mir, Place, Projection, VariantIdx};
use crate::typeck::ty::{Ty, TyKind};
use crate::typeck::tyctx::TyCtx;

pub fn lower_place<'ctx>(
    cx: &CodegenCtx<'ctx>,
    tcx: &mut TyCtx,
    mir: &Mir,
    locals: &HashMap<Local, PointerValue<'ctx>>,
    local_decls: &[LocalDecl],
    place: &Place,
) -> (PointerValue<'ctx>, Ty) {
    let mut ptr = locals[&place.local];
    let mut ty = local_decls[place.local.index()].ty;

    for proj in &place.projections {
        match proj {
            Projection::Deref => {
                let loaded = cx
                    .builder
                    .build_load(cx.llvm.ptr_type(Default::default()), ptr, "deref")
                    .unwrap();
                ptr = loaded.into_pointer_value();
                ty = deref_target(tcx, ty);
            }
            Projection::Field(index) => {
                let field_idx = layout::field_index(ty, *index) as u32;
                let struct_llvm_ty = ty::llvm_type(cx, tcx, mir, ty).into_struct_type();
                ptr = cx
                    .builder
                    .build_struct_gep(struct_llvm_ty, ptr, field_idx, "field")
                    .unwrap();
                ty = field_ty(tcx, mir, ty, *index);
            }
            Projection::Index(index_local) => {
                let index_val = cx
                    .builder
                    .build_load(cx.llvm.i64_type(), locals[index_local], "index")
                    .unwrap();
                let elem = elem_ty(tcx, ty);
                let elem_llvm_ty = ty::llvm_type(cx, tcx, mir, elem);
                ptr = unsafe {
                    cx.builder
                        .build_gep(elem_llvm_ty, ptr, &[index_val.into_int_value()], "elem")
                        .unwrap()
                };
                ty = elem;
            }
            Projection::ConstantIndex(offset) => {
                let elem = elem_ty(tcx, ty);
                let elem_llvm_ty = ty::llvm_type(cx, tcx, mir, elem);
                let idx = cx.llvm.i64_type().const_int(*offset as u64, false);
                ptr = unsafe {
                    cx.builder
                        .build_gep(elem_llvm_ty, ptr, &[idx], "elem")
                        .unwrap()
                };
                ty = elem;
            }
            Projection::Downcast(variant) => {
                let (def, args) = match tcx.kind(ty).clone() {
                    TyKind::Adt { def, args } => (def, args),
                    other => panic!("Downcast projection on non-Adt type {other:?}"),
                };
                let enum_llvm_ty = ty::llvm_type(cx, tcx, mir, ty).into_struct_type();
                ptr = cx
                    .builder
                    .build_struct_gep(enum_llvm_ty, ptr, 2, "payload")
                    .unwrap();
                ty = variant_ty(tcx, mir, def, &args, *variant);
            }
        }
    }

    (ptr, ty)
}

fn deref_target(tcx: &mut TyCtx, ty: Ty) -> Ty {
    match tcx.kind(ty).clone() {
        TyKind::Ref { base, .. } | TyKind::Iso(base) => base,
        other => panic!("Deref projection on non-reference type {other:?}"),
    }
}

fn field_ty(tcx: &mut TyCtx, mir: &Mir, ty: Ty, index: u32) -> Ty {
    match tcx.kind(ty).clone() {
        TyKind::Tuple(elems) => elems[index as usize],
        TyKind::Adt { .. } => layout::layout_of(tcx, mir, ty).fields[index as usize].ty,
        other => panic!("Field projection on non-aggregate type {other:?}"),
    }
}

fn elem_ty(tcx: &mut TyCtx, ty: Ty) -> Ty {
    match tcx.kind(ty).clone() {
        TyKind::Array { elem, .. } => elem,
        other => panic!("Index/ConstantIndex projection on non-array type {other:?}"),
    }
}

fn variant_ty(tcx: &mut TyCtx, mir: &Mir, def: DefId, args: &[Ty], variant: VariantIdx) -> Ty {
    let layout = layout::variant_layout(tcx, mir, def, args, variant);
    let field_tys: Vec<Ty> = layout.fields.iter().map(|field| field.ty).collect();
    tcx.mk_tuple(field_tys)
}

#[cfg(test)]
mod tests {
    use inkwell::values::InstructionOpcode;

    use super::*;
    use crate::ast::Mutability;
    use crate::driver::source::SrcSpan;
    use crate::nameres::PrimTy;

    fn function_with_entry<'ctx>(cx: &CodegenCtx<'ctx>) -> inkwell::values::FunctionValue<'ctx> {
        let fn_ty = cx.llvm.void_type().fn_type(&[], false);
        let function = cx.module.add_function("f", fn_ty, None);
        let entry = cx.llvm.append_basic_block(function, "entry");
        cx.builder.position_at_end(entry);
        function
    }

    #[test]
    fn field_projection_gets_to_a_struct_field_via_gep() {
        let (hir, mut tcx, _types, mir, _instances) =
            crate::testing::lower_to_mir("struct Point { x: i32, y: i32 }\nfun f() {}");
        let llvm = inkwell::context::Context::create();
        let cx = CodegenCtx::new(&llvm, "t");
        function_with_entry(&cx);

        let def = find_struct_def(&hir, "Point");
        let adt = tcx.mk_adt(def, Vec::new());
        let struct_llvm_ty = ty::llvm_type(&cx, &mut tcx, &mir, adt).into_struct_type();
        let alloca = cx.builder.build_alloca(struct_llvm_ty, "point").unwrap();

        let local = crate::mir::Local::from_usize(0);
        let mut locals = HashMap::new();
        locals.insert(local, alloca);
        let local_decls = vec![LocalDecl {
            ty: adt,
            name: None,
            span: SrcSpan::new(0, 0),
        }];

        let place = Place {
            local,
            projections: vec![Projection::Field(0)],
        };
        let (ptr, field_ty_got) = lower_place(&cx, &mut tcx, &mir, &locals, &local_decls, &place);

        let i32_ty = tcx.mk_prim(PrimTy::I32);
        assert_eq!(field_ty_got, i32_ty);
        assert_eq!(
            ptr.as_instruction().unwrap().get_opcode(),
            InstructionOpcode::GetElementPtr
        );
    }

    #[test]
    fn deref_projection_loads_the_pointee_address() {
        let (_hir, mut tcx, _types, mir, _instances) = crate::testing::lower_to_mir("fun f() {}");
        let llvm = inkwell::context::Context::create();
        let cx = CodegenCtx::new(&llvm, "t");
        function_with_entry(&cx);

        let i32_ty = tcx.mk_prim(PrimTy::I32);
        let ref_ty = tcx.mk_ref(i32_ty, Mutability::Immutable);
        let ptr_llvm_ty = cx.llvm.ptr_type(Default::default());
        let alloca = cx.builder.build_alloca(ptr_llvm_ty, "r").unwrap();

        let local = crate::mir::Local::from_usize(0);
        let mut locals = HashMap::new();
        locals.insert(local, alloca);
        let local_decls = vec![LocalDecl {
            ty: ref_ty,
            name: None,
            span: SrcSpan::new(0, 0),
        }];

        let place = Place {
            local,
            projections: vec![Projection::Deref],
        };
        let (ptr, deref_ty) = lower_place(&cx, &mut tcx, &mir, &locals, &local_decls, &place);

        assert_eq!(deref_ty, i32_ty);
        assert_eq!(
            ptr.as_instruction().unwrap().get_opcode(),
            InstructionOpcode::Load
        );
    }

    #[test]
    fn downcast_then_field_reaches_the_variants_payload_at_the_correct_offset() {
        let (hir, mut tcx, _types, mir, _instances) =
            crate::testing::lower_to_mir("enum E { A, C: i64 }\nfun f() {}");
        let llvm = inkwell::context::Context::create();
        let cx = CodegenCtx::new(&llvm, "t");
        function_with_entry(&cx);

        let def = find_enum_def(&hir, "E");
        let adt = tcx.mk_adt(def, Vec::new());
        let enum_llvm_ty = ty::llvm_type(&cx, &mut tcx, &mir, adt).into_struct_type();
        let alloca = cx.builder.build_alloca(enum_llvm_ty, "e").unwrap();

        let local = crate::mir::Local::from_usize(0);
        let mut locals = HashMap::new();
        locals.insert(local, alloca);
        let local_decls = vec![LocalDecl {
            ty: adt,
            name: None,
            span: SrcSpan::new(0, 0),
        }];

        let place = Place {
            local,
            projections: vec![
                Projection::Downcast(VariantIdx::from_usize(1)),
                Projection::Field(0),
            ],
        };
        let (ptr, field_ty_got) = lower_place(&cx, &mut tcx, &mir, &locals, &local_decls, &place);

        let i64_ty = tcx.mk_prim(PrimTy::I64);
        assert_eq!(field_ty_got, i64_ty);
        assert_eq!(
            ptr.as_instruction().unwrap().get_opcode(),
            InstructionOpcode::GetElementPtr
        );

        let ir = cx.module.print_to_string().to_string();
        assert!(
            ir.contains(", 2\n") || ir.contains(", i32 2"),
            "expected a GEP indexing field 2 (post-padding-fix payload slot):\n{ir}"
        );
    }

    fn find_struct_def(hir: &crate::hir::Hir, name: &str) -> DefId {
        for def_id in hir.def_ids() {
            if let crate::hir::OwnerNode::Struct(struct_) = hir.def(def_id)
                && crate::ast::interner::Interner::resolve(struct_.name.text) == name
            {
                return def_id;
            }
        }
        panic!("no struct named {name:?} found");
    }

    fn find_enum_def(hir: &crate::hir::Hir, name: &str) -> DefId {
        for def_id in hir.def_ids() {
            if let crate::hir::OwnerNode::Enum(enum_) = hir.def(def_id)
                && crate::ast::interner::Interner::resolve(enum_.name.text) == name
            {
                return def_id;
            }
        }
        panic!("no enum named {name:?} found");
    }
}
