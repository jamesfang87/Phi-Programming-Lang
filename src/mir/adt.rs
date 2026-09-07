use std::collections::HashMap;

use crate::hir::{DefId, Hir, HirId, OwnerNode, VariantPayload};
use crate::typeck::results::TypeResolutions;
use crate::typeck::ty::Ty;

#[derive(Debug)]
pub enum AdtDef {
    Struct {
        generics: Vec<HirId>,
        fields: Vec<Ty>,
    },
    Enum {
        generics: Vec<HirId>,
        variants: Vec<VariantDef>,
    },
}

#[derive(Debug)]
pub struct VariantDef {
    pub field_tys: Vec<Ty>,
}

pub(crate) fn collect_adt_defs(hir: &Hir, types: &TypeResolutions) -> HashMap<DefId, AdtDef> {
    let mut out = HashMap::new();
    for def_id in hir.def_ids() {
        match hir.def(def_id) {
            OwnerNode::Struct(struct_) => {
                let fields = struct_
                    .fields
                    .iter()
                    .map(|&field_id| {
                        types.ty(field_id).unwrap_or_else(|| {
                            panic!(
                                "collect_adt_defs: {field_id:?} has no recorded type -- \
                                 collect_fields is expected to record every field's declared type"
                            )
                        })
                    })
                    .collect();
                out.insert(
                    def_id,
                    AdtDef::Struct {
                        generics: struct_.generics.clone(),
                        fields,
                    },
                );
            }
            OwnerNode::Enum(enum_) => {
                let variants = enum_
                    .variants
                    .iter()
                    .map(|&variant_id| variant_def(hir, types, variant_id))
                    .collect();
                out.insert(
                    def_id,
                    AdtDef::Enum {
                        generics: enum_.generics.clone(),
                        variants,
                    },
                );
            }
            _ => {}
        }
    }
    out
}

fn variant_def(hir: &Hir, types: &TypeResolutions, variant_id: HirId) -> VariantDef {
    let variant_node = hir.variant(variant_id);
    let field_tys = match &variant_node.payload {
        VariantPayload::Unit => Vec::new(),
        VariantPayload::Type(_) => {
            let declared = types.ty(variant_id).unwrap_or_else(|| {
                panic!(
                    "collect_adt_defs: {variant_id:?}'s Type(_) payload has no recorded type -- \
                     collect_enum is expected to record it at the variant's own id"
                )
            });
            vec![declared]
        }
        VariantPayload::Record(fields) => fields
            .iter()
            .map(|&field_id| {
                types.ty(field_id).unwrap_or_else(|| {
                    panic!(
                        "collect_adt_defs: {field_id:?} has no recorded type -- collect_fields is \
                         expected to record every field's declared type"
                    )
                })
            })
            .collect(),
    };
    VariantDef { field_tys }
}

#[cfg(test)]
mod tests {
    use super::*;

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
    fn collects_a_structs_field_types_in_declaration_order() {
        let (hir, mut tcx, types) =
            crate::testing::typecheck_only("struct S { a: i8, b: i32 }\nfun f() {}");
        let def = find_struct_def(&hir, "S");
        let adts = collect_adt_defs(&hir, &types);
        let Some(AdtDef::Struct { fields, .. }) = adts.get(&def) else {
            panic!("expected an AdtDef::Struct for {def:?}");
        };
        let i8_ty = tcx.mk_prim(crate::nameres::PrimTy::I8);
        let i32_ty = tcx.mk_prim(crate::nameres::PrimTy::I32);
        assert_eq!(fields, &[i8_ty, i32_ty]);
    }

    #[test]
    fn collects_an_enums_variant_payloads_by_kind() {
        let (hir, _tcx, types) =
            crate::testing::typecheck_only("enum E { A, B: i32, C: { x: i64 } }\nfun f() {}");
        let def = find_enum_def(&hir, "E");
        let adts = collect_adt_defs(&hir, &types);
        let Some(AdtDef::Enum { variants, .. }) = adts.get(&def) else {
            panic!("expected an AdtDef::Enum for {def:?}");
        };
        assert_eq!(variants.len(), 3);
        assert!(variants[0].field_tys.is_empty(), "A is a unit variant");
        assert_eq!(
            variants[1].field_tys.len(),
            1,
            "B: i32 has one payload field"
        );
        assert_eq!(
            variants[2].field_tys.len(),
            1,
            "C: {{ x: i64 }} has one record field"
        );
    }
}
