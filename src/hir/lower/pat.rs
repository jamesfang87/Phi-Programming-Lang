//! Lowers patterns, including desugaring the `{ l }` record field shorthand into an explicit
//! `{ l: l }` binding pattern.

use crate::ast;
use crate::hir::lower::owner::OwnerLowerer;
use crate::hir::{BindingMode, HirId, PatKind, Payload, PayloadField};

impl OwnerLowerer<'_, '_> {
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
