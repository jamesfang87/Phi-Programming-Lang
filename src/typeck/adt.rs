use std::collections::HashMap;

use crate::hir::{DefId, Hir, HirId, OwnerNode, VariantPayload};
use crate::typeck::results::TypeResolutions;
use crate::typeck::ty::{Ty, TyKind};

// TODO: what is this used for?
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

// TODO: THERES LITERALLY ANOTHER VARIANT DEF
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

/// The ADTs that cannot have a finite size: those that contain themselves by value, whether
/// directly (`enum List { cons: { tail: List } }`) or through other value fields. A field of
/// `iso T` is a heap indirection, so it breaks the cycle and is not traversed.
pub(crate) fn infinitely_sized_adts(
    tcx: &crate::typeck::tyctx::TyCtx,
    adts: &HashMap<DefId, AdtDef>,
) -> Vec<DefId> {
    let mut graph: HashMap<DefId, Vec<DefId>> = HashMap::new();
    for (&def, adt) in adts {
        let mut edges = Vec::new();
        match adt {
            AdtDef::Struct { fields, .. } => {
                for &field in fields {
                    collect_adts(tcx, field, &mut edges);
                }
            }
            AdtDef::Enum { variants, .. } => {
                for variant in variants {
                    for &field in &variant.field_tys {
                        collect_adts(tcx, field, &mut edges);
                    }
                }
            }
        }
        graph.insert(def, edges);
    }

    let reaches = |from: DefId, target: DefId| -> bool {
        let mut stack = vec![from];
        let mut seen = std::collections::HashSet::new();
        while let Some(node) = stack.pop() {
            if node == target {
                return true;
            }
            if !seen.insert(node) {
                continue;
            }
            if let Some(next) = graph.get(&node) {
                stack.extend(next.iter().copied());
            }
        }
        false
    };

    // A node is on a cycle if one of its value fields can reach it again; a type is infinitely
    // sized if it is on a cycle or reaches one (an `Outer` that owns a recursive `Inner` is
    // itself infinite).
    let cyclic: Vec<DefId> = graph
        .keys()
        .copied()
        .filter(|&node| {
            graph
                .get(&node)
                .into_iter()
                .flatten()
                .copied()
                .any(|succ| reaches(succ, node))
        })
        .collect();
    graph
        .keys()
        .copied()
        .filter(|&node| cyclic.iter().any(|&cycle| reaches(node, cycle)))
        .collect()
}

fn collect_adts(tcx: &crate::typeck::tyctx::TyCtx, ty: Ty, out: &mut Vec<DefId>) {
    match tcx.kind(ty) {
        TyKind::Adt { def, .. } => out.push(*def),
        TyKind::Tuple(elems) => {
            for &elem in elems {
                collect_adts(tcx, elem, out);
            }
        }
        TyKind::Array { elem, .. } => collect_adts(tcx, *elem, out),
        // `iso T` is a heap indirection; references, functions, `dyn`, `any`, generics and
        // primitives never place another ADT directly inside this one.
        _ => {}
    }
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
                && crate::testing::resolve(struct_.name.text) == name
            {
                return def_id;
            }
        }
        panic!("no struct named {name:?} found");
    }

    fn find_enum_def(hir: &Hir, name: &str) -> DefId {
        for def_id in hir.def_ids() {
            if let OwnerNode::Enum(enum_) = hir.def(def_id)
                && crate::testing::resolve(enum_.name.text) == name
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
