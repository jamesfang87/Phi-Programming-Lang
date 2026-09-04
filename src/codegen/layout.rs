use std::collections::HashMap;

use crate::hir::{DefId, HirId};
use crate::mir::{AdtDef, Mir, VariantIdx};
use crate::nameres::PrimTy;
use crate::typeck::fold::subst_ty;
use crate::typeck::ty::{Ty, TyKind};
use crate::typeck::tyctx::TyCtx;

pub struct FieldLayout {
    pub ty: Ty,
    pub offset: u64,
}

pub struct AdtLayout {
    pub size: u64,
    pub align: u64,
    pub fields: Vec<FieldLayout>,
    pub tag_ty: Option<TagTy>,
    pub payload_offset: u64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TagTy {
    I8,
    I16,
    I32,
}

impl TagTy {
    fn for_variant_count(count: usize) -> TagTy {
        if count <= 1 << 8 {
            TagTy::I8
        } else if count <= 1 << 16 {
            TagTy::I16
        } else {
            TagTy::I32
        }
    }

    fn size_align(self) -> (u64, u64) {
        match self {
            TagTy::I8 => (1, 1),
            TagTy::I16 => (2, 2),
            TagTy::I32 => (4, 4),
        }
    }
}

pub fn layout_of(tcx: &mut TyCtx, mir: &Mir, ty: Ty) -> AdtLayout {
    match tcx.kind(ty).clone() {
        TyKind::Tuple(elems) => layout_fields(tcx, mir, &elems),
        TyKind::Adt { def, args } => match &mir.adts[&def] {
            AdtDef::Struct { generics, fields } => {
                let field_tys = subst_field_tys(tcx, generics, fields, &args);
                layout_fields(tcx, mir, &field_tys)
            }
            AdtDef::Enum { variants, .. } => enum_layout(tcx, mir, def, &args, variants.len()),
        },
        other => panic!("layout_of: {other:?} has no field-list layout"),
    }
}

pub fn variant_layout(
    tcx: &mut TyCtx,
    mir: &Mir,
    def: DefId,
    args: &[Ty],
    variant: VariantIdx,
) -> AdtLayout {
    let field_tys = variant_field_tys(tcx, mir, def, args, variant);
    layout_fields(tcx, mir, &field_tys)
}

pub fn field_index(_ty: Ty, field: u32) -> usize {
    field as usize
}

fn enum_layout(tcx: &mut TyCtx, mir: &Mir, def: DefId, args: &[Ty], variant_count: usize) -> AdtLayout {
    let tag_ty = TagTy::for_variant_count(variant_count);
    let (tag_size, tag_align) = tag_ty.size_align();

    let mut payload_size = 0u64;
    let mut payload_align = 1u64;
    for index in 0..variant_count {
        let variant = VariantIdx::from_usize(index);
        let layout = variant_layout(tcx, mir, def, args, variant);
        payload_size = payload_size.max(layout.size);
        payload_align = payload_align.max(layout.align);
    }

    let align = tag_align.max(payload_align);
    let payload_offset = round_up(tag_size, payload_align);
    let size = round_up(payload_offset + payload_size, align);

    AdtLayout {
        size,
        align,
        fields: Vec::new(),
        tag_ty: Some(tag_ty),
        payload_offset,
    }
}

fn subst_field_tys(tcx: &mut TyCtx, generics: &[HirId], fields: &[Ty], args: &[Ty]) -> Vec<Ty> {
    let subst = generic_subst(generics, args);
    fields.iter().map(|&declared| subst_ty(tcx, declared, &subst)).collect()
}

fn variant_field_tys(
    tcx: &mut TyCtx,
    mir: &Mir,
    def: DefId,
    args: &[Ty],
    variant: VariantIdx,
) -> Vec<Ty> {
    let AdtDef::Enum { generics, variants } = &mir.adts[&def] else {
        panic!("variant_field_tys: {def:?} is not an enum");
    };
    let subst = generic_subst(generics, args);
    variants[variant.index()]
        .field_tys
        .iter()
        .map(|&declared| subst_ty(tcx, declared, &subst))
        .collect()
}

fn generic_subst(generics: &[HirId], args: &[Ty]) -> HashMap<HirId, Ty> {
    generics.iter().copied().zip(args.iter().copied()).collect()
}

fn layout_fields(tcx: &mut TyCtx, mir: &Mir, tys: &[Ty]) -> AdtLayout {
    let mut offset = 0u64;
    let mut align = 1u64;
    let mut fields = Vec::with_capacity(tys.len());

    for &ty in tys {
        let (size, field_align) = size_align_of(tcx, mir, ty);
        offset = round_up(offset, field_align);
        fields.push(FieldLayout { ty, offset });
        offset += size;
        align = align.max(field_align);
    }

    AdtLayout {
        size: round_up(offset, align),
        align,
        fields,
        tag_ty: None,
        payload_offset: 0,
    }
}

fn size_align_of(tcx: &mut TyCtx, mir: &Mir, ty: Ty) -> (u64, u64) {
    match tcx.kind(ty).clone() {
        TyKind::Primitive(prim) => primitive_size_align(prim),
        TyKind::Unit | TyKind::Never => (0, 1),
        TyKind::Fun { .. } => (8, 8),
        TyKind::Ref { base, .. } | TyKind::Iso(base) => {
            if is_unsized(tcx, base) {
                (16, 8)
            } else {
                (8, 8)
            }
        }
        TyKind::Dyn { .. } => (16, 8),
        TyKind::Array {
            elem,
            len: Some(len_id),
        } => {
            let (elem_size, elem_align) = size_align_of(tcx, mir, elem);
            let len = array_len(mir, len_id);
            (round_up(elem_size, elem_align) * len, elem_align)
        }
        TyKind::Array { len: None, .. } => {
            unreachable!("size_align_of: a bare unsized array cannot be a field's own type")
        }
        TyKind::Tuple(_) | TyKind::Adt { .. } => {
            let layout = layout_of(tcx, mir, ty);
            (layout.size, layout.align)
        }
        other => unreachable!("size_align_of: no layout for {other:?} at codegen time"),
    }
}

fn primitive_size_align(prim: PrimTy) -> (u64, u64) {
    match prim {
        PrimTy::I8 | PrimTy::U8 | PrimTy::Bool => (1, 1),
        PrimTy::I16 | PrimTy::U16 => (2, 2),
        PrimTy::I32 | PrimTy::U32 | PrimTy::F32 | PrimTy::Char => (4, 4),
        PrimTy::I64 | PrimTy::U64 | PrimTy::F64 | PrimTy::Usize => (8, 8),
        PrimTy::Str => (16, 8),
    }
}

pub(super) fn is_unsized(tcx: &TyCtx, ty: Ty) -> bool {
    matches!(
        tcx.kind(ty),
        TyKind::Dyn { .. } | TyKind::Array { len: None, .. }
    )
}

pub(super) fn array_len(mir: &Mir, len_id: HirId) -> u64 {
    mir.array_lens.get(&len_id).copied().unwrap_or_else(|| {
        panic!(
            "array_len: {len_id:?} was never recorded -- mir::lower::collect_array_lens is \
             expected to evaluate every array type's length up front"
        )
    })
}

fn round_up(offset: u64, align: u64) -> u64 {
    (offset + align - 1) / align * align
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::hir::{Hir, OwnerNode};

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

    fn find_enum_def(hir: &Hir, name: &str) -> DefId {
        for def_id in hir.def_ids() {
            if let OwnerNode::Enum(enum_) = hir.def(def_id)
                && crate::ast::interner::Interner::resolve(enum_.name.text) == name
            {
                return def_id;
            }
        }
        panic!("no enum named {name:?} found");
    }

    #[test]
    fn struct_fields_are_laid_out_in_declaration_order_with_padding() {
        let (hir, mut tcx, _types, mir, _instances) =
            crate::testing::lower_to_mir("struct S { a: i8, b: i32, c: i8 }\nfun f() {}");
        let def = find_struct_def(&hir, "S");
        let adt = tcx.mk_adt(def, Vec::new());
        let layout = layout_of(&mut tcx, &mir, adt);

        assert_eq!(layout.fields.len(), 3);
        assert_eq!(layout.fields[0].offset, 0);
        assert_eq!(layout.fields[1].offset, 4);
        assert_eq!(layout.fields[2].offset, 8);
        assert_eq!(layout.align, 4);
        assert_eq!(layout.size, 12);
    }

    #[test]
    fn tuple_layout_matches_struct_layout_rules() {
        let (_hir, mut tcx, _types, mir, _instances) = crate::testing::lower_to_mir("fun f() {}");
        let i8_ty = tcx.mk_prim(PrimTy::I8);
        let i32_ty = tcx.mk_prim(PrimTy::I32);
        let tuple = tcx.mk_tuple(vec![i8_ty, i32_ty]);
        let layout = layout_of(&mut tcx, &mir, tuple);

        assert_eq!(layout.fields[0].offset, 0);
        assert_eq!(layout.fields[1].offset, 4);
        assert_eq!(layout.align, 4);
        assert_eq!(layout.size, 8);
    }

    #[test]
    fn generic_struct_fields_substitute_the_concrete_type_argument() {
        let (hir, mut tcx, _types, mir, _instances) =
            crate::testing::lower_to_mir("struct Box<T> { value: T }\nfun f() {}");
        let def = find_struct_def(&hir, "Box");
        let i64_ty = tcx.mk_prim(PrimTy::I64);
        let adt = tcx.mk_adt(def, vec![i64_ty]);
        let layout = layout_of(&mut tcx, &mir, adt);

        assert_eq!(layout.fields.len(), 1);
        assert_eq!(layout.fields[0].ty, i64_ty);
        assert_eq!(layout.size, 8);
        assert_eq!(layout.align, 8);
    }

    #[test]
    fn small_enum_gets_an_i8_tag() {
        let (hir, mut tcx, _types, mir, _instances) =
            crate::testing::lower_to_mir("enum E { A, B: i32, C: { x: i64 } }\nfun f() {}");
        let def = find_enum_def(&hir, "E");
        let adt = tcx.mk_adt(def, Vec::new());
        let layout = layout_of(&mut tcx, &mir, adt);

        assert_eq!(layout.tag_ty, Some(TagTy::I8));
        assert_eq!(layout.align, 8);
        assert_eq!(layout.size, 16);
    }

    #[test]
    fn variant_layout_lays_out_only_that_variants_payload() {
        let (hir, mut tcx, _types, mir, _instances) =
            crate::testing::lower_to_mir("enum E { A, B: i32, C: { x: i64 } }\nfun f() {}");
        let def = find_enum_def(&hir, "E");

        let unit_layout = variant_layout(&mut tcx, &mir, def, &[], VariantIdx::from_usize(0));
        assert_eq!(unit_layout.size, 0);
        assert!(unit_layout.fields.is_empty());

        let tuple_layout = variant_layout(&mut tcx, &mir, def, &[], VariantIdx::from_usize(1));
        assert_eq!(tuple_layout.fields.len(), 1);
        assert_eq!(tuple_layout.size, 4);

        let record_layout = variant_layout(&mut tcx, &mir, def, &[], VariantIdx::from_usize(2));
        assert_eq!(record_layout.fields.len(), 1);
        assert_eq!(record_layout.size, 8);
    }

    #[test]
    fn field_index_is_declaration_order_in_v1() {
        let (hir, mut tcx, _types, _mir, _instances) =
            crate::testing::lower_to_mir("struct S { a: i8, b: i32 }\nfun f() {}");
        let def = find_struct_def(&hir, "S");
        let adt = tcx.mk_adt(def, Vec::new());
        assert_eq!(field_index(adt, 0), 0);
        assert_eq!(field_index(adt, 1), 1);
    }
}
