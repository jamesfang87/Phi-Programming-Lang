//! Lang items are the definitions in the core library that the compiler itself has to know by
//! name: the enums `?` and `for` desugar through, and the traits the operators dispatch to.
//!
//! Phi identifies them by path rather than by an attribute on the declaration. There is exactly
//! one core library, it is embedded in the compiler binary (see [`crate::driver::core_lib`]),
//! and nothing outside it may declare a lang item -- so the path a lang item lives at is already
//! a fact both sides agree on, and spelling it out here costs no syntax. [`LangItem::path`] is
//! the whole of that agreement: moving `Add` from `core::ops` to somewhere else means changing
//! its entry there and nowhere else.
//!
//! [`collect`] resolves every path in the table to a [`DefId`] once name resolution has built
//! the module namespaces, and reports the ones that are missing. Lookups afterwards go through
//! [`LangItems::get`], which returns `None` for a lang item that failed to resolve rather than
//! panicking -- by then the error has already been reported, and a later pass carrying on with
//! one missing lang item produces better diagnostics than one that aborts.

use std::collections::HashMap;

use crate::ast::interner::Interner;
use crate::ast::{Ident, Path};
use crate::diag::{DiagCtx, Diagnostic};
use crate::hir::DefId;
use crate::lexer::src_span::SrcSpan;
use crate::nameres::symbol_table::SymbolTable;

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
    /// Every lang item, in the order [`collect`] resolves and reports them.
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
/// A lang item that failed to resolve is absent rather than recorded as an error: [`collect`]
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

/// Resolves every lang item against `symbol_tab`, reporting each one that the core library
/// doesn't declare.
///
/// This runs against the type namespace only: every lang item is an enum or a trait, and both
/// live there. It must run after [`SymbolTable::new`] has collected every module's namespace and
/// resolved its imports, which is what makes a path like `core::ops::Add` resolvable at all.
pub fn collect(symbol_tab: &SymbolTable<'_>, root: DefId) -> LangItems {
    let mut items = HashMap::new();

    for &item in LangItem::ALL {
        let path = synth_path(item);
        match symbol_tab.lookup_type_path(root, &path) {
            Some(def_id) => {
                items.insert(item, def_id);
            }
            None => report_missing(item),
        }
    }

    LangItems { items }
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
