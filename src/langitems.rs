//! Lang items are the definitions in the core library which are crucial to
//! the compiler. These consist of `Option` and `Result`, which are
//! commonly used by the std library and are required by the `?` operator.
//! They also include traits such as those which are dispatched by operators
//! and Iter, which is used in loops.
//!
//! Resolving a lang item happens in two stages, matching the compiler's own
//! `ast` -> `hir` pipeline: [`ast::LangItems`] is collected by
//! [`ast::collect`], keyed by [`NodeId`](crate::ast::NodeId) since it runs
//! before any HIR exists; [`hir::LangItems`] is then built out of it by
//! [`hir::LangItems::from_ast`] once lowering has assigned every item a
//! [`DefId`](crate::hir::DefId).

pub mod ast;
pub mod hir;

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
