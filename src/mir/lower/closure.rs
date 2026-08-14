//! Closure capture analysis and construction.
//!
//! No capture analysis exists anywhere else in the compiler; this module is where it is written.
//! [`BodyLowerCtx::captures_of`] walks a closure's own body (via [`crate::hir::visit::Visitor`],
//! which already stops at an arena boundary by default -- exactly what finding *this* closure's
//! own free variables needs, since a variable a *nested* closure captures is that nested
//! closure's own concern, found separately when it is lowered as its own task) and collects
//! every outer local it reads, in first-occurrence order. The environment those captures are
//! packed into has no declared type of its own to name (a closure's environment is synthesized,
//! not written in source), so it is represented internally as a plain tuple, one element per
//! capture in that same order: `Field(n)` addresses the `n`th capture exactly as it would any
//! other tuple element.

use std::collections::HashSet;

use crate::hir::visit::Visitor;
use crate::hir::{DefId, HirId, OwnerNode, Path, Res};
use crate::mir::lower::ctx::BodyLowerCtx;
use crate::mir::{AggregateKind, Local, Place, PlaceElem, Rvalue};
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
        let OwnerNode::Closure(closure) = self.hir.def(closure_def) else {
            panic!("mir::lower: captures_of called on a non-closure def")
        };
        let mut visitor = CaptureVisitor {
            hir: self.hir,
            owner: closure_def,
            seen: HashSet::new(),
            found: Vec::new(),
        };
        visitor.visit_block(closure.block);
        visitor.found
    }

    /// The closure's environment local's own type: a tuple of each capture's type, in capture
    /// order. Internal to the closure's own body; the enclosing body that builds the closure
    /// value never inspects it, since `Aggregate::Closure`'s operand list supplies captures
    /// positionally.
    pub(crate) fn environment_ty(&mut self, captures: &[HirId]) -> Ty {
        let tys: Vec<Ty> = captures
            .iter()
            .map(|&id| {
                self.types
                    .ty(id)
                    .unwrap_or_else(|| panic!("mir::lower: captured {id:?} has no recorded type"))
            })
            .collect();
        self.tcx.mk_tuple(tys)
    }

    /// Binds every captured HIR local to a projection into the environment local, so that an
    /// ordinary `ExprKind::Path` read inside the closure's own body resolves to
    /// `env.Field(n)` -- the "closure body's Places for captured variables project into [the
    /// environment]" the spec's "Closures" section describes.
    pub(crate) fn bind_environment(&mut self, env_local: Local, captures: &[HirId]) {
        for (index, &hir_id) in captures.iter().enumerate() {
            let place = Place {
                local: env_local,
                projection: vec![PlaceElem::Field(index as u32)],
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
            Rvalue::Aggregate(Box::new(AggregateKind::Closure { def: def_id }), operands),
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
