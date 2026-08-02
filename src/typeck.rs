//! Type checking runs in two stages, for the same reason name resolution runs before it: a
//! definition's body can refer to any other definition in the program, including ones declared
//! later, so nothing about a body can be checked until every *signature* is known.
//!
//! [`collect`] is the first stage. It walks every declaration in the program -- a struct's
//! fields, an enum's variants, a function's parameters and return type -- and converts each type
//! annotation the user wrote into the [`Ty`] the checker reasons about, recording it in
//! [`TypeckResults`]. It never looks inside a function body. The second stage, which checks
//! those bodies against the signatures collected here, is not written yet.

use std::collections::{HashMap, HashSet};

use crate::ast::{Mutability, SelfMode};
use crate::hir::{
    DefId, Hir, HirId, LocalId, NameResolverResults, Node, OwnerNode, VariantPayload,
};
use crate::typeck::ty::Ty;
use crate::typeck::tyctx::TyCtx;
use crate::typeck::tyres::TypeckResults;

pub mod lower_ty;
pub mod ty;
pub mod tyctx;
pub mod tyres;

pub struct Typeck<'hir> {
    hir: &'hir Hir,

    /// What every path in the program resolved to. Type lowering reads this instead of doing its
    /// own name lookups -- see [`lower_ty`].
    nameres: &'hir NameResolverResults,

    /// Owns every type this pass builds. Handed back by [`collect`] along with the results,
    /// since a [`Ty`] means nothing without it.
    tcx: TyCtx,

    results: TypeckResults,

    /// What `Self` lowers to inside each definition that introduces one, cached because `Self` is
    /// typically written many times in one body. Filled in on demand by
    /// [`Typeck::self_ty`](crate::typeck::Typeck).
    self_tys: HashMap<DefId, Ty>,

    /// The definitions whose `Self` is being computed right now, used to cut off a `Self` that
    /// is defined in terms of itself instead of recursing forever.
    computing_self_tys: HashSet<DefId>,
}

impl<'hir> Typeck<'hir> {
    pub fn collect_module(&mut self, module_id: DefId) {
        let arena = self.hir.arena(module_id);
        let OwnerNode::Module(module) = arena.owner() else {
            unreachable!("root of a Module owner is always OwnerNode::Module");
        };

        for &item in &module.items {
            match self.hir.owner(item) {
                OwnerNode::Module(_) => self.collect_module(item),
                OwnerNode::Function(_) => self.collect_function(item),
                OwnerNode::Struct(_) => self.collect_struct(item),
                OwnerNode::Enum(_) => self.collect_enum(item),
                OwnerNode::Trait(_) => self.collect_trait(item),
                OwnerNode::Extend(_) => self.collect_extend(item),
                _ => {
                    unreachable!(
                        "A module should not contain fields, variants, type params, and closures in the top level"
                    )
                }
            }
        }
    }

    /// Collects a function's signature: its type parameters, the type of `self` if it is a
    /// method, each parameter's type, and its return type.
    ///
    /// The body is deliberately skipped. Checking it needs every other signature in the program
    /// to be collected first, which is exactly what this pass is producing.
    pub fn collect_function(&mut self, def_id: DefId) {
        let OwnerNode::Function(function) = self.hir.owner(def_id) else {
            unreachable!("root of a Function owner is always OwnerNode::Function");
        };
        let (generics, self_param, params, ret) = (
            function.generics.clone(),
            function.self_param,
            function.params.clone(),
            function.ret,
        );

        self.collect_generics(def_id, &generics);

        // A method's `self` counts as its first parameter, so that a signature says everything
        // a call has to be checked against without the caller having to look the receiver up
        // separately.
        let mut param_tys = Vec::with_capacity(params.len() + usize::from(self_param.is_some()));
        if let Some(local_id) = self_param {
            param_tys.push(self.collect_self_param(def_id, local_id));
        }
        for local_id in params {
            let hir_id = HirId {
                owner: def_id,
                local_id,
            };
            let Node::Param(param) = self.hir.node(hir_id) else {
                unreachable!("Node that is not a parameter found in a function's parameter list");
            };

            let ty = self.lower_ty(def_id, param.ty);
            self.results.add(hir_id, ty);
            param_tys.push(ty);
        }
        let ret = ret.map(|ret| self.lower_ty(def_id, ret));

        let sig = self.tcx.mk_fun(param_tys, ret);
        self.results.add_def(def_id, sig);
    }

    /// Gives the `self` parameter of a method its type: the enclosing `Self`, wrapped according
    /// to how the method takes it. `any self` accepts every one of the other three forms at
    /// once, which no single type describes, so it keeps the bare `Self` type and leaves the
    /// distinction to be enforced at the call site.
    fn collect_self_param(&mut self, def_id: DefId, local_id: LocalId) -> Ty {
        let hir_id = HirId {
            owner: def_id,
            local_id,
        };
        let Node::SelfParam(self_param) = self.hir.node(hir_id) else {
            unreachable!("Node that is not a self param found in a function's self param slot");
        };
        let (mode, span) = (self_param.mode, self_param.span);

        let self_ty = self.self_ty(def_id, span);
        let ty = match mode {
            SelfMode::Immutable => self.tcx.mk_ref(self_ty, Mutability::Immutable),
            SelfMode::Mutable => self.tcx.mk_ref(self_ty, Mutability::Mutable),
            SelfMode::Move => self_ty,
            SelfMode::Any => self.tcx.mk_any(self_ty),
        };
        self.results.add(hir_id, ty);
        ty
    }

    pub fn collect_struct(&mut self, def_id: DefId) {
        let OwnerNode::Struct(struct_) = self.hir.owner(def_id) else {
            unreachable!("root of a Struct owner is always OwnerNode::Struct");
        };
        let (generics, fields, span) = (
            struct_.generics.clone(),
            struct_.fields.clone(),
            struct_.span,
        );

        // The generics have to be recorded first: the struct's own type is itself applied to
        // them.
        self.collect_generics(def_id, &generics);
        let self_ty = self.self_ty(def_id, span);
        self.results.add_def(def_id, self_ty);

        self.collect_fields(def_id, &fields);
    }

    pub fn collect_enum(&mut self, def_id: DefId) {
        let OwnerNode::Enum(enum_) = self.hir.owner(def_id) else {
            unreachable!("root of an Enum owner is always OwnerNode::Enum");
        };
        let (generics, variants, span) =
            (enum_.generics.clone(), enum_.variants.clone(), enum_.span);

        self.collect_generics(def_id, &generics);
        let self_ty = self.self_ty(def_id, span);
        self.results.add_def(def_id, self_ty);

        for local_id in variants {
            let hir_id = HirId {
                owner: def_id,
                local_id,
            };
            let Node::Variant(variant) = self.hir.node(hir_id) else {
                unreachable!("Node that is not a variant found in an enum's variant list");
            };

            match &variant.payload {
                VariantPayload::Unit => {}
                VariantPayload::Type(ty_id) => {
                    let ty_id = *ty_id;
                    let ty = self.lower_ty(def_id, ty_id);
                    self.results.add(hir_id, ty);
                }
                VariantPayload::Record(fields) => {
                    let fields = fields.clone();
                    self.collect_fields(def_id, &fields);
                }
            }
        }
    }

    pub fn collect_trait(&mut self, def_id: DefId) {
        let OwnerNode::Trait(trait_) = self.hir.owner(def_id) else {
            unreachable!("root of a Trait owner is always OwnerNode::Trait");
        };
        let (generics, functions, span) = (
            trait_.generics.clone(),
            trait_.functions.clone(),
            trait_.span,
        );

        self.collect_generics(def_id, &generics);
        // A trait names no type of its own, so what it gets recorded as is the `Self` it stands
        // for: the placeholder every implementing type substitutes.
        let self_ty = self.self_ty(def_id, span);
        self.results.add_def(def_id, self_ty);

        for function in functions {
            self.collect_function(function);
        }
    }

    /// Collects an `extend` block's three bracket groups and the signature of each method it
    /// holds.
    ///
    /// The three groups scope differently -- see [`Extend`](crate::hir::Extend) -- but all three
    /// lower the same way here. The block's own `<T>` list is written as types even though it
    /// declares parameters, and name resolution has already bound each entry to itself, so
    /// lowering one yields the [`Generic`](crate::typeck::ty::TyKind::Generic) it declares.
    pub fn collect_extend(&mut self, def_id: DefId) {
        let OwnerNode::Extend(extend) = self.hir.owner(def_id) else {
            unreachable!("root of an Extend owner is always OwnerNode::Extend");
        };
        let (extend_generics, adt_generics, trait_generics, methods, span) = (
            extend.extend_generics.clone(),
            extend.adt_generics.clone(),
            extend.trait_generics.clone(),
            extend.methods.clone(),
            extend.span,
        );

        self.lower_tys(def_id, &extend_generics);
        self.lower_tys(def_id, &adt_generics);
        self.lower_tys(def_id, &trait_generics);

        // Which is the extended type applied to `adt_generics`, so this is also what `Self`
        // means inside each method below.
        let self_ty = self.self_ty(def_id, span);
        self.results.add_def(def_id, self_ty);

        for method in methods {
            self.collect_function(method);
        }
    }

    /// Records the type each of `def_id`'s own type parameters stands for: itself.
    fn collect_generics(&mut self, def_id: DefId, generics: &[LocalId]) {
        for &local_id in generics {
            let hir_id = HirId {
                owner: def_id,
                local_id,
            };
            debug_assert!(
                matches!(self.hir.node(hir_id), Node::Generic(_)),
                "Node that is not a generic found in a generics list"
            );

            let ty = self.tcx.mk_generic(hir_id);
            self.results.add(hir_id, ty);
        }
    }

    /// Records the declared type of each field in a struct or a record variant.
    fn collect_fields(&mut self, def_id: DefId, fields: &[LocalId]) {
        for &local_id in fields {
            let hir_id = HirId {
                owner: def_id,
                local_id,
            };
            let Node::Field(field) = self.hir.node(hir_id) else {
                unreachable!("Node that is not a field found in a field list");
            };

            let ty = self.lower_ty(def_id, field.ty);
            self.results.add(hir_id, ty);
        }
    }
}

/// Converts every type annotation in the program into a [`Ty`], as described in the
/// [module docs](self).
///
/// The [`TyCtx`] comes back with the results because the [`Ty`] handles inside them are indices
/// into it and mean nothing on their own.
pub fn collect(hir: &Hir, nameres: &NameResolverResults) -> (TyCtx, TypeckResults) {
    let mut checker = Typeck {
        hir,
        nameres,
        tcx: TyCtx::new(),
        results: TypeckResults::new(),
        self_tys: HashMap::new(),
        computing_self_tys: HashSet::new(),
    };
    checker.collect_module(hir.root_id());
    (checker.tcx, checker.results)
}
