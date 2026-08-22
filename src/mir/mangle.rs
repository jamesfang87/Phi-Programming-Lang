//! Name mangling: turning one monomorphized [`Instance`] into a unique, stable, linker-safe
//! symbol name.
//!
//! [`mangle`] builds an underscore-joined path from the instance's definition up through its
//! enclosing modules (readable, but not load-bearing for uniqueness on its own, since escaping a
//! type's rendering to `[A-Za-z0-9_]` is lossy in principle -- two distinct types could in theory
//! render to the same escaped string), then appends a fixed-width FNV-1a hash of the instance's
//! own `Debug` representation, which is exact: `Instance` interns every `Ty` it carries, so its
//! `Debug` output already distinguishes any two instances `PartialEq`/`Hash` would.

use crate::ast::Mutability;
use crate::ast::interner::Interner;
use crate::hir::{DefId, Hir, OwnerNode};
use crate::mir::{AnyMode, Instance};
use crate::typeck::ty::{Ty, TyKind};
use crate::typeck::tyctx::TyCtx;

/// Builds `instance`'s symbol name: a readable path, its generic arguments and `any`-mode (if
/// any), and a hash suffix guaranteeing uniqueness. Only `[A-Za-z0-9_]` ever appears in the
/// result, matching what an eventual object-file symbol needs.
pub fn mangle(hir: &Hir, tcx: &TyCtx, instance: &Instance) -> String {
    let mut name = ancestor_path(hir, instance.def).join("_");

    for &arg in &instance.args {
        name.push('_');
        name.push_str(&mangle_ty(hir, tcx, arg));
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

/// `def`'s own name, followed by each ancestor's, from the root module down -- `Hir::parent`
/// walked all the way up, then reversed.
fn ancestor_path(hir: &Hir, def: DefId) -> Vec<String> {
    let mut chain = Vec::new();
    let mut current = Some(def);
    while let Some(id) = current {
        chain.push(escape(&def_name(hir, id)));
        current = hir.parent(id);
    }
    chain.reverse();
    chain
}

/// A human-readable name for `def`, for the mangled name's readable prefix. Not required to be
/// unique on its own -- see the module docs -- so an `extend` block (which the language gives no
/// name of its own) and a closure (likewise) get a serviceable placeholder rather than a real
/// lookup: `def`'s own numeric index is already globally unique, which is all this needs from
/// them.
fn def_name(hir: &Hir, def: DefId) -> String {
    match hir.def(def) {
        OwnerNode::Module(m) => m
            .path
            .segments
            .last()
            .map(|seg| Interner::resolve(seg.text).to_string())
            .unwrap_or_else(|| "crate".to_string()),
        OwnerNode::Function(f) => Interner::resolve(f.name.text).to_string(),
        OwnerNode::Struct(s) => Interner::resolve(s.name.text).to_string(),
        OwnerNode::Enum(e) => Interner::resolve(e.name.text).to_string(),
        OwnerNode::Trait(t) => Interner::resolve(t.name.text).to_string(),
        OwnerNode::Extend(_) => format!("extend{}", def.index()),
        OwnerNode::Closure(_) => format!("closure{}", def.index()),
    }
}

/// Replaces every character that is not `[A-Za-z0-9_]` with `_`.
fn escape(s: &str) -> String {
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

/// A structural, escaped rendering of `ty`, for a generic argument's contribution to the
/// mangled name. Panics on `Generic`/`SelfTy`/`Var`: an [`Instance`] this is called on is
/// expected to already be fully concrete, `mir::monomorphize`'s whole job.
fn mangle_ty(hir: &Hir, tcx: &TyCtx, ty: Ty) -> String {
    match tcx.kind(ty).clone() {
        TyKind::Primitive(prim) => format!("{prim:?}"),
        TyKind::Adt { def, args } => join_args(&def_name(hir, def), &args, hir, tcx),
        TyKind::Dyn { trait_, args } => {
            join_args(&format!("dyn_{}", def_name(hir, trait_)), &args, hir, tcx)
        }
        TyKind::Ref { base, mutability } => {
            let prefix = if mutability == Mutability::Mutable {
                "refmut_"
            } else {
                "ref_"
            };
            format!("{prefix}{}", mangle_ty(hir, tcx, base))
        }
        TyKind::Any(base) => format!("any_{}", mangle_ty(hir, tcx, base)),
        TyKind::Iso(base) => format!("iso_{}", mangle_ty(hir, tcx, base)),
        TyKind::Tuple(elems) => join_args("tuple", &elems, hir, tcx),
        TyKind::Array { elem, .. } => format!("array_{}", mangle_ty(hir, tcx, elem)),
        TyKind::Fun { params, ret } => {
            let params = join_args("", &params, hir, tcx);
            let ret = ret.map_or_else(|| "unit".to_string(), |r| mangle_ty(hir, tcx, r));
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

fn join_args(head: &str, args: &[Ty], hir: &Hir, tcx: &TyCtx) -> String {
    if args.is_empty() {
        return head.to_string();
    }
    let rendered: Vec<String> = args.iter().map(|&a| mangle_ty(hir, tcx, a)).collect();
    format!("{head}_{}", rendered.join("_"))
}

/// A 64-bit FNV-1a hash of `instance`'s `Debug` representation. `Instance` interns every `Ty`
/// it holds, so two instances that are not `PartialEq` always render different `Debug` text,
/// making this exact rather than merely probabilistic (modulo ordinary hash collisions, which a
/// 64-bit digest makes vanishingly unlikely).
fn hash_instance(instance: &Instance) -> u64 {
    fnv1a(format!("{instance:?}").as_bytes())
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
    use crate::testing::{first_function, lower_mir_src, resolve_src};

    fn mangled(src: &str) -> Vec<(Instance, String)> {
        let (hir, tcx, _types, instances) = lower_mir_src(src);
        instances
            .keys()
            .map(|instance| (instance.clone(), mangle(&hir, &tcx, instance)))
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
        let hir = resolve_src("fun f() {}");
        crate::diagnostics::DiagCtx::clear();
        let checked = crate::typeck::check(&hir);
        let crate::typeck::TypeckOutput { tcx, .. } = checked;
        let def = first_function(&hir);
        let instance = Instance {
            def,
            any_mode: None,
            args: Vec::new(),
        };
        assert_eq!(mangle(&hir, &tcx, &instance), mangle(&hir, &tcx, &instance));
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
}
