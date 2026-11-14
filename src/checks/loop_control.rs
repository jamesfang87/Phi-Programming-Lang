use crate::diagnostics::typeck::expr::{report_break_outside_loop, report_continue_outside_loop};
use crate::hir::visit::{self, Visitor};
use crate::hir::{ExprKind, Hir, HirId, StmtKind};
use crate::session::Session;

pub fn check(session: &Session, hir: &Hir) {
    let mut checker = LoopControl {
        session,
        hir,
        loop_depth: 0,
    };
    for def_id in hir.def_ids() {
        if let Some(block) = super::body_block(hir, def_id) {
            checker.visit_block(block);
        }
    }
}

struct LoopControl<'hir, 'a> {
    session: &'a Session,
    hir: &'hir Hir,
    loop_depth: usize,
}

impl<'hir> Visitor<'hir> for LoopControl<'hir, '_> {
    fn hir(&self) -> &'hir Hir {
        self.hir
    }

    fn visit_stmt(&mut self, id: HirId) {
        match &self.hir.stmt(id).kind {
            StmtKind::Break => {
                if self.loop_depth == 0 {
                    report_break_outside_loop(self.session, self.hir.stmt(id).span);
                }
            }
            StmtKind::Continue => {
                if self.loop_depth == 0 {
                    report_continue_outside_loop(self.session, self.hir.stmt(id).span);
                }
            }
            _ => visit::walk_stmt(self, id),
        }
    }

    fn visit_expr(&mut self, id: HirId) {
        let block = match &self.hir.expr(id).kind {
            ExprKind::Loop { block, .. } => *block,
            _ => {
                visit::walk_expr(self, id);
                return;
            }
        };

        self.loop_depth += 1;
        self.visit_block(block.into());
        self.loop_depth -= 1;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testing::{lower_to_hir, session};

    #[test]
    fn break_and_continue_inside_a_loop_are_accepted() {
        let hir = lower_to_hir(
            "fun f() {
                 while true { break; }
                 while true { continue; }
             }",
        );
        session().clear_diagnostics();

        check(session(), &hir);

        assert!(session().messages().is_empty());
    }

    #[test]
    fn break_outside_a_loop_is_reported() {
        let hir = lower_to_hir("fun f() { break; }");
        session().clear_diagnostics();

        check(session(), &hir);

        assert_eq!(session().messages(), ["`break` outside a loop"]);
    }

    #[test]
    fn continue_outside_a_loop_is_reported() {
        let hir = lower_to_hir("fun f() { continue; }");
        session().clear_diagnostics();

        check(session(), &hir);

        assert_eq!(session().messages(), ["`continue` outside a loop"]);
    }
}
