//! Name resolution walks the HIR and resolves every name and path to the declaration it refers
//! to: a local binding, a top-level item, an enum variant, `Self`, or a primitive type.
//!
//! Each `resolve_*` method visits one kind of HIR node and, whenever it finds a name or path,
//! looks it up through [`SymbolTable`] and records the answer in [`NameResolutions`] under
//! that node's [`HirId`](crate::hir::HirId). Typeck consumes those results afterward instead of
//! doing its own lookups, which keeps "what does this name mean" a single, self-contained pass
//! over the tree.
//!
//! The submodules split the walk by node kind: [`resolve_block`] and [`resolve_item`] drive the
//! traversal into [`resolve_expr`], [`resolve_pat`], and [`resolve_ty`] as each kind of node
//! turns up. [`symbol_table`] holds the scopes and per-module namespaces the lookups run
//! against, and [`results`] defines the output type and what a name can resolve to.

pub mod resolve_block;
pub mod resolve_expr;
pub mod resolve_item;
pub mod resolve_pat;
pub mod resolve_ty;
pub mod results;
pub mod symbol_table;
#[cfg(test)]
mod tests;

use crate::ast::Symbol;
use crate::hir::{DefId, Hir};
use crate::langitems;
use crate::nameres::results::{NameResolutions, SelfTyRes, TypeRes};
use crate::nameres::symbol_table::SymbolTable;

/// Note that the resolver carries no "where am I?" state of its own. Every `resolve_*` method
/// already takes the [`DefId`] of the owner it is walking, and the HIR records each owner's
/// parent, so both kinds of surrounding context -- the enclosing module and the meaning of
/// `Self` -- are derived from that id on demand ([`Hir::module_of`], [`Self::self_ty`]) rather
/// than tracked in a field that has to be saved and restored at every nesting boundary.
struct NameResolver<'hir> {
    hir: &'hir Hir,
    symbol_tab: SymbolTable<'hir>,
    results: NameResolutions,
}

pub fn resolve(hir: &Hir) -> NameResolutions {
    let symbol_tab = SymbolTable::new(hir);

    // Resolved up front, against the finished namespaces, because the symbol table they are
    // looked up through does not outlive this function. They are carried out in the results so
    // that a later pass can reach them without one.
    let lang_items = langitems::collect(&symbol_tab, hir.root_id());

    let mut resolver = NameResolver {
        hir,
        results: NameResolutions::new(),
        symbol_tab,
    };
    resolver.resolve_module(hir.root_id());
    resolver.results.record_lang_items(lang_items);
    resolver.results
}

impl<'hir> NameResolver<'hir> {
    /// What `Self` resolves to inside `owner_id`: the nearest enclosing definition that
    /// introduces a `Self` -- a struct, enum, trait, or `extend` block -- found by walking up
    /// the parent chain, so that a method (or a closure nested inside one) picks up the `Self`
    /// of the item it is declared in. `None` when there is no such enclosing definition.
    ///
    /// This reads the table [`resolve_module`](Self::resolve_module)'s traversal fills in.
    /// That is safe because a definition's `Self` is recorded before its body is walked, and
    /// `Self` can only be *named* from within that body.
    fn self_ty(&self, owner_id: DefId) -> Option<SelfTyRes> {
        let mut current = owner_id;
        loop {
            if let Some(res) = self.results.self_ty(current) {
                return Some(res);
            }
            current = self.hir.parent(current)?;
        }
    }

    /// What `name` resolves to as a generic type parameter visible inside `owner_id`: `name`
    /// itself if `owner_id` declares a generic by that name, otherwise the same lookup against
    /// each enclosing definition in turn, so a method (or a closure nested inside one) can name
    /// a type parameter its enclosing `extend` block or trait declares. `None` if no enclosing
    /// definition declares a generic named `name`.
    ///
    /// This reads the table [`resolve_generics`](Self::resolve_generics) fills in, which is
    /// safe for the same reason [`Self::self_ty`] is: a definition's generics are recorded
    /// before its body is walked, and its body is the only place they can be named from.
    fn generic_ty(&self, owner_id: DefId, name: Symbol) -> Option<TypeRes> {
        let mut current = owner_id;
        loop {
            if let Some(res) = self.results.generic(current, name) {
                return Some(res);
            }
            current = self.hir.parent(current)?;
        }
    }
}
