use std::collections::HashMap;

use crate::ast::interner::Interner;
use crate::ast::{Ident, Symbol};
use crate::hir::visit::Visitor;
use crate::hir::{DefId, Hir, HirId, OwnerNode, VariantPayload};
use crate::nameres::NameResolver;
use crate::nameres::results::{SelfTyRes, TypeRes, ValueRes};

impl<'hir> NameResolver<'hir> {
    /// Binds each generic type parameter `owner_id` declares for itself into the type namespace
    /// visible inside its own body, and resolves each parameter's trait bounds against the outer
    /// one.
    ///
    /// Must run before anything else in `owner_id`'s own declaration is resolved, since a
    /// parameter list, return type, field, or variant payload may name one of these parameters.
    /// [`NameResolver::generic_ty`] is what later lookups read this table through.
    fn resolve_generics(&mut self, owner_id: DefId, ids: &[HirId]) {
        let hir: &'hir Hir = self.hir;
        let mut params = HashMap::new();

        for &id in ids {
            let generic = hir.generic(id);
            // A bound is a bare `Path` on the parameter rather than a node of its own, so unlike
            // every other type-position path there is no `HirId` to record the answer under.
            // Keeping the answers in a list beside the parameter is what lets the trait solver
            // build a `ParamEnv` without re-running name lookup; see
            // [`NameResolutions::bounds`].
            let bounds = generic
                .bounds
                .iter()
                .map(|bound| self.resolve_ty_path(owner_id, bound))
                .collect();
            self.results.record_bounds(id, bounds);
            self.declare_generic(&mut params, generic.name, id);
        }

        self.results.record_generic(owner_id, params);
    }

    /// Records one type parameter: under its name for lookups from inside the body, and against
    /// the node that declares it.
    ///
    /// The second is not redundant. An `extend` block's entry is a real `Node::Ty` that type
    /// lowering visits like any other annotation, looking its answer up by id; without it that
    /// lookup finds nothing and the parameter lowers to `TyKind::Error`. The resolution points at
    /// the node itself, which is the identity mapping [`NameResolutions::record_type`] has to
    /// keep rather than discard.
    ///
    /// [`NameResolutions::record_type`]: crate::nameres::results::NameResolutions::record_type
    fn declare_generic(&mut self, params: &mut HashMap<Symbol, TypeRes>, name: Ident, id: HirId) {
        let res = TypeRes::Generic(id);
        params.insert(name.text, res);
        self.results.record_type(id, res);
    }

    pub fn resolve_function(&mut self, fun_id: DefId) {
        let hir = self.hir;
        let arena = hir.arena(fun_id);
        let OwnerNode::Function(function) = arena.owner() else {
            unreachable!("root of a Function owner is always OwnerNode::Function");
        };

        self.resolve_generics(fun_id, &function.generics);

        self.symbol_tab.push_scope();
        if let Some(id) = function.self_param {
            let self_param = hir.self_param(id);

            // `self` is bound like a parameter -- it just doesn't carry a name of its own in the
            // HIR, so the keyword is interned here to bind it under. It resolves to
            // `ValueRes::SelfVal` rather than `ValueRes::Local`: its type isn't declared anywhere, it's
            // the enclosing item's `Self`, recovered from `fun_id` the same way
            // [`NameResolver::self_ty`] does.
            let name = Ident {
                text: Interner::intern("self"),
                span: self_param.span,
            };
            self.results.record_value(id, ValueRes::SelfVal(id));
            self.symbol_tab.bind(name, ValueRes::SelfVal(id));
        }
        for &param_id in &function.params {
            let param = hir.param(param_id);
            self.symbol_tab.bind(param.name, ValueRes::Local(param_id));
            self.visit_ty(param.ty);
        }
        if let Some(ret) = function.ret {
            self.visit_ty(ret);
        }

        let body = function.block;
        if let Some(body_id) = body {
            self.visit_block(body_id);
        }
        self.symbol_tab.pop_scope();
    }

    /// Resolves a closure's own owner: its parameter types, its return type, and its body, in a
    /// fresh scope binding each parameter -- the same shape as [`Self::resolve_function`], but for
    /// the anonymous owner a closure literal gets promoted to during lowering.
    pub fn resolve_closure(&mut self, closure_id: DefId) {
        let hir = self.hir;
        let arena = hir.arena(closure_id);
        let OwnerNode::Closure(closure) = arena.owner() else {
            unreachable!("root of a Closure owner is always OwnerNode::Closure");
        };

        self.symbol_tab.push_scope();
        for &param_id in &closure.params {
            let param = hir.closure_param(param_id);
            self.symbol_tab.bind(param.name, ValueRes::Local(param_id));
            if let Some(ty) = param.ty {
                self.visit_ty(ty);
            }
        }
        if let Some(ret) = closure.ret {
            self.visit_ty(ret);
        }
        self.visit_block(closure.block);
        self.symbol_tab.pop_scope();
    }

    pub fn resolve_struct(&mut self, struct_id: DefId) {
        let hir = self.hir;
        let arena = hir.arena(struct_id);
        let OwnerNode::Struct(struct_) = arena.owner() else {
            unreachable!("root of a Struct owner is always OwnerNode::Struct");
        };

        self.resolve_generics(struct_id, &struct_.generics);

        self.results.record_self_ty(
            struct_id,
            SelfTyRes::Ty {
                adt: struct_id,
                trait_: None,
            },
        );
        for &field_id in &struct_.fields {
            self.visit_ty(hir.field(field_id).ty);
        }
    }

    pub fn resolve_enums(&mut self, enum_id: DefId) {
        let hir = self.hir;
        let arena = hir.arena(enum_id);
        let OwnerNode::Enum(enum_) = arena.owner() else {
            unreachable!("root of an Enum owner is always OwnerNode::Enum");
        };

        self.resolve_generics(enum_id, &enum_.generics);

        self.results.record_self_ty(
            enum_id,
            SelfTyRes::Ty {
                adt: enum_id,
                trait_: None,
            },
        );
        for &variant_id in &enum_.variants {
            match &hir.variant(variant_id).payload {
                VariantPayload::Unit => {}
                VariantPayload::Type(ty_id) => self.visit_ty(*ty_id),
                VariantPayload::Record(fields) => {
                    for &field_id in fields {
                        self.visit_ty(hir.field(field_id).ty);
                    }
                }
            }
        }
    }

    pub fn resolve_trait(&mut self, trait_id: DefId) {
        let arena = self.hir.arena(trait_id);
        let OwnerNode::Trait(trait_) = arena.owner() else {
            unreachable!("root of a Trait owner is always OwnerNode::Trait");
        };

        self.resolve_generics(trait_id, &trait_.generics);

        // Inside a trait's own default methods, `Self` stands for whatever type eventually
        // implements it -- there's no concrete adt yet, so the trait's own id stands in for both
        // halves of `SelfTyRes::Ty`.
        self.results.record_self_ty(
            trait_id,
            SelfTyRes::Ty {
                adt: trait_id,
                trait_: Some(trait_id),
            },
        );
        for &function_id in &trait_.functions {
            self.visit_function(function_id);
        }
    }

    pub fn resolve_extend(&mut self, extend_id: DefId) {
        let arena = self.hir.arena(extend_id);
        let OwnerNode::Extend(extend) = arena.owner() else {
            unreachable!("root of an Extend owner is always OwnerNode::Extend");
        };

        // Only this first bracket group declares fresh type
        // parameters; `Foo<T>` and `Bar<T>` below apply existing types (possibly these ones)
        // and are resolved as ordinary types.
        self.resolve_generics(extend_id, &extend.extend_generics);

        let adt_res = self.resolve_ty_path(extend_id, &extend.adt_path);

        // Recorded under the block's own node, which is the one `HirId` an `extend` has that
        // nothing else claims a type-position answer for. `SelfTyRes` below keeps only the
        // *successful* half of this -- a `DefId` -- and collapses everything else to
        // `SelfTyRes::Err`, which is enough to lower `Self` but not to say why an `extend` was
        // rejected. Coherence has to tell `extend i32` from `extend Show` from `extend Missing`
        // to word its diagnostic, and staying quiet for the last of the three, so it reads the
        // undiscarded answer from here.
        self.results.record_type(extend.hir_id, adt_res);

        let trait_res = extend
            .trait_path
            .as_ref()
            .map(|path| self.resolve_ty_path(extend_id, path));

        for &ty_id in &extend.adt_generics {
            self.visit_ty(ty_id);
        }
        for &ty_id in &extend.trait_generics {
            self.visit_ty(ty_id);
        }

        // Unlike a struct/enum/trait, an `extend` block's `Self` isn't structural: it's whatever
        // its `adt_path` resolved to just above. Recording `SelfTyRes::Err` when that failed keeps a
        // `Self` inside the block from being reported a second time.
        let self_ty = match adt_res {
            TypeRes::Def(adt) => SelfTyRes::Ty {
                adt,
                trait_: trait_res.and_then(|res| match res {
                    TypeRes::Def(trait_id) => Some(trait_id),
                    _ => None,
                }),
            },
            _ => SelfTyRes::Err,
        };
        self.results.record_self_ty(extend_id, self_ty);

        for &method_id in &extend.methods {
            self.visit_function(method_id);
        }
    }
}
