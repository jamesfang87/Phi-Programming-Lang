//! Lang items are the definitions in the core library that the compiler itself has to know by
//! name: the enums `?` and `for` desugar through, and the traits the operators dispatch to.
//!
//! Phi identifies them by path rather than by an attribute on the declaration. There is exactly
//! one core library, it is embedded in the compiler binary (see [`crate::driver::source::SrcCollector`]),
//! and nothing outside it may declare a lang item -- so the path a lang item lives at is already
//! a fact both sides agree on, and spelling it out here costs no syntax. [`LangItem::path`] is
//! the whole of that agreement: moving `Add` from `core::ops` to somewhere else means changing
//! its entry there and nowhere else.
//!
//! [`collect_ast`] resolves every path in the table to a [`NodeId`] once AST-level name
//! resolution has built the module namespaces, and reports the ones that are missing.
//! [`translate`] then carries that answer across lowering, into the [`DefId`]-keyed [`LangItems`]
//! every later pass actually reads. Lookups go through [`LangItems::get`], which returns `None`
//! for a lang item that failed to resolve rather than panicking -- by then the error has already
//! been reported, and a later pass carrying on with one missing lang item produces better
//! diagnostics than one that aborts.

use std::collections::HashMap;

use crate::ast::interner::Interner;
use crate::ast::{Ident, NodeId, Path};
use crate::diag::{DiagCtx, Diagnostic};
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
    /// `core::iter::Iterator`, the protocol a `for` loop desugars through.
    Iterator,
}

impl LangItem {
    /// Every lang item, in the order [`collect_ast`] resolves and reports them.
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

    /// The path this lang item is declared at, from the root module down.
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

    /// This lang item's path, written the way it would be in source.
    pub fn display_path(self) -> String {
        self.path().join("::")
    }
}

/// Every lang item the core library declares, resolved to the definition it names.
///
/// A lang item that failed to resolve is absent rather than recorded as an error: [`collect_ast`]
/// has already reported it, and the passes that consume this table treat a missing entry as "no
/// candidate", which is the same answer they'd reach for a type that doesn't implement the
/// trait.
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
/// lang-item resolution (see [`collect_ast`]) runs before any HIR -- and its `DefId`s -- exist.
/// [`translate`] is what turns one of these into the [`LangItems`] every pass after lowering
/// actually reads.
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

/// Resolves every lang item against `symbol_tab`, reporting each one that the core library
/// doesn't declare.
///
/// This runs against the type namespace only: every lang item is an enum or a trait, and both
/// live there. It must run after [`AstSymbolTable::new`] has collected every module's namespace
/// and resolved its imports, which is what makes a path like `core::ops::Add` resolvable at all.
pub fn collect_ast(symbol_tab: &AstSymbolTable<'_>, root: NodeId) -> AstLangItems {
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
///
/// The segments carry an empty span: they name no source text, since the path is the
/// compiler's own rather than one the user wrote. Nothing reports against these spans -- a
/// failure here is [`report_missing`]'s to describe, and it names the path in full.
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

/// Carries [`collect_ast`]'s answer across lowering: every lang item AST-level resolution found,
/// translated from the `NodeId` it resolved to into the `DefId` that node lowered to.
///
/// `to_def_id` is a closure rather than a `HashMap` because lowering doesn't keep one lying
/// around for this alone -- `LoweringCtx::def_ids`, already built for translating every ordinary
/// `hir::Path`, is exactly the lookup this needs too. A lang item that resolved at all is always
/// a struct, enum, or trait item, which is preallocated a `DefId` before any lowering proper
/// starts (see `hir::lower::lower_unit`), so `to_def_id` is expected to answer `Some` for every
/// `NodeId` `ast_items` holds -- `None` here would mean that invariant broke, not that the lang
/// item failed to resolve (a failure already left no entry in `ast_items` to translate; see
/// [`collect_ast`]).
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

/// Reports a lang item the core library doesn't declare.
///
/// This names no span. There is no source location that could be to blame: the core library is
/// embedded in the compiler binary, and the path being looked up is the compiler's own, so
/// nothing the user wrote is at fault. See [`Diagnostic::error_global`].
fn report_missing(item: LangItem) {
    DiagCtx::emit(
        Diagnostic::error_global(format!("missing lang item `{}`", item.display_path())).with_help(
            "the core library must declare this item; it is embedded in the compiler, so this \
             is a compiler bug rather than a problem with the program being compiled",
        ),
    );
}
