//! Lowers patterns, including desugaring the `{ l }` record field shorthand into an explicit
//! `{ l: l }` binding pattern.

use crate::ast;
use crate::hir::lower::owner::OwnerLowerer;
use crate::hir::{BindingMode, HirId, PatKind, Payload, PayloadField};

impl OwnerLowerer<'_> {
    pub(super) fn lower_pat(&mut self, p: &ast::Pat) -> HirId {
        self.synth_pat(p.span, |low, _id| low.lower_pat_kind(&p.kind))
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
                                self.synth_pat(f.span, move |_, _| PatKind::Binding {
                                    name,
                                    mode: BindingMode::Inferred,
                                })
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
