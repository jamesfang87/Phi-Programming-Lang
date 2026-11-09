use std::collections::HashMap;

use crate::hir::{DefId, Hir, HirId, OwnerNode, Res, TyDef, Type};
use crate::typeck::results::TypeResolutions;
use crate::typeck::ty::ctx::TyCtx;
use crate::typeck::ty::visitor::{Subst, subst_ty};
use crate::typeck::ty::{Ty, TyKind};

pub struct VtableInfo {
    pub methods: Vec<Option<DefId>>,
    pub block: DefId,

    pub extend_generics: Vec<HirId>,
}

pub(crate) fn collect_vtables(
    hir: &Hir,
    types: &TypeResolutions,
) -> HashMap<(Ty, DefId, Vec<Ty>), VtableInfo> {
    let mut out = HashMap::new();
    for extend_def in hir.def_ids() {
        // TODO: This is exactly the kind of logic which should be extracted into a separate function
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

        // TODO: This can also be refactored out; we are basically going thru the methods in the trait
        // and finding the corresponding method in the extend header that implements the method
        // in the trait
        // good name could be find_trait_fun_impls, but this is a mid name
        let method_impls = trait_node
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

        // TODO: this can also be refactored out into "get_trait_type_args"
        let trait_args: Option<Vec<Ty>> = extend
            .trait_generics
            .iter()
            .map(|&id| types.ty(id.into()))
            .collect();

        let trait_args = trait_args.unwrap_or_default();
        out.entry((self_ty, trait_def, trait_args.clone()))
            .or_insert(VtableInfo {
                methods: method_impls,
                block: extend_def,
                extend_generics: extend.extend_generics.clone(),
            });
    }
    out
}

/// Returns the implementation block for `trait_` that applies to `concrete`, with the bindings
/// that match its header's generic parameters to the concrete type.
/// A block whose header is already concrete is preferred over a generic block.
pub(crate) fn find_vtable<'a>(
    vtables: &'a HashMap<(Ty, DefId, Vec<Ty>), VtableInfo>,
    tcx: &mut TyCtx,
    concrete: Ty,
    trait_: DefId,
    trait_args: &[Ty],
) -> Option<(&'a VtableInfo, HashMap<HirId, Ty>)> {
    if let Some(info) = vtables.get(&(concrete, trait_, trait_args.to_vec())) {
        return Some((info, HashMap::new()));
    }

    for ((pattern, trait_def, declared_args), info) in vtables {
        if *trait_def != trait_ || declared_args.len() != trait_args.len() {
            continue;
        }
        let mut bindings = HashMap::new();
        if !match_type_pattern(tcx, *pattern, concrete, &mut bindings) {
            continue;
        }
        let substitution = Subst {
            generics: bindings.clone(),
            self_ty: None,
        };
        let matches = declared_args
            .iter()
            .zip(trait_args)
            .all(|(&declared, &requested)| subst_ty(tcx, declared, &substitution) == requested);
        if matches {
            return Some((info, bindings));
        }
    }
    None
}

/// Returns the type arguments of the `dyn` type that `ty` unsizes to, looking through any
/// references or owned pointers.
pub(crate) fn trait_args_of_unsized_ty(tcx: &TyCtx, ty: Ty) -> Vec<Ty> {
    let mut current = ty;
    loop {
        current = match *tcx.kind(current) {
            TyKind::Ref { base, .. } | TyKind::Iso(base) => base,
            _ => break,
        };
    }
    match tcx.kind(current) {
        TyKind::Dyn { args, .. } => args.clone(),
        _ => Vec::new(),
    }
}

/// Returns whether `pattern` matches `concrete`, recording what each generic parameter in
/// `pattern` stands for in `bindings`.
fn match_type_pattern(
    tcx: &TyCtx,
    pattern: Ty,
    concrete: Ty,
    bindings: &mut HashMap<HirId, Ty>,
) -> bool {
    match (tcx.kind(pattern).clone(), tcx.kind(concrete).clone()) {
        (TyKind::Generic(param), _) => match bindings.get(&param) {
            Some(bound) => *bound == concrete,
            None => {
                bindings.insert(param, concrete);
                true
            }
        },
        (TyKind::Adt { def: a, args: x }, TyKind::Adt { def: b, args: y }) => {
            a == b && match_ty_list(tcx, &x, &y, bindings)
        }
        (
            TyKind::Ref {
                base: a,
                mutability: m,
            },
            TyKind::Ref {
                base: b,
                mutability: n,
            },
        ) => m == n && match_type_pattern(tcx, a, b, bindings),
        (TyKind::Iso(a), TyKind::Iso(b)) => match_type_pattern(tcx, a, b, bindings),
        (TyKind::Tuple(a), TyKind::Tuple(b)) => match_ty_list(tcx, &a, &b, bindings),
        (
            TyKind::Array {
                elem: a,
                len: len_a,
            },
            TyKind::Array {
                elem: b,
                len: len_b,
            },
        ) => len_a == len_b && match_type_pattern(tcx, a, b, bindings),
        (
            TyKind::Fun {
                params: a,
                ret: r_a,
            },
            TyKind::Fun {
                params: b,
                ret: r_b,
            },
        ) => match_ty_list(tcx, &a, &b, bindings) && match_ret(tcx, r_a, r_b, bindings),
        _ => pattern == concrete,
    }
}

/// Returns whether two type lists are the same length and match pairwise, binding generics as it
/// goes.
fn match_ty_list(tcx: &TyCtx, xs: &[Ty], ys: &[Ty], bindings: &mut HashMap<HirId, Ty>) -> bool {
    xs.len() == ys.len()
        && xs
            .iter()
            .zip(ys.iter())
            .all(|(&p, &q)| match_type_pattern(tcx, p, q, bindings))
}

/// Returns whether two optional return types match, treating two absent returns as equal.
fn match_ret(tcx: &TyCtx, a: Option<Ty>, b: Option<Ty>, bindings: &mut HashMap<HirId, Ty>) -> bool {
    match (a, b) {
        (Some(a), Some(b)) => match_type_pattern(tcx, a, b, bindings),
        (None, None) => true,
        (Some(_), None) | (None, Some(_)) => false,
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

    fn find_trait_def(hir: &Hir, name: &str) -> DefId {
        for def_id in hir.def_ids() {
            if let OwnerNode::Trait(trait_) = hir.def(def_id)
                && crate::testing::resolve(trait_.name.text) == name
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
            .get(&(point_ty, trait_def, Vec::new()))
            .expect("Point implements Greet");
        assert_eq!(info.methods.len(), 1);
        assert!(info.methods[0].is_some(), "greet is implemented");
    }

    #[test]
    fn a_type_can_have_one_vtable_per_trait_argument_list() {
        let (hir, mut tcx, types) = crate::testing::typecheck_only(
            "trait Conv<T> { fun conv(&self) -> T; }
                 struct W { v: i32 }
                 extend W with Conv<bool> { fun conv(&self) -> bool { return true; } }
                 extend W with Conv<i32> { fun conv(&self) -> i32 { return self.v; } }
                 fun f() {}",
        );
        let w_def = find_struct_def(&hir, "W");
        let trait_def = find_trait_def(&hir, "Conv");
        let w_ty = tcx.mk_adt(w_def, Vec::new());
        let bool_ty = tcx.mk_prim(crate::nameres::PrimTy::Bool);
        let i32_ty = tcx.mk_prim(crate::nameres::PrimTy::I32);

        let vtables = collect_vtables(&hir, &types);
        let bool_impl = vtables
            .get(&(w_ty, trait_def, vec![bool_ty]))
            .expect("W implements Conv<bool>");
        let int_impl = vtables
            .get(&(w_ty, trait_def, vec![i32_ty]))
            .expect("W implements Conv<i32>");

        assert_ne!(
            bool_impl.methods[0], int_impl.methods[0],
            "Conv<bool> and Conv<i32> keep separate method implementations"
        );
    }

    #[test]
    fn a_type_with_no_matching_extend_block_has_no_entry() {
        let (hir, mut tcx, types) = crate::testing::typecheck_only(SRC);
        let trait_def = find_trait_def(&hir, "Greet");
        let i32_ty = tcx.mk_prim(crate::nameres::PrimTy::I32);

        let vtables = collect_vtables(&hir, &types);
        assert!(!vtables.contains_key(&(i32_ty, trait_def, Vec::new())));
    }
}
