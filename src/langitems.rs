//! Lang items are the definitions in the core library which are crucial to
//! the compiler. These consist of `Option` and `Result`, which are
//! commonly used by the std library and are required by the `?` operator.
//! They also include traits such as those which are dispatched by operators
//! and Iter, which is used in loops.
//!

use std::collections::HashMap;

use crate::ast::interner::Interner;
use crate::ast::{Ident, NodeId, Path};
use crate::diagnostics::langitems::report_missing;
use crate::driver::source::SrcSpan;
use crate::hir::DefId;
use crate::nameres::res::Type as AstType;
use crate::nameres::symbol_table::SymbolTable as AstSymbolTable;

/// One definition in the core library that the compiler knows by name.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum LangItem {
    /// `core::option::Option`, which `for` loops and `Iterator` are defined in terms of.
    Option,
    /// `core::result::Result`, which `?` is defined in terms of.
    Result,
    Add,
    Sub,
    Mul,
    Div,
    Rem,
    Neg,
    Not,
    Eq,
    Comparable,
    Index,
    IndexSet,
    Drop,
    /// `core::iter::Iterator`, which is used to desugar `for` loops
    Iterator,
}

impl LangItem {
    pub const ALL: &'static [LangItem] = &[
        LangItem::Option,
        LangItem::Result,
        LangItem::Add,
        LangItem::Sub,
        LangItem::Mul,
        LangItem::Div,
        LangItem::Rem,
        LangItem::Neg,
        LangItem::Not,
        LangItem::Eq,
        LangItem::Comparable,
        LangItem::Index,
        LangItem::IndexSet,
        LangItem::Drop,
        LangItem::Iterator,
    ];

    pub fn path(self) -> &'static [&'static str] {
        match self {
            LangItem::Option => &["core", "option", "Option"],
            LangItem::Result => &["core", "result", "Result"],
            LangItem::Add => &["core", "ops", "Add"],
            LangItem::Sub => &["core", "ops", "Sub"],
            LangItem::Mul => &["core", "ops", "Mul"],
            LangItem::Div => &["core", "ops", "Div"],
            LangItem::Rem => &["core", "ops", "Rem"],
            LangItem::Neg => &["core", "ops", "Neg"],
            LangItem::Not => &["core", "ops", "Not"],
            LangItem::Eq => &["core", "ops", "Eq"],
            LangItem::Comparable => &["core", "ops", "Comparable"],
            LangItem::Index => &["core", "ops", "Index"],
            LangItem::IndexSet => &["core", "ops", "IndexSet"],
            LangItem::Drop => &["core", "ops", "Drop"],
            LangItem::Iterator => &["core", "iter", "Iterator"],
        }
    }

    pub fn display_path(self) -> String {
        self.path().join("::")
    }
}

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
}

/// The AST-side twin of [`LangItems`], keyed by [`NodeId`] rather than [`DefId`] since AST-level
/// lang-item resolution (see [`collect_ast`]) runs before any HIR and its `DefId`s exist.
/// [`translate`] converts one of these into the [`LangItems`] every pass after lowering reads.
///
/// A separate struct rather than a generic `LangItems<Id>`: the two only ever need `get`/`is`
/// (and, here, an iterator for [`translate`] to walk), so the handful of duplicated lines are
/// cheaper than a generic parameter and a hand-written `Default` impl to dodge an `Id: Default`
/// bound.
#[derive(Default, Debug)]
pub struct AstLangItems {
    items: HashMap<LangItem, NodeId>,
}

impl AstLangItems {
    /// The definition `item` names, or `None` if it failed to resolve.
    pub fn get(&self, item: LangItem) -> Option<NodeId> {
        self.items.get(&item).copied()
    }

    /// Whether `node_id` is the definition `item` names.
    pub fn is(&self, item: LangItem, node_id: NodeId) -> bool {
        self.get(item) == Some(node_id)
    }
}

pub fn collect_ast_lang_items(symbol_tab: &AstSymbolTable<'_>, root: NodeId) -> AstLangItems {
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

    AstLangItems { items }
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

/// Translates AstLangItems to its HIR equivalent.
pub fn translate(
    ast_items: &AstLangItems,
    to_def_id: impl Fn(NodeId) -> Option<DefId>,
) -> LangItems {
    let mut items = HashMap::new();

    for &item in LangItem::ALL {
        if let Some(node_id) = ast_items.get(item) {
            let def_id = to_def_id(node_id).unwrap_or_else(|| {
                panic!(
                    "lowering bug: lang item `{}` resolved to {node_id:?}, which lowering never \
                     gave a DefId",
                    item.display_path()
                )
            });
            items.insert(item, def_id);
        }
    }

    LangItems { items }
}
