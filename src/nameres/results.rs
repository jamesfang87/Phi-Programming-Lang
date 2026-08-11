//! The output of AST-level name resolution.

use std::collections::HashMap;

use smallvec::SmallVec;

use crate::ast::{NodeId, Path};
use crate::langitems::AstLangItems;
use crate::nameres::res::Res;

/// Every path written in the program, grouped by the node that owns it, plus the lang items.
///
/// One table serves what the HIR-side resolver split into four. An `ast::Generic` owns its
/// bound paths as further entries in that node's list (in source order). `Self` is an ordinary
/// path since `TyKind::SelfType` was removed. Generics are resolver-internal scope-stack state.
///
/// `lang_items` stays pass output: resolving it is name resolution's job and can only be done
/// while the symbol table exists, but every consumer of it is a later pass.
#[derive(Debug, Default)]
pub struct NameResolutions {
    /// `SmallVec<[_; 2]>` carries inline capacity (not a cap). Two is the common maximum:
    /// `extend Vec<T> with Show` puts `adt_path` and `trait_path` on one `Item`. A generic with
    /// three bounds spills to the heap and stays correct.
    paths: HashMap<NodeId, SmallVec<[(Path, Res); 2]>>,
    lang_items: AstLangItems,
}

impl NameResolutions {
    pub fn new() -> Self {
        Self::default()
    }

    /// Records that `path`, written on `owner`, named `res`.
    ///
    /// **Invariant: within one node, no two entries may have equal paths.** A node owning two
    /// textually identical paths (`extend Foo with Foo`, or a duplicate bound `T: Show + Show`)
    /// is rejected by the check that owns it, before it reaches here. That is what leaves
    /// `get` unambiguous by construction rather than by a first-match tiebreak.
    pub fn record(&mut self, owner: NodeId, path: Path, res: Res) {
        let entries = self.paths.entry(owner).or_default();
        debug_assert!(
            !entries.iter().any(|(p, _)| p == &path),
            "a node may not own two equal paths; the owning check should have rejected this"
        );
        entries.push((path, res));
    }

    /// What `path`, as written on `owner`, named. Matches on segment symbols -- see
    /// `impl PartialEq for Path`. For `extend Vec<T> with Show`, the caller holding
    /// `adt_path` matches `[Vec]` and the caller holding `trait_path` matches `[Show]`, with
    /// no positional contract and no role discriminant for a later edit to invalidate.
    pub fn get(&self, owner: NodeId, path: &Path) -> Option<Res> {
        self.paths
            .get(&owner)?
            .iter()
            .find(|(p, _)| p == path)
            .map(|(_, res)| *res)
    }

    /// Every path `owner` owns, in source order. Empty for a node that owns none.
    pub fn entries(&self, owner: NodeId) -> &[(Path, Res)] {
        self.paths.get(&owner).map_or(&[], |v| v.as_slice())
    }

    pub fn record_lang_items(&mut self, lang_items: AstLangItems) {
        self.lang_items = lang_items;
    }

    pub fn lang_items(&self) -> &AstLangItems {
        &self.lang_items
    }
}
