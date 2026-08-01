use crate::hir::{DefId, HirId, LocalId, Node, PatKind, Payload};
use crate::nameres::resolve_results::Res;
use crate::nameres::NameResolver;

impl<'hir> NameResolver<'hir> {
    pub fn bind_pat(&mut self, owner_id: DefId, pat_id: LocalId) {
        let hir = self.hir;

        let Node::Pat(pat) = hir.node(HirId {
            owner: owner_id,
            local_id: pat_id,
        }) else {
            unreachable!("Expected a pat's local id to name a pat");
        };

        match &pat.kind {
            PatKind::Binding { name, .. } => {
                let hir_id = HirId {
                    owner: owner_id,
                    local_id: pat_id,
                };
                self.results.add(hir_id, Res::Local(hir_id));
                self.symbol_tab.bind(*name, Res::Local(hir_id));
            }
            // Which enum a `.variant` pattern names comes from the scrutinee's type, so the
            // variant itself is left for typeck; only its payload's bindings are introduced
            // here.
            PatKind::Variant { payload, .. } => match payload {
                Payload::None => {}
                Payload::Single(inner) => self.bind_pat(owner_id, *inner),
                Payload::Record(fields) => {
                    for &(_, field_pat) in fields {
                        self.bind_pat(owner_id, field_pat);
                    }
                }
            },
            PatKind::Tuple(bindings) => {
                for &binding in bindings {
                    self.bind_pat(owner_id, binding);
                }
            }
            _ => {}
        }
    }
}
