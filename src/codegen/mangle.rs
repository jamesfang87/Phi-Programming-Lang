use crate::ast::Mutability;
use crate::hir::{DefId, Hir, OwnerNode};
use crate::mir::{AnyMode, Instance};
use crate::session::Session;
use crate::typeck::ty::ctx::TyCtx;
use crate::typeck::ty::{Ty, TyKind};

pub fn mangle(hir: &Hir, session: &Session, tcx: &TyCtx, instance: &Instance) -> String {
    let mut name = ancestor_path(hir, session, instance.def).join("_");

    for &arg in &instance.args {
        name.push('_');
        name.push_str(&mangle_ty(hir, session, tcx, arg));
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

fn mangle_ty(hir: &Hir, session: &Session, tcx: &TyCtx, ty: Ty) -> String {
    match tcx.kind(ty).clone() {
        TyKind::Primitive(prim) => format!("{prim:?}"),
        TyKind::Adt { def, args } => join_args(&leaf(hir, session, def), &args, hir, session, tcx),
        TyKind::Dyn { trait_, args } => join_args(
            &format!("dyn_{}", leaf(hir, session, trait_)),
            &args,
            hir,
            session,
            tcx,
        ),
        TyKind::Ref { base, mutability } => {
            let prefix = if mutability == Mutability::Mutable {
                "refmut_"
            } else {
                "ref_"
            };
            format!("{prefix}{}", mangle_ty(hir, session, tcx, base))
        }
        TyKind::Any(base) => format!("any_{}", mangle_ty(hir, session, tcx, base)),
        TyKind::Iso(base) => format!("iso_{}", mangle_ty(hir, session, tcx, base)),
        TyKind::Tuple(elems) => join_args("tuple", &elems, hir, session, tcx),
        TyKind::Array { elem, .. } => format!("array_{}", mangle_ty(hir, session, tcx, elem)),
        TyKind::Fun { params, ret } => {
            let params = join_args("", &params, hir, session, tcx);
            let ret = ret.map_or_else(|| "unit".to_string(), |r| mangle_ty(hir, session, tcx, r));
            format!("fn{params}_{ret}")
        }
        TyKind::Unit => "unit".to_string(),
        TyKind::Never => "never".to_string(),
        TyKind::Error => "error".to_string(),
        TyKind::Var(_) | TyKind::Generic(_) | TyKind::SelfTy(_) => panic!(
            "codegen::mangle: {ty:?} is still unresolved; mangle is only meaningful after \
             mir::monomorphize has run"
        ),
    }
}

fn join_args(head: &str, args: &[Ty], hir: &Hir, session: &Session, tcx: &TyCtx) -> String {
    if args.is_empty() {
        return head.to_string();
    }
    let rendered: Vec<String> = args
        .iter()
        .map(|&a| mangle_ty(hir, session, tcx, a))
        .collect();
    format!("{head}_{}", rendered.join("_"))
}

/// The definition's written name, sanitized into a valid symbol fragment.
fn leaf(hir: &Hir, session: &Session, def: DefId) -> String {
    sanitize(&def_name(hir, session, def))
}

fn ancestor_path(hir: &Hir, session: &Session, def: DefId) -> Vec<String> {
    let mut chain = Vec::new();
    let mut current = Some(def);
    while let Some(id) = current {
        chain.push(leaf(hir, session, id));
        current = hir.parent(id);
    }
    chain.reverse();
    chain
}

fn def_name(hir: &Hir, session: &Session, def: DefId) -> String {
    match hir.def(def) {
        OwnerNode::Module(m) => m
            .path
            .segments
            .last()
            .map(|seg| session.resolve(seg.text).to_string())
            .unwrap_or_else(|| "crate".to_string()),
        OwnerNode::Function(f) => session.resolve(f.name.text).to_string(),
        OwnerNode::Struct(s) => session.resolve(s.name.text).to_string(),
        OwnerNode::Enum(e) => session.resolve(e.name.text).to_string(),
        OwnerNode::Trait(t) => session.resolve(t.name.text).to_string(),
        OwnerNode::Extend(_) => format!("extend{}", def.index()),
        OwnerNode::Closure(_) => format!("closure{}", def.index()),
    }
}

fn sanitize(s: &str) -> String {
    s.chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '_' {
                c
            } else {
                '_'
            }
        })
        .collect()
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
        let (hir, tcx, _types, _mir, instances) = lower_to_mir(src);
        let session = crate::testing::session();
        instances
            .keys()
            .map(|instance| (instance.clone(), mangle(&hir, session, &tcx, instance)))
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
        let _ = &mir;
        let def = first_function(&hir);
        let instance = Instance {
            def,
            any_mode: None,
            args: Vec::new(),
            self_ty: None,
        };
        let session = crate::testing::session();
        assert_eq!(
            mangle(&hir, session, &tcx, &instance),
            mangle(&hir, session, &tcx, &instance)
        );
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
        let (hir, mut tcx, _types, _mir, _instances) = lower_to_mir(
            "struct Foo { public a: i32 }
             trait Marker {}
             fun f() {}",
        );
        let session = crate::testing::session();
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

        assert_eq!(mangle_ty(&hir, session, &tcx, i32_ty), "I32");
        assert_eq!(mangle_ty(&hir, session, &tcx, foo_ty), "Foo");
        assert_eq!(mangle_ty(&hir, session, &tcx, ref_i32), "ref_I32");
        assert_eq!(mangle_ty(&hir, session, &tcx, refmut_i32), "refmut_I32");
        assert_eq!(mangle_ty(&hir, session, &tcx, any_i32), "any_I32");
        assert_eq!(mangle_ty(&hir, session, &tcx, iso_i32), "iso_I32");
        assert_eq!(mangle_ty(&hir, session, &tcx, tuple), "tuple_I32_Foo");
        assert_eq!(mangle_ty(&hir, session, &tcx, array), "array_I32");
        assert_eq!(mangle_ty(&hir, session, &tcx, fun), "fn_I32_unit");
        assert_eq!(mangle_ty(&hir, session, &tcx, dyn_marker), "dyn_Marker");
        assert_eq!(mangle_ty(&hir, session, &tcx, unit), "unit");
        assert_eq!(mangle_ty(&hir, session, &tcx, never), "never");
        assert_eq!(mangle_ty(&hir, session, &tcx, error), "error");
    }

    #[test]
    fn an_any_mode_instance_carries_the_mode_in_its_name() {
        let (hir, tcx, _types, _mir, _instances) = lower_to_mir("fun f() {}");
        let session = crate::testing::session();
        let def = first_function(&hir);
        let name = |any_mode| {
            mangle(
                &hir,
                session,
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
        let (hir, mut tcx, _types, _mir, _instances) = lower_to_mir("fun f() {}");
        let generic = tcx.mk_generic(crate::hir::DefId::from_usize(0).owner_id());

        mangle_ty(&hir, crate::testing::session(), &tcx, generic);
    }
}
