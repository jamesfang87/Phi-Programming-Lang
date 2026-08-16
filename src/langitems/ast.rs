use std::collections::HashMap;

use super::LangItem;
use crate::ast::interner::Interner;
use crate::ast::{Ident, NodeId, Path};
use crate::diagnostics::langitems::report_missing;
use crate::driver::source::SrcSpan;
use crate::nameres::res::Type as AstType;
use crate::nameres::symbol_table::SymbolTable;

/// The AST-side record of lang items, keyed by [`NodeId`] rather than
/// [`DefId`](crate::hir::DefId) since AST-level lang-item resolution (see [`collect`]) runs
/// before any HIR and its `DefId`s exist. [`hir::LangItems::from_ast`](super::hir::LangItems::from_ast)
/// converts one of these into a [`hir::LangItems`](super::hir::LangItems) once lowering has run.
#[derive(Default, Debug)]
pub struct LangItems {
    items: HashMap<LangItem, NodeId>,
}

impl LangItems {
    /// The definition `item` names, or `None` if it failed to resolve.
    pub fn get(&self, item: LangItem) -> Option<NodeId> {
        self.items.get(&item).copied()
    }

    /// Whether `node_id` is the definition `item` names.
    pub fn is(&self, item: LangItem, node_id: NodeId) -> bool {
        self.get(item) == Some(node_id)
    }
}

pub fn collect(symbol_tab: &SymbolTable<'_>, root: NodeId) -> LangItems {
    let mut items = HashMap::new();

    for &item in LangItem::ALL {
        let path = synth_path(item);
        match symbol_tab.lookup_type_path(root, &path) {
            Some(AstType::Def(def)) => {
                items.insert(item, def.node_id());
            }
            // `Prim` and `Generic` never name a lang item, and a `None` result means the core
            // library doesn't declare it at all -- both are the same "missing" outcome here.
            _ => report_missing(item),
        }
    }

    LangItems { items }
}

/// Builds the [`Path`] for a lang item so it can go through the ordinary path lookup.
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
