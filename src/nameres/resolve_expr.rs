//! The lookups an expression needs. The traversal itself is [`crate::hir::visit`]'s; see the
//! [`Visitor`](crate::hir::visit::Visitor) implementation in [`crate::nameres`].

use crate::ast::Path;
use crate::hir::visit::Visitor;
use crate::hir::{AccessArgs, DefId, ExprKind, HirId, OwnerNode};
use crate::nameres::NameResolver;
use crate::nameres::results::{TypeRes, ValueRes};
use crate::nameres::symbol_table::report_not_found;

impl<'hir> NameResolver<'hir> {
    /// Resolves a path used as a value: a local or `self` first, since an inner binding shadows
    /// any item of the same name, then the value namespace of the module the path sits in.
    pub fn resolve_value_path(&mut self, owner_id: DefId, path: &Path) -> ValueRes {
        if let [name] = path.segments.as_slice()
            && let Some(res) = self.symbol_tab.lookup(name.text)
        {
            return res;
        }

        if let Some(def_id) = self.symbol_tab.lookup_value_path(owner_id, path) {
            return ValueRes::Def(def_id);
        }

        let name = *path
            .segments
            .last()
            .expect("a path always has at least one segment");
        report_not_found(name);
        ValueRes::Err
    }

    /// Resolves the path of a struct literal (`Vector2D { .. }`) against the type namespace.
    pub fn resolve_struct_path(&mut self, owner_id: DefId, path: &Path) -> TypeRes {
        if let Some(def_id) = self.symbol_tab.lookup_type_path(owner_id, path) {
            return TypeRes::Def(def_id);
        }

        let name = *path
            .segments
            .last()
            .expect("a path always has at least one segment");
        report_not_found(name);
        TypeRes::Err
    }

    /// Resolves a `base.member` access, including its own children.
    ///
    /// This does the whole node rather than letting the shared walk reach the children, because
    /// the two readings of `base` are not the same traversal. When `base` names an enum the
    /// access is a variant -- `Shape.circle` -- and `base` is a type path that must *not* be
    /// resolved as a value; otherwise `base` is an ordinary value expression whose member is a
    /// field or a method, which only typeck can tell apart.
    pub fn resolve_access(&mut self, id: HirId) {
        let ExprKind::Access { base, member, args } = &self.hir.expr(id).kind else {
            unreachable!("resolve_access called on an expression that is not an access");
        };
        let (base, member) = (*base, *member);
        // The payload or argument list is resolved the same way regardless of what the access
        // turns out to be.
        let args: Vec<HirId> = match args {
            AccessArgs::None => Vec::new(),
            AccessArgs::Call(args) => args.clone(),
            AccessArgs::Record(fields) => fields.iter().map(|field| field.value).collect(),
        };

        for arg in args {
            self.visit_expr(arg);
        }

        match self.enum_named_by(base) {
            Some(enum_def) => {
                let res = self
                    .symbol_tab
                    .lookup_variant(enum_def, member.text)
                    .unwrap_or_else(|| {
                        report_not_found(member);
                        ValueRes::Err
                    });
                self.results.record_value(id, res);
            }
            // A value, so the member is a field or a method: deferred to typeck.
            None => self.visit_expr(base),
        }
    }

    /// The enum `base` names, if it names one at all rather than being a value.
    ///
    /// A single-segment path that a local shadows is a value, whatever else is in scope, which is
    /// what lets a variable and an enum share a name.
    fn enum_named_by(&self, base: HirId) -> Option<DefId> {
        let base = self.hir.expr(base);
        let ExprKind::Path(path) = &base.kind else {
            return None;
        };
        if let [name] = path.segments.as_slice()
            && self.symbol_tab.lookup(name.text).is_some()
        {
            return None;
        }
        let def_id = self.symbol_tab.lookup_type_path(base.hir_id.owner, path)?;
        matches!(self.hir.def(def_id), OwnerNode::Enum(_)).then_some(def_id)
    }
}
