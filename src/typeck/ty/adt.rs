use std::collections::{HashMap, HashSet};

use crate::hir::{DefId, Enum, Hir, HirId, OwnerNode, Struct, VariantPayload};
use crate::typeck::results::TypeResolutions;
use crate::typeck::ty::{Ty, TyKind};

#[derive(Debug)]
pub enum AdtDef {
    Struct {
        generics: Vec<HirId>,
        fields: Vec<Ty>,
    },
    Enum {
        generics: Vec<HirId>,

        variants: Vec<Vec<Ty>>,
    },
}

pub(crate) fn collect_adt_defs(hir: &Hir, types: &TypeResolutions) -> HashMap<DefId, AdtDef> {
    let mut adts = HashMap::new();
    for def_id in hir.def_ids() {
        let adt = match hir.def(def_id) {
            OwnerNode::Struct(struct_) => struct_adt_def(struct_, types),
            OwnerNode::Enum(enum_) => enum_adt_def(enum_, hir, types),
            _ => continue,
        };
        adts.insert(def_id, adt);
    }
    adts
}

fn struct_adt_def(struct_: &Struct, types: &TypeResolutions) -> AdtDef {
    let fields = struct_
        .fields
        .iter()
        .map(|&field_id| recorded_field_ty(types, field_id))
        .collect();
    AdtDef::Struct {
        generics: struct_.generics.clone(),
        fields,
    }
}

fn enum_adt_def(enum_: &Enum, hir: &Hir, types: &TypeResolutions) -> AdtDef {
    let variants = enum_
        .variants
        .iter()
        .map(|&variant_id| variant_field_tys(hir, types, variant_id))
        .collect();
    AdtDef::Enum {
        generics: enum_.generics.clone(),
        variants,
    }
}

fn recorded_field_ty(types: &TypeResolutions, id: HirId) -> Ty {
    types.ty(id).unwrap_or_else(|| {
        panic!(
            "collect_adt_defs: {id:?} has no recorded type -- collect_fields is expected to \
             record every field's declared type"
        )
    })
}

pub(crate) fn infinitely_sized_adts(
    tcx: &crate::typeck::ty::ctx::TyCtx,
    adts: &HashMap<DefId, AdtDef>,
) -> Vec<DefId> {
    let graph = value_edge_graph(tcx, adts);
    let cyclic = cyclic_adts(&graph);
    adts_reaching(&graph, &cyclic)
}

fn value_edge_graph(
    tcx: &crate::typeck::ty::ctx::TyCtx,
    adts: &HashMap<DefId, AdtDef>,
) -> HashMap<DefId, Vec<DefId>> {
    adts.iter()
        .map(|(&def, adt)| (def, value_edges(tcx, adt)))
        .collect()
}

fn value_edges(tcx: &crate::typeck::ty::ctx::TyCtx, adt: &AdtDef) -> Vec<DefId> {
    let mut edges = Vec::new();
    match adt {
        AdtDef::Struct { fields, .. } => {
            for &field in fields {
                collect_adts(tcx, field, &mut edges);
            }
        }
        AdtDef::Enum { variants, .. } => {
            for variant in variants {
                for &field in variant {
                    collect_adts(tcx, field, &mut edges);
                }
            }
        }
    }
    edges
}

fn cyclic_adts(graph: &HashMap<DefId, Vec<DefId>>) -> Vec<DefId> {
    graph
        .keys()
        .copied()
        .filter(|&node| {
            graph
                .get(&node)
                .into_iter()
                .flatten()
                .copied()
                .any(|succ| reaches(graph, succ, node))
        })
        .collect()
}

fn adts_reaching(graph: &HashMap<DefId, Vec<DefId>>, targets: &[DefId]) -> Vec<DefId> {
    graph
        .keys()
        .copied()
        .filter(|&node| targets.iter().any(|&target| reaches(graph, node, target)))
        .collect()
}

fn reaches(graph: &HashMap<DefId, Vec<DefId>>, from: DefId, target: DefId) -> bool {
    let mut stack = vec![from];
    let mut seen = HashSet::new();
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
}

fn collect_adts(tcx: &crate::typeck::ty::ctx::TyCtx, ty: Ty, out: &mut Vec<DefId>) {
    match tcx.kind(ty) {
        TyKind::Adt { def, .. } => out.push(*def),
        TyKind::Tuple(elems) => {
            for &elem in elems {
                collect_adts(tcx, elem, out);
            }
        }
        TyKind::Array { elem, .. } => collect_adts(tcx, *elem, out),

        _ => {}
    }
}

fn variant_field_tys(hir: &Hir, types: &TypeResolutions, variant_id: HirId) -> Vec<Ty> {
    let variant_node = hir.variant(variant_id);
    match &variant_node.payload {
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
            .map(|&field_id| recorded_field_ty(types, field_id))
            .collect(),
    }
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
        assert!(variants[0].is_empty(), "A is a unit variant");
        assert_eq!(variants[1].len(), 1, "B: i32 has one payload field");
        assert_eq!(variants[2].len(), 1, "C: {{ x: i64 }} has one record field");
    }
}
