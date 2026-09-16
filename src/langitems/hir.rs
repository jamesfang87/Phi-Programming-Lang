use std::collections::HashMap;

use super::LangItem;
use crate::ast::NodeId;
use crate::hir::DefId;

#[derive(Default, Debug, Clone)]
pub struct LangItems {
    items: HashMap<LangItem, DefId>,
}

impl LangItems {
    pub fn get(&self, item: LangItem) -> Option<DefId> {
        self.items.get(&item).copied()
    }

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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::langitems::ast::LangItems as AstLangItems;

    /// A lang item that resolved during name resolution but never got a `DefId` during lowering
    /// is a bug in lowering, and the panic says which item it was.
    #[test]
    #[should_panic(expected = "lowering never gave a DefId")]
    fn a_lang_item_whose_node_lowering_missed_is_a_lowering_bug() {
        let ast_items = AstLangItems::with_item(LangItem::ALL[0], NodeId::next());

        LangItems::from_ast(&ast_items, |_| None);
    }
}
