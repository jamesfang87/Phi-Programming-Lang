use crate::hir::{HirId, PatKind, Payload};
use crate::nameres::NameResolver;
use crate::nameres::results::{TypeRes, ValueRes};

impl<'hir> NameResolver<'hir> {
    pub fn bind_pat(&mut self, pat_id: HirId) {
        let pat = self.hir.pat(pat_id);

        match &pat.kind {
            PatKind::Binding { name, .. } => {
                self.symbol_tab.bind(*name, ValueRes::Local(pat_id));
            }
            // Which enum a `.variant` pattern names comes from the scrutinee's type, so the
            // variant itself is left for typeck; only its payload's bindings are introduced
            // here.
            PatKind::Variant { payload, .. } => match payload {
                Payload::None => {}
                Payload::Single(inner) => self.bind_pat(*inner),
                Payload::Record(fields) => {
                    for field in fields {
                        self.bind_pat(field.value);
                    }
                }
            },
            PatKind::Tuple(bindings) => {
                for &binding in bindings {
                    self.bind_pat(binding);
                }
            }
            _ => {}
        }
    }
}
