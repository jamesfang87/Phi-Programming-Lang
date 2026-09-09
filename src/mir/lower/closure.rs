use std::collections::HashSet;

use crate::hir::visit::Visitor;
use crate::hir::{DefId, HirId, Path, Res};
use crate::mir::lower::ctx::BodyLowerCtx;
use crate::mir::{AggregateKind, Local, Place, Projection, Rvalue};
use crate::typeck::ty::Ty;

struct CaptureVisitor<'hir> {
    hir: &'hir crate::hir::Hir,
    owner: DefId,
    seen: HashSet<HirId>,
    found: Vec<HirId>,
}

impl<'hir> Visitor<'hir> for CaptureVisitor<'hir> {
    fn hir(&self) -> &'hir crate::hir::Hir {
        self.hir
    }

    fn visit_path(&mut self, path: &'hir Path) {
        if let Res::Local(local) = path.res {
            let id = super::expr::hir_local_id(local);
            if id.owner != self.owner && self.seen.insert(id) {
                self.found.push(id);
            }
        }
    }
}

impl<'a> BodyLowerCtx<'a> {
    /// Every outer local `closure_def`'s own body reads, in first-occurrence order -- the
    /// closure's captures, in the order `AggregateKind::Closure`'s operand list and the
    /// closure's own environment tuple both use.
    pub(crate) fn captures_of(&self, closure_def: DefId) -> Vec<HirId> {
        let closure = self.hir.closure(closure_def);
        let mut visitor = CaptureVisitor {
            hir: self.hir,
            owner: closure_def,
            seen: HashSet::new(),
            found: Vec::new(),
        };
        visitor.visit_block(closure.block);
        visitor.found
    }

    pub(crate) fn environment_ty(&mut self, captures: &[HirId]) -> Ty {
        let mut tys: Vec<Ty> = vec![self.tcx.mk_prim(crate::nameres::PrimTy::Usize)];
        tys.extend(captures.iter().map(|&id| {
            self.types
                .ty(id)
                .unwrap_or_else(|| panic!("mir::lower: captured {id:?} has no recorded type"))
        }));
        let tuple = self.tcx.mk_tuple(tys);
        self.tcx.mk_ref(tuple, crate::ast::Mutability::Mutable)
    }

    /// Binds every captured HIR local to a projection into the environment local, so that an
    /// ordinary `ExprKind::Path` read inside the closure's own body resolves to
    pub(crate) fn bind_environment(&mut self, env_local: Local, captures: &[HirId]) {
        for (index, &hir_id) in captures.iter().enumerate() {
            let place = Place {
                local: env_local,
                projections: vec![Projection::Deref, Projection::Field(index as u32 + 1)],
            };
            self.bind_place(hir_id, place);
        }
    }

    /// Builds a closure literal's value: `Assign(dest, Aggregate(Closure { def }, captures))`,
    /// at the point it is evaluated, where each capture's `Copy`/`Move`/`Ref`-ness follows the
    /// same rule an ordinary read of that place would.
    pub(crate) fn lower_closure_literal_into(
        &mut self,
        def_id: DefId,
        dest: Place,
        span: crate::driver::source::SrcSpan,
    ) {
        self.discover(crate::mir::lower::Task::Ordinary(def_id));
        let captures = self.captures_of(def_id);
        let operands = captures
            .iter()
            .map(|&hir_id| self.capture_operand(hir_id))
            .collect();
        self.assign(
            dest,
            Rvalue::Aggregate(
                Box::new(AggregateKind::Closure {
                    def: def_id,
                    args: Vec::new(),
                }),
                operands,
            ),
            span,
        );
    }

    /// Reads a captured variable's operand from the *enclosing* body being lowered right now
    /// (not the closure's own body, which does not exist yet at this point): the same
    /// `Copy`/`Move` rule any other read of that place already gets, since a capture is exactly
    /// that, an ordinary read.
    fn capture_operand(&mut self, hir_id: HirId) -> crate::mir::Operand {
        let place = self.place_for(hir_id);
        let ty = self
            .types
            .ty(hir_id)
            .unwrap_or_else(|| panic!("mir::lower: captured {hir_id:?} has no recorded type"));
        self.operand_for_place(place, ty)
    }
}
