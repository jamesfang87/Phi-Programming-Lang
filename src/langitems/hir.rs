use std::collections::HashMap;

use super::LangItem;
use crate::ast::NodeId;
use crate::hir::DefId;

/// The HIR-side record of lang items, keyed by [`DefId`]. Built out of an
/// [`ast::LangItems`](super::ast::LangItems) by [`LangItems::from_ast`] once lowering has
/// assigned every item a `DefId`; every pass after lowering reads this rather than the AST-side
/// record.
#[derive(Default, Debug)]
pub struct LangItems {
    items: HashMap<LangItem, DefId>,
}

impl LangItems {
    /// The definition `item` names, or `None` if it failed to resolve.
    pub fn get(&self, item: LangItem) -> Option<DefId> {
        self.items.get(&item).copied()
    }

    /// Whether `def_id` is the definition `item` names.
    pub fn is(&self, item: LangItem, def_id: DefId) -> bool {
        self.get(item) == Some(def_id)
    }

    /// Translates an [`ast::LangItems`](super::ast::LangItems) into its HIR equivalent.
    pub fn from_ast(
        ast_items: &super::ast::LangItems,
        to_def_id: impl Fn(NodeId) -> Option<DefId>,
    ) -> LangItems {
        let mut items = HashMap::new();

        for &item in LangItem::ALL {
            if let Some(node_id) = ast_items.get(item) {
                let def_id = to_def_id(node_id).unwrap_or_else(|| {
                    panic!(
                        "lowering bug: lang item `{}` resolved to {node_id:?}, which lowering \
                         never gave a DefId",
                        item.display_path()
                    )
                });
                items.insert(item, def_id);
            }
        }

        LangItems { items }
    }
}
