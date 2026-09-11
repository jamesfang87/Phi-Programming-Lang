use std::collections::HashMap;

use crate::hir::{DefId, Hir, HirId, OwnerNode, Res, TyDef, Type};
use crate::typeck::results::TypeResolutions;
use crate::typeck::ty::Ty;

pub struct VtableInfo {
    pub methods: Vec<Option<DefId>>,
    pub block: DefId,
    pub trait_args: Vec<Ty>,
    /// The `extend .. with` block's own generic parameters. A call made through the trait's body
    /// has to line its arguments up against these before the method's own, and keeping them here
    /// lets monomorphization do that without reading the block back out of the HIR.
    pub extend_generics: Vec<HirId>,
}

pub(crate) fn collect_vtables(
    hir: &Hir,
    types: &TypeResolutions,
) -> HashMap<(Ty, DefId), VtableInfo> {
    let mut out = HashMap::new();
    for extend_def in hir.def_ids() {
        let OwnerNode::Extend(extend) = hir.def(extend_def) else {
            continue;
        };
        let Some(trait_path) = extend.trait_path.as_ref() else {
            continue;
        };
        let Res::Type(Type::Def(TyDef::Trait(trait_def))) = trait_path.res else {
            continue;
        };
        let Some(self_ty) = types.ty_of_def(extend_def) else {
            continue;
        };

        let trait_node = hir.trait_(trait_def);
        let methods = trait_node
            .functions
            .iter()
            .map(|&trait_method| {
                let method_name = hir.function(trait_method).name.text;
                extend
                    .methods
                    .iter()
                    .copied()
                    .find(|&method| hir.function(method).name.text == method_name)
            })
            .collect();
        let trait_args: Option<Vec<Ty>> = extend
            .trait_generics
            .iter()
            .map(|&id| types.ty(id))
            .collect();

        out.entry((self_ty, trait_def)).or_insert(VtableInfo {
            methods,
            block: extend_def,
            trait_args: trait_args.unwrap_or_default(),
            extend_generics: extend.extend_generics.clone(),
        });
    }
    out
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

    fn find_trait_def(hir: &Hir, name: &str) -> DefId {
        for def_id in hir.def_ids() {
            if let OwnerNode::Trait(trait_) = hir.def(def_id)
                && crate::ast::interner::Interner::resolve(trait_.name.text) == name
            {
                return def_id;
            }
        }
        panic!("no trait named {name:?} found");
    }

    const SRC: &str = "trait Greet { fun greet(&self) -> i32; }
struct Point { x: i32, y: i32 }
extend Point with Greet { fun greet(&self) -> i32 { return self.x; } }
fun f() {}";

    #[test]
    fn resolves_the_impl_method_for_a_concrete_type() {
        let (hir, mut tcx, types) = crate::testing::typecheck_only(SRC);
        let point_def = find_struct_def(&hir, "Point");
        let trait_def = find_trait_def(&hir, "Greet");
        let point_ty = tcx.mk_adt(point_def, Vec::new());

        let vtables = collect_vtables(&hir, &types);
        let info = vtables
            .get(&(point_ty, trait_def))
            .expect("Point implements Greet");
        assert_eq!(info.methods.len(), 1);
        assert!(info.methods[0].is_some(), "greet is implemented");
    }

    #[test]
    fn a_type_with_no_matching_extend_block_has_no_entry() {
        let (hir, mut tcx, types) = crate::testing::typecheck_only(SRC);
        let trait_def = find_trait_def(&hir, "Greet");
        let i32_ty = tcx.mk_prim(crate::nameres::PrimTy::I32);

        let vtables = collect_vtables(&hir, &types);
        assert!(!vtables.contains_key(&(i32_ty, trait_def)));
    }
}
