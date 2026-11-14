use std::collections::HashMap;

use crate::hir::{DefId, Hir, HirId, OwnerNode, Res, TyDef, Type};
use crate::typeck::results::TypeResolutions;
use crate::typeck::ty::ctx::TyCtx;
use crate::typeck::ty::visitor::{Subst, subst_ty};
use crate::typeck::ty::{Ty, TyKind};

pub type VtableKey = (Ty, DefId, Vec<Ty>);

pub struct VtableInfo {
    pub methods: Vec<Option<DefId>>,
    pub block: DefId,

    pub extend_generics: Vec<HirId>,
}

pub(crate) fn collect_vtables(
    hir: &Hir,
    tcx: &mut TyCtx,
    types: &TypeResolutions,
) -> HashMap<VtableKey, VtableInfo> {
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

        let trait_args = trait_type_arguments(hir, tcx, types, extend, self_ty, trait_def);
        out.entry((self_ty, trait_def, trait_args.clone()))
            .or_insert(VtableInfo {
                methods: method_impls,
                block: extend_def,
                extend_generics: extend.extend_generics.clone(),
            });
    }
    out
}

/// Returns the complete argument list an `extend` block gives its trait: the explicit arguments
/// followed by each associated type's definition or default.
fn trait_type_arguments(
    hir: &Hir,
    tcx: &mut TyCtx,
    types: &TypeResolutions,
    extend: &crate::hir::Extend,
    self_ty: Ty,
    trait_def: DefId,
) -> Vec<Ty> {
    let mut args: Vec<Ty> = extend
        .trait_generics
        .iter()
        .map(|&id| types.ty(id.into()))
        .collect::<Option<Vec<_>>>()
        .unwrap_or_default();

    for associated in &hir.trait_(trait_def).associated_types {
        if let Some(definition) = extend
            .associated_types
            .iter()
            .find(|definition| definition.name.text == associated.name.text)
        {
            if let Some(ty) = types.ty(definition.ty.into()) {
                args.push(ty);
            }
            continue;
        }

        if let Some(default) = associated.default
            && let Some(ty) = types.ty(default.into())
        {
            let substitution = Subst {
                generics: HashMap::new(),
                self_ty: Some(self_ty),
            };
            args.push(subst_ty(tcx, ty, &substitution));
        }
    }

    args
}

/// Returns the implementation block for `trait_` that applies to `concrete`, with the bindings
/// that match its header's generic parameters to the concrete type.
/// A block whose header is already concrete is preferred over a generic block.
pub(crate) fn find_vtable<'a>(
    vtables: &'a HashMap<VtableKey, VtableInfo>,
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

        let vtables = collect_vtables(&hir, &mut tcx, &types);
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

        let vtables = collect_vtables(&hir, &mut tcx, &types);
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

        let vtables = collect_vtables(&hir, &mut tcx, &types);
        assert!(!vtables.contains_key(&(i32_ty, trait_def, Vec::new())));
    }
    fn generic_of(hir: &Hir, def: DefId, index: usize) -> HirId {
        match hir.def(def) {
            OwnerNode::Struct(struct_) => struct_.generics[index],
            OwnerNode::Enum(enum_) => enum_.generics[index],
            OwnerNode::Trait(trait_) => trait_.generics[index],
            _ => panic!("the definition declares no generics"),
        }
    }

    #[test]
    fn a_generic_extend_block_matches_a_concrete_instance() {
        let (hir, mut tcx, types) = crate::testing::typecheck_only(
            "trait Greet { fun greet(&self) -> i32; }
             struct Box<T> { value: T }
             extend<T> Box<T> with Greet { fun greet(&self) -> i32 { return 1; } }
             fun f() {}",
        );
        let box_def = find_struct_def(&hir, "Box");
        let trait_def = find_trait_def(&hir, "Greet");
        let i32_ty = tcx.mk_prim(crate::nameres::PrimTy::I32);
        let concrete = tcx.mk_adt(box_def, vec![i32_ty]);

        let vtables = collect_vtables(&hir, &mut tcx, &types);
        let (info, bindings) = find_vtable(&vtables, &mut tcx, concrete, trait_def, &[])
            .expect("the generic extend block applies to Box<i32>");

        assert_eq!(info.methods.len(), 1);
        assert_eq!(bindings.len(), 1, "Box's parameter is bound by the match");
    }

    #[test]
    fn type_patterns_match_every_composite_shape() {
        let (hir, mut tcx, _types) = crate::testing::typecheck_only(
            "struct Box<T> { value: T }
             fun f() {}",
        );
        let box_def = find_struct_def(&hir, "Box");
        let generic = generic_of(&hir, box_def, 0);
        let i32_ty = tcx.mk_prim(crate::nameres::PrimTy::I32);
        let bool_ty = tcx.mk_prim(crate::nameres::PrimTy::Bool);

        let generic_ty = tcx.mk_generic(generic);
        let pattern_adt = tcx.mk_adt(box_def, vec![generic_ty]);
        let concrete_adt = tcx.mk_adt(box_def, vec![i32_ty]);
        let mut bindings = HashMap::new();
        assert!(match_type_pattern(
            &tcx,
            pattern_adt,
            concrete_adt,
            &mut bindings
        ));
        assert_eq!(bindings.get(&generic), Some(&i32_ty));

        let repeated = tcx.mk_tuple(vec![generic_ty, generic_ty]);
        let distinct = tcx.mk_tuple(vec![i32_ty, bool_ty]);
        assert!(!match_type_pattern(
            &tcx,
            repeated,
            distinct,
            &mut HashMap::new()
        ));

        let shared_ref = tcx.mk_ref(i32_ty, crate::ast::Mutability::Immutable);
        let mutable_ref = tcx.mk_ref(i32_ty, crate::ast::Mutability::Mutable);
        assert!(!match_type_pattern(
            &tcx,
            shared_ref,
            mutable_ref,
            &mut HashMap::new()
        ));
        assert!(match_type_pattern(
            &tcx,
            shared_ref,
            shared_ref,
            &mut HashMap::new()
        ));

        let iso = tcx.mk_iso(i32_ty);
        assert!(match_type_pattern(&tcx, iso, iso, &mut HashMap::new()));

        let array = tcx.mk_array(i32_ty, Some(3));
        let longer_array = tcx.mk_array(i32_ty, Some(4));
        assert!(match_type_pattern(&tcx, array, array, &mut HashMap::new()));
        assert!(!match_type_pattern(
            &tcx,
            array,
            longer_array,
            &mut HashMap::new()
        ));

        let function = tcx.mk_fun(vec![i32_ty], Some(bool_ty));
        let other_function = tcx.mk_fun(vec![i32_ty], Some(i32_ty));
        let unit_function = tcx.mk_fun(vec![i32_ty], None);
        assert!(match_type_pattern(
            &tcx,
            function,
            function,
            &mut HashMap::new()
        ));
        assert!(!match_type_pattern(
            &tcx,
            function,
            other_function,
            &mut HashMap::new()
        ));
        assert!(!match_type_pattern(
            &tcx,
            function,
            unit_function,
            &mut HashMap::new()
        ));
        assert!(match_type_pattern(
            &tcx,
            unit_function,
            unit_function,
            &mut HashMap::new()
        ));
    }

    #[test]
    fn unsized_trait_arguments_are_found_through_references_and_owned_pointers() {
        let (hir, mut tcx, _types) = crate::testing::typecheck_only(
            "trait Show { }
             struct S { value: i32 }
             fun f() {}",
        );
        let trait_def = find_trait_def(&hir, "Show");
        let i32_ty = tcx.mk_prim(crate::nameres::PrimTy::I32);
        let dyn_ty = tcx.mk_dyn(trait_def, vec![i32_ty]);
        let shared = tcx.mk_ref(dyn_ty, crate::ast::Mutability::Immutable);
        let owned = tcx.mk_iso(dyn_ty);

        assert_eq!(trait_args_of_unsized_ty(&tcx, dyn_ty), vec![i32_ty]);
        assert_eq!(trait_args_of_unsized_ty(&tcx, shared), vec![i32_ty]);
        assert_eq!(trait_args_of_unsized_ty(&tcx, owned), vec![i32_ty]);
        let plain_ref = tcx.mk_ref(i32_ty, crate::ast::Mutability::Immutable);
        assert_eq!(trait_args_of_unsized_ty(&tcx, plain_ref), Vec::new());
    }
}
