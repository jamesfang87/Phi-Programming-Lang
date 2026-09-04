use std::collections::HashMap;

use super::LangItem;
use crate::ast::interner::Interner;
use crate::ast::{Ident, NodeId, Path};
use crate::diagnostics::langitems::report_missing;
use crate::driver::source::SrcSpan;
use crate::nameres::res::{Res, Type as AstType};
use crate::nameres::symbol_table::SymbolTable;

#[derive(Default, Debug)]
pub struct LangItems {
    items: HashMap<LangItem, NodeId>,
}

impl LangItems {
    pub fn get(&self, item: LangItem) -> Option<NodeId> {
        self.items.get(&item).copied()
    }
}

pub fn collect(symbol_tab: &SymbolTable<'_>, root: NodeId) -> LangItems {
    let mut items = HashMap::new();

    for &item in LangItem::ALL {
        let path = synth_path(item);
        // Most lang items are types (structs, traits), found in the type namespace. A handful
        // -- `write_bytes` so far -- name a function instead, which lives in the value
        // namespace and so needs the other lookup. Trying the type lookup first costs nothing
        // for a function path, since it can never resolve there.
        match symbol_tab.lookup_type_path(root, &path) {
            Some(AstType::Def(def)) => {
                items.insert(item, def.node_id());
            }
            _ => match symbol_tab.lookup_value_path(root, &path) {
                Some(Res::Function(id)) => {
                    items.insert(item, id);
                }
                _ => report_missing(item),
            },
        }
    }

    LangItems { items }
}

fn synth_path(item: LangItem) -> Path {
    let span = SrcSpan::new(0, 0);
    let segments = item
        .path()
        .iter()
        .map(|segment| Ident {
            text: Interner::intern(segment),
            span,
        })
        .collect();

    Path { segments, span }
}
