//! Lowers patterns, including desugaring the `{ l }` record field shorthand into an explicit
//! `{ l: l }` binding pattern.

use crate::ast;
use crate::hir::lower::owner::OwnerLowerer;
use crate::hir::{BindingMode, HirId, PatKind, Payload, PayloadField};

impl OwnerLowerer<'_, '_> {
    /// Lowers `p`, recording its `NodeId -> HirId` mapping when it binds a name (a
    /// `PatKind::Binding`) so that a later use of it -- resolved by AST-level resolution against
    /// `p.id` -- can be translated by [`crate::hir::lower::ctx::LoweringCtx::translate_res`].
    /// Sound because a pattern always lowers before whatever reads the name it binds: a `let`'s
    /// pattern lowers before the statements after it, and a match arm's before its own body (see
    /// `OwnerLowerer::lower_arm`).
    pub(super) fn lower_pat(&mut self, p: &ast::Pat) -> HirId {
        let hir_id = self.synth_pat(p.span, |low, _id| low.lower_pat_kind(&p.kind));
        if matches!(p.kind, ast::PatKind::Binding(_)) {
            self.cx.record_hir_id(p.id, hir_id);
        }
        hir_id
    }

    fn lower_pat_kind(&mut self, kind: &ast::PatKind) -> PatKind {
        match kind {
            ast::PatKind::Wildcard => PatKind::Wildcard,
            ast::PatKind::Binding(name) => PatKind::Binding {
                name: *name,
                mode: BindingMode::Inferred,
            },
            ast::PatKind::Literal(lit) => PatKind::Literal(*lit),
            ast::PatKind::Variant { variant, payload } => PatKind::Variant {
                variant: *variant,
                payload: self.lower_pat_payload(payload),
            },
            ast::PatKind::Tuple(pats) => {
                PatKind::Tuple(pats.iter().map(|pp| self.lower_pat(pp)).collect())
            }
            ast::PatKind::Error => PatKind::Error,
        }
    }

    /// Lowers a variant pattern's payload, desugaring the `{ l }` field shorthand into
    /// `{ l: l }` -- a real binding pattern -- so that every record field has a pattern behind
    /// it in the HIR.
    ///
    /// A shorthand field's synthesized binding has no `ast::Pat` behind it -- the shorthand only
    /// becomes one here, during lowering -- so AST-level resolution can't key it under a `Pat`'s
    /// `NodeId` the way [`Self::lower_pat`] does for an ordinary binding. It keys under the
    /// `PayloadField`'s own `NodeId` instead (`Local::Variable(field.id)`, see
    /// `Resolver::visit_record_pat_fields` in `src/nameres/resolver.rs`), which is why
    /// this records `f.id`, not the synthesized pattern's own (nonexistent) id, into
    /// `node_to_hir`.
    fn lower_pat_payload(&mut self, payload: &ast::Payload<ast::Pat>) -> Payload {
        match payload {
            ast::Payload::None => Payload::None,
            ast::Payload::Single(inner) => Payload::Single(self.lower_pat(inner)),
            ast::Payload::Record(fields) => Payload::Record(
                fields
                    .iter()
                    .map(|f| {
                        let value = match &f.value {
                            Some(inner) => self.lower_pat(inner),
                            None => {
                                let name = f.name;
                                let hir_id = self.synth_pat(f.span, move |_, _| PatKind::Binding {
                                    name,
                                    mode: BindingMode::Inferred,
                                });
                                self.cx.record_hir_id(f.id, hir_id);
                                hir_id
                            }
                        };
                        PayloadField {
                            name: f.name,
                            value,
                        }
                    })
                    .collect(),
            ),
        }
    }
}
