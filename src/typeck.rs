pub mod check;
pub mod collect;
pub mod lower_ty;
pub mod results;
pub mod traits;
pub mod ty;

use std::collections::{BTreeMap, HashMap};

use crate::ast::{UnaryOp, Visibility};
use crate::diagnostics::display::DisplayCtx;
use crate::diagnostics::typeck::report_recursive_type;
use crate::hir::{DefId, ExprKind, Hir, HirId, Node, OwnerNode};
use crate::session::Session;
use crate::typeck::check::expr::DerefTarget;
use crate::typeck::results::TypeResolutions;
use crate::typeck::traits::collect::ExtendIndex;
use crate::typeck::traits::solve::Obligation;
use crate::typeck::ty::adt;
use crate::typeck::ty::ctx::TyCtx;
use crate::typeck::ty::unify::Unifier;
use crate::typeck::ty::{Ty, TyKind};

pub struct Typeck<'hir> {
    session: &'hir Session,
    hir: &'hir Hir,

    tcx: TyCtx,
    types: TypeResolutions,
    unifier: Unifier,

    extends: ExtendIndex,

    pending_trait_bounds: BTreeMap<DefId, Vec<Obligation>>,
}

impl<'hir> Typeck<'hir> {
    pub fn new(session: &'hir Session, hir: &'hir Hir) -> Self {
        Typeck {
            session,
            hir,
            tcx: TyCtx::new(),
            types: TypeResolutions::new(),
            unifier: Unifier::new(),
            extends: ExtendIndex::new(),
            pending_trait_bounds: BTreeMap::new(),
        }
    }

    fn ty_of(&mut self, id: impl Into<HirId>) -> Ty {
        self.ty_of_expecting(id, None)
    }

    fn ty_of_expecting(&mut self, id: impl Into<HirId>, expected: Option<Ty>) -> Ty {
        let ty = self.annotation_or_check(id.into(), expected);
        self.resolve(ty)
    }

    fn ty_of_as_place(&mut self, id: impl Into<HirId>) -> Ty {
        self.ty_of_as_place_expecting(id, None)
    }

    fn ty_of_as_place_expecting(&mut self, id: impl Into<HirId>, expected: Option<Ty>) -> Ty {
        let ty = self.place_annotation_or_check(id.into(), expected);
        self.resolve(ty)
    }

    fn annotation_or_check(&mut self, id: HirId, expected: Option<Ty>) -> Ty {
        if let Some(ty) = self.types.ty(id) {
            return ty;
        }

        debug_assert!(
            matches!(self.hir.node(id), Node::Expr(_)),
            "{} node {id:?} was asked for its type before whatever records one ran",
            self.hir.node(id).kind_name()
        );

        let ty = self.check_expr(id, expected);
        self.types.record(id, ty);
        ty
    }

    fn place_annotation_or_check(&mut self, id: HirId, expected: Option<Ty>) -> Ty {
        if let Some(ty) = self.types.ty(id) {
            return ty;
        }
        let expr = self.hir.expr(id);
        if let ExprKind::Unary {
            op: UnaryOp::Deref,
            operand,
        } = expr.kind
        {
            let span = expr.span;
            let ty = self.check_deref_as(id, operand, span, DerefTarget::Place);
            self.types.record(id, ty);
            return ty;
        }
        self.annotation_or_check(id, expected)
    }

    fn resolve(&mut self, ty: Ty) -> Ty {
        self.unifier.find_deep(&mut self.tcx, ty)
    }

    pub(crate) fn signature(&mut self, def: DefId) -> Option<(Vec<Ty>, Option<Ty>)> {
        let sig = self.ty_of(def.owner_id());
        match self.tcx.kind(sig) {
            TyKind::Fun { params, ret } => Some((params.clone(), *ret)),
            _ => None,
        }
    }

    fn is_visible_from(&self, owner_module: DefId, from: DefId, visibility: Visibility) -> bool {
        match visibility {
            Visibility::Public => true,
            Visibility::Private => {
                let mut current = Some(self.hir.module_of(from));
                while let Some(module) = current {
                    if module == owner_module {
                        return true;
                    }
                    current = self.hir.parent(module);
                }
                false
            }
        }
    }

    fn display_cx(&self) -> DisplayCtx<'_> {
        DisplayCtx::new(self.session, self.hir, &self.tcx)
    }

    fn report_recursive_types(&self, hir: &Hir, adts: &HashMap<DefId, adt::AdtDef>) {
        for def in adt::recursive_adts(&self.tcx, adts) {
            let name = match hir.def(def) {
                OwnerNode::Struct(struct_) => struct_.name,
                OwnerNode::Enum(enum_) => enum_.name,
                _ => continue,
            };
            report_recursive_type(self.display_cx(), name);
        }
    }
}

pub struct TypeckOutput {
    pub tcx: TyCtx,
    pub types: TypeResolutions,
}

/// Returns every type's `Drop::drop` method, keyed by the ADT the implementation extends.
fn collect_drop_methods(checker: &Typeck<'_>) -> HashMap<DefId, DefId> {
    let Some(drop_trait) = checker
        .hir
        .lang_items()
        .get(crate::langitems::LangItem::Drop)
    else {
        return HashMap::new();
    };

    let mut methods = HashMap::new();
    for block in checker.extends.all() {
        let Some(trait_ref) = checker.extends.trait_of(block) else {
            continue;
        };
        if trait_ref.def != drop_trait {
            continue;
        }
        let TyKind::Adt { def, .. } = *checker.tcx.kind(checker.extended_type(block)) else {
            continue;
        };
        let Some(method) = checker.get_method_in_block(block, checker.session.intern("drop"))
        else {
            continue;
        };
        methods.insert(def, method);
    }
    methods
}

pub fn check(session: &Session, hir: &Hir) -> TypeckOutput {
    let mut checker = Typeck::new(session, hir);

    checker.collect_module(hir.root_id());
    let adts = adt::collect_adt_defs(hir, &checker.types);
    checker.report_recursive_types(hir, &adts);
    checker.tcx.set_adts(adts);
    checker.tcx.set_maybe_uninit(
        hir.lang_items()
            .get(crate::langitems::LangItem::MaybeUninit),
    );

    checker.collect_traits();
    let drop_methods = collect_drop_methods(&checker);
    checker.tcx.set_drop_methods(drop_methods);
    checker.check_traits();
    checker.register_extend_header_bounds();
    checker.check_module(hir.root_id());
    checker.check_bound_obligations();

    TypeckOutput {
        tcx: checker.tcx,
        types: checker.types,
    }
}
