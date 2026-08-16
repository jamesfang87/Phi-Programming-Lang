use std::collections::HashMap;

use smallvec::SmallVec;

use crate::ast::{NodeId, Path};
use crate::langitems::ast::LangItems;
use crate::nameres::res::Res;

#[derive(Debug, Default)]
pub struct NameResolutions {
    paths: HashMap<NodeId, SmallVec<[(Path, Res); 2]>>,
    lang_items: LangItems,
}

impl NameResolutions {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn record(&mut self, owner: NodeId, path: Path, res: Res) {
        let entries = self.paths.entry(owner).or_default();
        debug_assert!(
            !entries.iter().any(|(p, _)| p == &path),
            "a node may not own two equal paths; the owning check should have rejected this"
        );
        entries.push((path, res));
    }

    pub fn get(&self, owner: NodeId, path: &Path) -> Option<Res> {
        self.paths
            .get(&owner)?
            .iter()
            .find(|(p, _)| p == path)
            .map(|(_, res)| *res)
    }

    /// This returns every (Path, Res) for a NodeId `owner`.
    /// If there are none, then this is empty
    pub fn entries(&self, owner: NodeId) -> &[(Path, Res)] {
        self.paths.get(&owner).map_or(&[], |v| v.as_slice())
    }

    pub fn record_lang_items(&mut self, lang_items: LangItems) {
        self.lang_items = lang_items;
    }

    pub fn lang_items(&self) -> &LangItems {
        &self.lang_items
    }
}
