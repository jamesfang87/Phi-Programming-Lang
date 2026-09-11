//! Definition-level facts a pass needs *after* lowering, snapshotted from the HIR during it.
//!
//! MIR stores [`DefId`]s rather than copies of what they name, so anything that walks a `Mir`
//! eventually asks a question about a definition -- who encloses it, what kind it is, which
//! generic parameters its caller's argument list lines up against. Those questions have two
//! possible answers: reach back into the [`Hir`](crate::hir::Hir) that produced the `Mir`, or read
//! them out of this table. This module exists so post-lowering passes can take the second option,
//! and so [`monomorphize`](crate::mir::monomorphize), [`checks`](crate::mir::checks) and
//! [`codegen`](crate::codegen) can treat `(TyCtx, Mir)` as the whole world they need.
//! [`DefNames`](super::def_names::DefNames) does the same job for names, and for the same reason.
//!
//! What earns a place here: a fact that is *flat* -- an id, a small enum, or a `Vec` of those --
//! and that a post-lowering pass actually reads. Anything richer means the HIR read belongs in
//! [`mir::lower`](crate::mir::lower) and its result belongs in a [`Body`](super::body::Body),
//! not in a second copy of the HIR.

use crate::hir::{DefId, Hir, HirId, OwnerNode};

/// What a definition is, as far as the passes after lowering can tell. Mirrors
/// [`OwnerNode`]'s variants because it is one, minus the payload this table deliberately drops.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum DefKind {
    Module,
    Function,
    Struct,
    Enum,
    Trait,
    Extend,
    Closure,
}

/// Everything one definition contributes to the passes that run after it was lowered.
#[derive(Clone, Debug)]
pub struct DefInfo {
    pub kind: DefKind,
    /// The definition this one is declared inside, or `None` for the crate root module.
    pub parent: Option<DefId>,
    /// The def's own declared generic parameters, in written order. For an `extend .. with` block
    /// these are the block's own (`extend Vec<T>`'s `T`), and for a trait, the trait's.
    pub generics: Vec<HirId>,
    /// For a method a trait declares: that trait, and the method's position in the trait's
    /// declaration order. That position *is* the vtable slot a `dyn` call loads from, so one
    /// lookup answers both "is this a trait method, and of which trait" and "which slot".
    pub trait_method: Option<(DefId, u32)>,
}

/// Every [`DefInfo`], addressable by the [`DefId`] it describes.
pub struct DefInfos {
    /// Indexed by [`DefId::index`] directly: [`Hir::def_ids`] hands out `DefId`s densely and in
    /// arena order, so filling this `Vec` in that same order keeps index and `DefId` in step
    /// without paying for a hash map per lookup.
    infos: Vec<DefInfo>,
}

impl DefInfos {
    fn get(&self, def: DefId) -> &DefInfo {
        self.infos.get(def.index()).unwrap_or_else(|| {
            panic!(
                "DefInfos: {def:?} has no entry -- collect_def_infos writes one for every \
                 hir.def_ids() entry, so this DefId belongs to a different Hir"
            )
        })
    }

    pub fn kind(&self, def: DefId) -> DefKind {
        self.get(def).kind
    }

    pub fn parent(&self, def: DefId) -> Option<DefId> {
        self.get(def).parent
    }

    /// The def's own declared generics. Empty for anything that declares none, including a
    /// closure, whose body mentions only its enclosing definition's parameters.
    pub fn generics(&self, def: DefId) -> &[HirId] {
        &self.get(def).generics
    }

    /// The trait a method was declared by, and its slot in that trait's vtable.
    pub fn trait_method(&self, def: DefId) -> Option<(DefId, u32)> {
        self.get(def).trait_method
    }

    /// The vtable slot of a trait method, `None` for anything else.
    pub fn vtable_slot(&self, def: DefId) -> Option<u32> {
        self.trait_method(def).map(|(_, slot)| slot)
    }
}

pub(crate) fn collect_def_infos(hir: &Hir) -> DefInfos {
    let mut infos: Vec<DefInfo> = hir
        .def_ids()
        .map(|def| DefInfo {
            kind: kind_of(hir.def(def)),
            parent: hir.parent(def),
            generics: declared_generics(hir, def).to_vec(),
            trait_method: None,
        })
        .collect();

    // A method's slot is a fact about its trait's declaration order, so it is read from the trait
    // and written into the method -- the one place where an entry is filled by something other
    // than its own definition.
    for def in hir.def_ids() {
        let OwnerNode::Trait(trait_) = hir.def(def) else {
            continue;
        };
        for (slot, &method) in trait_.functions.iter().enumerate() {
            infos[method.index()].trait_method = Some((def, slot as u32));
        }
    }

    DefInfos { infos }
}

fn kind_of(node: &OwnerNode) -> DefKind {
    match node {
        OwnerNode::Module(_) => DefKind::Module,
        OwnerNode::Function(_) => DefKind::Function,
        OwnerNode::Struct(_) => DefKind::Struct,
        OwnerNode::Enum(_) => DefKind::Enum,
        OwnerNode::Trait(_) => DefKind::Trait,
        OwnerNode::Extend(_) => DefKind::Extend,
        OwnerNode::Closure(_) => DefKind::Closure,
    }
}

/// The generic parameters a definition itself declares. Each owner type names its own list
/// differently, and the two that declare none are spelled out rather than caught by a wildcard,
/// so adding a generic to a third owner type stops compiling until it is handled here.
fn declared_generics(hir: &Hir, def: DefId) -> &[HirId] {
    match hir.def(def) {
        OwnerNode::Function(function) => &function.generics,
        OwnerNode::Struct(struct_) => &struct_.generics,
        OwnerNode::Enum(enum_) => &enum_.generics,
        OwnerNode::Trait(trait_) => &trait_.generics,
        OwnerNode::Extend(extend) => &extend.extend_generics,
        OwnerNode::Module(_) | OwnerNode::Closure(_) => &[],
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::hir::Hir;
    use crate::mir::def_names::collect_def_names;

    fn named(hir: &Hir, name: &str) -> DefId {
        let names = collect_def_names(hir);
        hir.def_ids()
            .find(|&def| names.def_name(def) == name)
            .unwrap_or_else(|| panic!("fixture declares no definition named {name:?}"))
    }

    #[test]
    fn a_trait_method_knows_its_trait_and_its_slot() {
        let hir = crate::testing::lower_to_hir(
            "trait Shape { fun area(&self) -> f64; fun perim(&self) -> f64; }\nfun f() {}",
        );
        let infos = collect_def_infos(&hir);
        let area = named(&hir, "area");
        let perim = named(&hir, "perim");
        let shape = named(&hir, "Shape");

        assert_eq!(infos.trait_method(area), Some((shape, 0)));
        assert_eq!(infos.trait_method(perim), Some((shape, 1)));
        assert_eq!(infos.vtable_slot(area), Some(0));
        assert_eq!(infos.kind(area), DefKind::Function);
        assert_eq!(infos.parent(area), Some(shape));
    }

    #[test]
    fn an_extend_block_and_its_method_are_both_recorded() {
        let hir = crate::testing::lower_to_hir(
            "trait Holder<T> { fun held(&self) -> T; }\nstruct Wrap<T> { v: T }\n\
             extend<T> Wrap<T> with Holder<T> { fun spare(&self) -> T { return self.v; } }",
        );
        let infos = collect_def_infos(&hir);

        let wrap = named(&hir, "Wrap");
        assert_eq!(infos.kind(wrap), DefKind::Struct);
        assert_eq!(infos.generics(wrap).len(), 1, "Wrap<T> declares T");

        let spare = named(&hir, "spare");
        let block = infos
            .parent(spare)
            .expect("spare is written inside a block");
        assert_eq!(infos.kind(block), DefKind::Extend);
        assert_eq!(
            infos.generics(block).len(),
            1,
            "`extend Wrap<T>` contributes its own T to its methods' parameter list"
        );
        assert_eq!(
            infos.trait_method(spare),
            None,
            "a method written in an `extend` block is not a trait's own declaration"
        );

        assert_eq!(
            infos.trait_method(named(&hir, "held")),
            Some((named(&hir, "Holder"), 0))
        );
    }
}
