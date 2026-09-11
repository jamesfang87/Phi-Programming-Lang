use std::collections::HashMap;

use crate::ast::interner::Interner;
use crate::hir::{DefId, Hir, HirId, Node, OwnerNode};

pub struct DefNames {
    leaf: HashMap<DefId, String>,
    ancestor_path: HashMap<DefId, Vec<String>>,
    /// The definition's written name, before [`replace_non_alphanumeric_chars`] sanitizes it for
    /// use in a symbol. Diagnostics print this one, so a type reads as `Vec<T>`, not `Vec_T`.
    display: HashMap<DefId, String>,
    /// Every generic parameter's declared name, keyed by the [`HirId`] the type interner uses to
    /// name it. A type printed as `T` needs the same name the HIR gave it.
    generics: HashMap<HirId, String>,
}

impl DefNames {
    pub fn leaf(&self, def: DefId) -> &str {
        self.leaf.get(&def).unwrap_or_else(|| {
            panic!(
                "DefNames::leaf: {def:?} was never collected -- collect_def_names walks every \
                 hir.def_ids() entry, so this DefId belongs to a different Hir"
            )
        })
    }

    pub fn ancestor_path(&self, def: DefId) -> &[String] {
        self.ancestor_path.get(&def).unwrap_or_else(|| {
            panic!(
                "DefNames::ancestor_path: {def:?} was never collected -- collect_def_names walks \
                 every hir.def_ids() entry, so this DefId belongs to a different Hir"
            )
        })
    }

    /// The definition's written name, for printing in a diagnostic.
    pub fn def_name(&self, def: DefId) -> &str {
        self.display.get(&def).unwrap_or_else(|| {
            panic!(
                "DefNames::def_name: {def:?} was never collected -- collect_def_names walks every \
                 hir.def_ids() entry, so this DefId belongs to a different Hir"
            )
        })
    }

    /// The written name of the generic parameter `id` names.
    pub fn generic_name(&self, id: HirId) -> &str {
        self.generics.get(&id).unwrap_or_else(|| {
            panic!(
                "DefNames::generic_name: {id:?} was never collected -- collect_generic_names walks \
                 every arena node, so this HirId belongs to a different Hir"
            )
        })
    }
}

pub(crate) fn collect_def_names(hir: &Hir) -> DefNames {
    let display: HashMap<DefId, String> = hir
        .def_ids()
        .map(|def_id| (def_id, def_name(hir, def_id)))
        .collect();

    let leaf: HashMap<DefId, String> = display
        .iter()
        .map(|(&def_id, name)| (def_id, replace_non_alphanumeric_chars(name)))
        .collect();

    let ancestor_path = hir
        .def_ids()
        .map(|def_id| {
            let mut chain = Vec::new();
            let mut current = Some(def_id);
            while let Some(id) = current {
                chain.push(leaf[&id].clone());
                current = hir.parent(id);
            }
            chain.reverse();
            (def_id, chain)
        })
        .collect();

    DefNames {
        leaf,
        ancestor_path,
        display,
        generics: collect_generic_names(hir),
    }
}

fn collect_generic_names(hir: &Hir) -> HashMap<HirId, String> {
    hir.def_ids()
        .flat_map(|def| hir.arena(def).nodes.iter())
        .filter_map(|node| match node {
            Node::Generic(generic) => Some((
                node.hir_id(),
                Interner::resolve(generic.name.text).to_string(),
            )),
            _ => None,
        })
        .collect()
}

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

fn replace_non_alphanumeric_chars(s: &str) -> String {
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

    #[test]
    fn leaf_is_the_bare_sanitized_name() {
        let hir = crate::testing::lower_to_hir("struct Point { x: i32 }\nfun f() {}");
        let def = find_struct_def(&hir, "Point");
        let names = collect_def_names(&hir);
        assert_eq!(names.leaf(def), "Point");
    }

    #[test]
    fn ancestor_path_is_the_root_to_def_chain() {
        let hir = crate::testing::lower_to_hir("fun f() {}");
        let def = crate::testing::first_function(&hir);
        let names = collect_def_names(&hir);
        let path = names.ancestor_path(def);
        assert_eq!(path.last().map(String::as_str), Some("f"));
    }

    #[test]
    fn generic_name_is_collected_for_the_id_a_type_carries() {
        let hir = crate::testing::lower_to_hir("fun id<T>(v: T) -> T { return v; }");
        let names = collect_def_names(&hir);
        let function = crate::testing::first_function(&hir);
        let generics = &hir.function(function).generics;
        assert_eq!(generics.len(), 1);
        assert_eq!(names.generic_name(generics[0]), "T");
    }
}
