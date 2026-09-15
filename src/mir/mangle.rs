use crate::ast::Mutability;
use crate::mir::{AnyMode, Instance, Mir};
use crate::typeck::ty::{Ty, TyKind};
use crate::typeck::tyctx::TyCtx;

pub fn mangle(mir: &Mir, tcx: &TyCtx, instance: &Instance) -> String {
    let mut name = mir.def_names.ancestor_path(instance.def).join("_");

    for &arg in &instance.args {
        name.push('_');
        name.push_str(&mangle_ty(mir, tcx, arg));
    }

    if let Some(mode) = instance.any_mode {
        name.push('_');
        name.push_str(match mode {
            AnyMode::Owned => "owned",
            AnyMode::Ref => "ref",
            AnyMode::RefMut => "refmut",
        });
    }

    name.push_str(&format!("_h{:016x}", hash_instance(instance)));
    name
}

fn mangle_ty(mir: &Mir, tcx: &TyCtx, ty: Ty) -> String {
    match tcx.kind(ty).clone() {
        TyKind::Primitive(prim) => format!("{prim:?}"),
        TyKind::Adt { def, args } => join_args(mir.def_names.leaf(def), &args, mir, tcx),
        TyKind::Dyn { trait_, args } => join_args(
            &format!("dyn_{}", mir.def_names.leaf(trait_)),
            &args,
            mir,
            tcx,
        ),
        TyKind::Ref { base, mutability } => {
            let prefix = if mutability == Mutability::Mutable {
                "refmut_"
            } else {
                "ref_"
            };
            format!("{prefix}{}", mangle_ty(mir, tcx, base))
        }
        TyKind::Any(base) => format!("any_{}", mangle_ty(mir, tcx, base)),
        TyKind::Iso(base) => format!("iso_{}", mangle_ty(mir, tcx, base)),
        TyKind::Tuple(elems) => join_args("tuple", &elems, mir, tcx),
        TyKind::Array { elem, .. } => format!("array_{}", mangle_ty(mir, tcx, elem)),
        TyKind::Fun { params, ret } => {
            let params = join_args("", &params, mir, tcx);
            let ret = ret.map_or_else(|| "unit".to_string(), |r| mangle_ty(mir, tcx, r));
            format!("fn{params}_{ret}")
        }
        TyKind::Unit => "unit".to_string(),
        TyKind::Never => "never".to_string(),
        TyKind::Error => "error".to_string(),
        TyKind::Var(_) | TyKind::Generic(_) | TyKind::SelfTy(_) => panic!(
            "mir::mangle: {ty:?} is still unresolved; mangle is only meaningful after \
             mir::monomorphize has run"
        ),
    }
}

fn join_args(head: &str, args: &[Ty], mir: &Mir, tcx: &TyCtx) -> String {
    if args.is_empty() {
        return head.to_string();
    }
    let rendered: Vec<String> = args.iter().map(|&a| mangle_ty(mir, tcx, a)).collect();
    format!("{head}_{}", rendered.join("_"))
}

fn hash_instance(instance: &Instance) -> u64 {
    let mut bytes =
        format!("{:?}", (&instance.def, &instance.any_mode, &instance.args)).into_bytes();
    if let Some(self_ty) = instance.self_ty {
        bytes.extend(format!("{self_ty:?}").into_bytes());
    }
    fnv1a(&bytes)
}

fn fnv1a(bytes: &[u8]) -> u64 {
    const OFFSET_BASIS: u64 = 0xcbf29ce484222325;
    const PRIME: u64 = 0x100000001b3;
    let mut hash = OFFSET_BASIS;
    for &byte in bytes {
        hash ^= u64::from(byte);
        hash = hash.wrapping_mul(PRIME);
    }
    hash
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::nameres::PrimTy;
    use crate::testing::{first_function, lower_to_hir, lower_to_mir, named_def};

    fn mangled(src: &str) -> Vec<(Instance, String)> {
        let (_hir, tcx, _types, mir, instances) = lower_to_mir(src);
        instances
            .keys()
            .map(|instance| (instance.clone(), mangle(&mir, &tcx, instance)))
            .collect()
    }

    #[test]
    fn a_symbol_name_is_a_bare_identifier() {
        let names = mangled("fun add(x: i32, y: i32) -> i32 { return x + y; }");
        for (_, name) in &names {
            assert!(
                name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_'),
                "{name:?} is not a bare identifier"
            );
        }
    }

    #[test]
    fn mangling_the_same_instance_twice_is_stable() {
        let hir = lower_to_hir("fun f() {}");
        crate::testing::clear_diagnostics();
        let checked = crate::typeck::check(crate::testing::session(), &hir);
        let crate::typeck::TypeckOutput { mut tcx, types } = checked;
        let mir = crate::mir::lower::lower(
            crate::testing::session(),
            &hir,
            &mut tcx,
            &types,
            crate::options::Mode::Debug,
        );
        let def = first_function(&hir);
        let instance = Instance {
            def,
            any_mode: None,
            args: Vec::new(),
            self_ty: None,
        };
        assert_eq!(mangle(&mir, &tcx, &instance), mangle(&mir, &tcx, &instance));
    }

    #[test]
    fn two_distinct_generic_instantiations_mangle_differently() {
        let names = mangled(
            "fun identity<T>(x: T) -> T { return x; }
             fun f() -> i32 {
                 let a = identity(1);
                 let b = identity(true);
                 return a;
             }",
        );
        let rendered: std::collections::HashSet<&String> = names.iter().map(|(_, n)| n).collect();
        assert_eq!(
            rendered.len(),
            names.len(),
            "every instance mangles to a distinct name: {names:?}"
        );
    }

    #[test]
    fn every_type_kind_has_its_own_spelling() {
        let (hir, mut tcx, _types, mir, _instances) = lower_to_mir(
            "struct Foo { public a: i32 }
             trait Marker {}
             fun f() {}",
        );
        let foo = named_def(&hir, "Foo");
        let marker = named_def(&hir, "Marker");

        let i32_ty = tcx.mk_prim(PrimTy::I32);
        let foo_ty = tcx.mk_adt(foo, vec![]);
        let ref_i32 = tcx.mk_ref(i32_ty, Mutability::Immutable);
        let refmut_i32 = tcx.mk_ref(i32_ty, Mutability::Mutable);
        let any_i32 = tcx.mk_any(i32_ty);
        let iso_i32 = tcx.mk_iso(i32_ty);
        let tuple = tcx.mk_tuple(vec![i32_ty, foo_ty]);
        let array = tcx.mk_array(i32_ty, Some(3));
        let fun = tcx.mk_fun(vec![i32_ty], None);
        let dyn_marker = tcx.mk_dyn(marker, vec![]);
        let unit = tcx.unit();
        let never = tcx.never();
        let error = tcx.error();

        assert_eq!(mangle_ty(&mir, &tcx, i32_ty), "I32");
        assert_eq!(mangle_ty(&mir, &tcx, foo_ty), "Foo");
        assert_eq!(mangle_ty(&mir, &tcx, ref_i32), "ref_I32");
        assert_eq!(mangle_ty(&mir, &tcx, refmut_i32), "refmut_I32");
        assert_eq!(mangle_ty(&mir, &tcx, any_i32), "any_I32");
        assert_eq!(mangle_ty(&mir, &tcx, iso_i32), "iso_I32");
        assert_eq!(mangle_ty(&mir, &tcx, tuple), "tuple_I32_Foo");
        assert_eq!(mangle_ty(&mir, &tcx, array), "array_I32");
        assert_eq!(mangle_ty(&mir, &tcx, fun), "fn_I32_unit");
        assert_eq!(mangle_ty(&mir, &tcx, dyn_marker), "dyn_Marker");
        assert_eq!(mangle_ty(&mir, &tcx, unit), "unit");
        assert_eq!(mangle_ty(&mir, &tcx, never), "never");
        assert_eq!(mangle_ty(&mir, &tcx, error), "error");
    }

    #[test]
    fn an_any_mode_instance_carries_the_mode_in_its_name() {
        let (hir, tcx, _types, mir, _instances) = lower_to_mir("fun f() {}");
        let def = first_function(&hir);
        let name = |any_mode| {
            mangle(
                &mir,
                &tcx,
                &Instance {
                    def,
                    any_mode,
                    args: Vec::new(),
                    self_ty: None,
                },
            )
        };

        let owned = name(Some(AnyMode::Owned));
        let by_ref = name(Some(AnyMode::Ref));
        let by_ref_mut = name(Some(AnyMode::RefMut));

        assert!(owned.contains("_owned_"), "{owned}");
        assert!(by_ref.contains("_ref_"), "{by_ref}");
        assert!(by_ref_mut.contains("_refmut_"), "{by_ref_mut}");
    }

    #[test]
    #[should_panic(expected = "is still unresolved")]
    fn an_unresolved_type_cannot_be_mangled() {
        let (_hir, mut tcx, _types, mir, _instances) = lower_to_mir("fun f() {}");
        let generic = tcx.mk_generic(crate::hir::DefId::from_usize(0).owner_id());

        mangle_ty(&mir, &tcx, generic);
    }
}
