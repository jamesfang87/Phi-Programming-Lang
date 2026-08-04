pub mod resolve_expr;
pub mod resolve_item;
pub mod resolve_ty;
pub mod results;
pub mod symbol_table;
#[cfg(test)]
mod tests;

use crate::ast::Symbol;
use crate::hir::visit::{self, Visitor};
use crate::hir::{DefId, ExprKind, Hir, HirId, PatKind, TyKind};
use crate::langitems;
use crate::nameres::results::{NameResolutions, TypeRes, ValueRes};
use crate::nameres::symbol_table::SymbolTable;

struct NameResolver<'hir> {
    hir: &'hir Hir,
    symbol_tab: SymbolTable<'hir>,
    results: NameResolutions,
}

pub fn resolve(hir: &Hir) -> NameResolutions {
    let symbol_tab = SymbolTable::new(hir);
    let lang_items = langitems::collect(&symbol_tab, hir.root_id());

    let mut resolver = NameResolver {
        hir,
        results: NameResolutions::new(),
        symbol_tab,
    };
    resolver.visit_module(hir.root_id());
    resolver.results.record_lang_items(lang_items);
    resolver.results
}

impl<'hir> NameResolver<'hir> {
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

/// Name resolution's traversal is [`crate::hir::visit`]'s, so that "what are this node's
/// children" is answered in one place for every pass rather than re-derived here.
///
/// Only the nodes that need something *around* or *instead of* the default walk are overridden:
/// a block opens a scope, a path records what it named, a binding pattern binds. Statements are
/// not overridden at all -- the shared walk already visits a `let`'s annotation, its `else`
/// block, and a `with` binding's annotation, three subtrees this pass used to skip silently.
impl<'hir> Visitor<'hir> for NameResolver<'hir> {
    fn hir(&self) -> &'hir Hir {
        self.hir
    }

    /// Every nested owner is followed as soon as it is reached: a closure body resolves against
    /// the scope it was written in, and a method against its trait or `extend` block.
    fn visit_nested_owner(&mut self, def_id: DefId) {
        visit::walk_item(self, def_id);
    }

    fn visit_function(&mut self, def_id: DefId) {
        self.resolve_function(def_id);
    }

    fn visit_closure(&mut self, def_id: DefId) {
        self.resolve_closure(def_id);
    }

    fn visit_struct(&mut self, def_id: DefId) {
        self.resolve_struct(def_id);
    }

    fn visit_enum(&mut self, def_id: DefId) {
        self.resolve_enums(def_id);
    }

    fn visit_trait(&mut self, def_id: DefId) {
        self.resolve_trait(def_id);
    }

    fn visit_extend(&mut self, def_id: DefId) {
        self.resolve_extend(def_id);
    }

    fn visit_block(&mut self, id: HirId) {
        self.symbol_tab.push_scope();
        visit::walk_block(self, id);
        self.symbol_tab.pop_scope();
    }

    /// An arm's bindings are scoped to that arm, so the scope has to open around the pattern and
    /// the body together rather than around either alone.
    fn visit_arm(&mut self, id: HirId) {
        self.symbol_tab.push_scope();
        visit::walk_arm(self, id);
        self.symbol_tab.pop_scope();
    }

    fn visit_expr(&mut self, id: HirId) {
        match &self.hir.expr(id).kind {
            ExprKind::Path(path) => {
                let path = path.clone();
                let res = self.resolve_value_path(id.owner, &path);
                self.results.record_value(id, res);
            }
            ExprKind::Ctor {
                path: Some(path), ..
            } => {
                // A struct literal names its type, so this one path in an expression is resolved
                // against the type namespace.
                let path = path.clone();
                let res = self.resolve_struct_path(id.owner, &path);
                self.results.record_type(id, res);
            }
            // `base.member` is either a variant named through its enum or an access on a value,
            // and the two walk their base differently -- see `resolve_access`, which does the
            // whole node and is why this returns instead of falling through to the walk.
            ExprKind::Access { .. } => {
                self.resolve_access(id);
                return;
            }
            _ => {}
        }
        visit::walk_expr(self, id);
    }

    /// Every pattern in the HIR is a binder, so this introduces names rather than looking them
    /// up. Which enum a `.variant` pattern belongs to comes from the scrutinee's type, so the
    /// variant itself is left for typeck and only its payload's bindings are introduced here.
    fn visit_pat(&mut self, id: HirId) {
        if let PatKind::Binding { name, .. } = self.hir.pat(id).kind {
            self.symbol_tab.bind(name, ValueRes::Local(id));
        }
        visit::walk_pat(self, id);
    }

    fn visit_ty(&mut self, id: HirId) {
        match &self.hir.ty(id).kind {
            TyKind::Path { path, .. } | TyKind::Dyn(path) => {
                let path = path.clone();
                let res = self.resolve_ty_path(id.owner, &path);
                self.results.record_type(id, res);
            }
            // `Self` resolves no path; what it stands for is recorded once per definition. See
            // the `TyKind::SelfType` arm's note in `resolve_ty`.
            _ => {}
        }
        visit::walk_ty(self, id);
    }
}
