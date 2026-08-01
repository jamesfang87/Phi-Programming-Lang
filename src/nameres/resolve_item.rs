use std::collections::HashMap;

use crate::ast::interner::Interner;
use crate::ast::Ident;
use crate::diag::{DiagCtx, Diagnostic};
use crate::hir::{DefId, HirId, LocalId, Node, OwnerNode, TyKind, VariantPayload};
use crate::lexer::src_span::SrcSpan;
use crate::nameres::resolve_results::Res;
use crate::nameres::NameResolver;

/// Where an owner's own generic type parameter comes from.
///
/// A `fun`, `struct`, `enum`, or `trait` declares its generics as proper [`Node::Generic`]s,
/// each with a name and its own bounds. An `extend` block instead reuses type syntax for its
/// `<T>` list -- `extend<T> Foo<T> with Bar<T>` parses all three bracket groups as plain type
/// lists, because a type is exactly what the ADT's and trait's groups (`Foo<T>`, `Bar<T>`)
/// actually hold, and the grammar doesn't distinguish that from the block's own group without
/// looking at where it sits. Only the first group, right after `extend`, is ever treated as a
/// declaration (see [`NameResolver::resolve_extend`]); its entries have no bounds and must each
/// be a single bare name, so [`NameResolver::resolve_generics`] still has to validate that shape
/// before it can bind one.
#[derive(Clone, Copy)]
enum GenericDecl {
    Generic(LocalId),
    BareTy(LocalId),
}

impl GenericDecl {
    /// Wraps a `fun`/`struct`/`enum`/`trait`'s own `Vec<LocalId>` of [`Node::Generic`]s, as
    /// [`NameResolver::resolve_generics`] expects.
    fn generics(ids: &[LocalId]) -> Vec<GenericDecl> {
        ids.iter().map(|&id| GenericDecl::Generic(id)).collect()
    }

    /// Wraps an `extend` block's own `<T>` list -- a `Vec<LocalId>` of bare [`Node::Ty`]s -- as
    /// [`NameResolver::resolve_generics`] expects.
    fn bare_tys(ids: &[LocalId]) -> Vec<GenericDecl> {
        ids.iter().map(|&id| GenericDecl::BareTy(id)).collect()
    }
}

impl<'hir> NameResolver<'hir> {
    /// A module's imports are resolved earlier, while [`SymbolTable`](crate::name_res::symbol_table::SymbolTable)
    /// is being built -- see [`SymbolTable::resolve_imports`](crate::name_res::symbol_table::SymbolTable::resolve_imports).
    /// By the time this walk runs, an imported name is already sitting in the importing module's
    /// own namespace, indistinguishable from a name the module declared itself, so this walk
    /// never has to look at `module.imports` at all.
    pub fn resolve_module(&mut self, module_id: DefId) {
        let arena = self.hir.arena(module_id);
        let OwnerNode::Module(module) = arena.owner() else {
            unreachable!("root of a Module owner is always OwnerNode::Module");
        };

        for &item in &module.items {
            match self.hir.owner(item) {
                OwnerNode::Module(_) => self.resolve_module(item),
                OwnerNode::Function(_) => self.resolve_function(item),
                OwnerNode::Struct(_) => self.resolve_struct(item),
                OwnerNode::Enum(_) => self.resolve_enums(item),
                OwnerNode::Trait(_) => self.resolve_trait(item),
                OwnerNode::Extend(_) => self.resolve_extend(item),
                OwnerNode::Closure(_) => {
                    unreachable!(
                        "A module should not contain fields, variants, type params, and closures in the top level"
                    )
                }
            }
        }
    }

    /// Binds each generic type parameter `owner_id` declares for itself -- `function.generics`,
    /// `struct_.generics`, an `extend` block's own `<T>` list, and so on -- into the type
    /// namespace visible inside its own body, and resolves each parameter's trait bounds (if it
    /// has any) against the outer type namespace.
    ///
    /// Must run before anything else in `owner_id`'s own declaration is resolved, since a
    /// parameter list, return type, field, or variant payload may name one of these parameters.
    /// [`NameResolver::generic_ty`] is what later lookups read this table through.
    fn resolve_generics(&mut self, owner_id: DefId, decls: &[GenericDecl]) {
        let arena = self.hir.arena(owner_id);
        let mut params = HashMap::new();
        for &decl in decls {
            let local_id = match decl {
                GenericDecl::Generic(id) | GenericDecl::BareTy(id) => id,
            };
            let hir_id = HirId {
                owner: owner_id,
                local_id,
            };

            let name = match decl {
                GenericDecl::Generic(id) => {
                    let Node::Generic(generic) = arena.get(id) else {
                        unreachable!("Node that is not a generic found in a generics list");
                    };
                    for bound in &generic.bounds {
                        self.resolve_ty_path(owner_id, bound);
                    }
                    generic.name
                }
                GenericDecl::BareTy(id) => {
                    let Node::Ty(ty) = arena.get(id) else {
                        unreachable!("Node that is not a ty found in a bare generics list");
                    };
                    let TyKind::Base { path, args } = &ty.kind else {
                        Self::report_expected_generic_param(ty.span);
                        continue;
                    };
                    let ([name], true) = (path.segments.as_slice(), args.is_empty()) else {
                        Self::report_expected_generic_param(ty.span);
                        continue;
                    };
                    *name
                }
            };

            let res = Res::TyParam(hir_id);
            self.results.add(hir_id, res);
            params.insert(name.text, res);
        }
        self.results.add_generics(owner_id, params);
    }

    /// Reports an `extend` block's own `<...>` list containing something other than a bare type
    /// parameter name, such as `extend<i32> Foo<i32>` -- that group declares fresh parameters,
    /// unlike `Foo<T>`'s and `Bar<T>`'s, which apply existing types (see [`GenericDecl`]).
    fn report_expected_generic_param(span: SrcSpan) {
        DiagCtx::emit(
            Diagnostic::error("expected a generic type parameter name here", span)
                .with_label("not a valid parameter name")
                .with_help(
                    "an `extend` block's own `<...>` list declares fresh type parameters, so \
                     each entry must be a single bare name, such as `T`",
                ),
        );
    }

    pub fn resolve_function(&mut self, fun_id: DefId) {
        let hir = self.hir;
        let arena = hir.arena(fun_id);
        let OwnerNode::Function(function) = arena.owner() else {
            unreachable!("root of a Function owner is always OwnerNode::Function");
        };

        self.resolve_generics(fun_id, &GenericDecl::generics(&function.generics));

        self.symbol_tab.push_scope();
        if let Some(self_param_id) = function.self_param {
            let Node::SelfParam(self_param) = arena.get(self_param_id) else {
                unreachable!("Node that is not a self param found in a function's self param slot");
            };
            let hir_id = HirId {
                owner: fun_id,
                local_id: self_param_id,
            };

            // `self` is bound like a parameter -- it just doesn't carry a name of its own in the
            // HIR, so the keyword is interned here to bind it under. It resolves to
            // `Res::SelfVal` rather than `Res::Local`: its type isn't declared anywhere, it's
            // the enclosing item's `Self`, recovered from `fun_id` the same way
            // [`NameResolver::self_ty`] does.
            let name = Ident {
                text: Interner::intern("self"),
                span: self_param.span,
            };
            self.results.add(hir_id, Res::SelfVal(hir_id));
            self.symbol_tab.bind(name, Res::SelfVal(hir_id));
        }
        for &param_id in &function.params {
            if let Node::Param(param_node) = arena.get(param_id) {
                let hir_id = HirId {
                    owner: fun_id,
                    local_id: param_id,
                };

                self.results.add(hir_id, Res::Local(hir_id));
                self.symbol_tab.bind(param_node.name, Res::Local(hir_id));
                self.resolve_ty(fun_id, param_node.ty);
            } else {
                unreachable!("Node that is not a parameter found in a function's parameter list");
            }
        }
        if let Some(ret) = function.ret {
            self.resolve_ty(fun_id, ret);
        }

        let body = function.body;
        if let Some(body_id) = body {
            self.resolve_block(fun_id, body_id);
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
            let Node::ClosureParam(param) = arena.get(param_id) else {
                unreachable!("Node that is not a closure param found in a closure's param list");
            };
            let hir_id = HirId {
                owner: closure_id,
                local_id: param_id,
            };

            self.results.add(hir_id, Res::Local(hir_id));
            self.symbol_tab.bind(param.name, Res::Local(hir_id));
            if let Some(ty) = param.ty {
                self.resolve_ty(closure_id, ty);
            }
        }
        if let Some(ret) = closure.ret {
            self.resolve_ty(closure_id, ret);
        }
        self.resolve_expr(closure_id, closure.body);
        self.symbol_tab.pop_scope();
    }

    pub fn resolve_struct(&mut self, struct_id: DefId) {
        let hir = self.hir;
        let arena = hir.arena(struct_id);
        let OwnerNode::Struct(struct_) = arena.owner() else {
            unreachable!("root of a Struct owner is always OwnerNode::Struct");
        };

        self.resolve_generics(struct_id, &GenericDecl::generics(&struct_.generics));

        self.results.add_self_ty(
            struct_id,
            Res::SelfTy {
                adt: struct_id,
                trait_: None,
            },
        );
        for &field_id in &struct_.fields {
            let Node::Field(field) = arena.get(field_id) else {
                unreachable!("Node that is not a field found in a struct's field list");
            };
            self.resolve_ty(struct_id, field.ty);
        }
    }

    pub fn resolve_enums(&mut self, enum_id: DefId) {
        let hir = self.hir;
        let arena = hir.arena(enum_id);
        let OwnerNode::Enum(enum_) = arena.owner() else {
            unreachable!("root of an Enum owner is always OwnerNode::Enum");
        };

        self.resolve_generics(enum_id, &GenericDecl::generics(&enum_.generics));

        self.results.add_self_ty(
            enum_id,
            Res::SelfTy {
                adt: enum_id,
                trait_: None,
            },
        );
        for &variant_id in &enum_.variants {
            let Node::Variant(variant) = arena.get(variant_id) else {
                unreachable!("Node that is not a variant found in an enum's variant list");
            };

            match &variant.payload {
                VariantPayload::Unit => {}
                VariantPayload::Type(ty_id) => self.resolve_ty(enum_id, *ty_id),
                VariantPayload::Record(fields) => {
                    for &field_id in fields {
                        let Node::Field(field) = arena.get(field_id) else {
                            unreachable!(
                                "Node that is not a field found in a variant's field list"
                            );
                        };
                        self.resolve_ty(enum_id, field.ty);
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

        self.resolve_generics(trait_id, &GenericDecl::generics(&trait_.generics));

        // Inside a trait's own default methods, `Self` stands for whatever type eventually
        // implements it -- there's no concrete adt yet, so the trait's own id stands in for both
        // halves of `Res::SelfTy`.
        self.results.add_self_ty(
            trait_id,
            Res::SelfTy {
                adt: trait_id,
                trait_: Some(trait_id),
            },
        );
        for &function_id in &trait_.functions {
            self.resolve_function(function_id);
        }
    }

    pub fn resolve_extend(&mut self, extend_id: DefId) {
        let arena = self.hir.arena(extend_id);
        let OwnerNode::Extend(extend) = arena.owner() else {
            unreachable!("root of an Extend owner is always OwnerNode::Extend");
        };

        // Only this first bracket group -- the block's own `<T>` -- declares fresh type
        // parameters; `Foo<T>` and `Bar<T>` below apply existing types (possibly these ones)
        // and are resolved as ordinary types, not declarations. See `GenericDecl`.
        self.resolve_generics(extend_id, &GenericDecl::bare_tys(&extend.extend_generics));

        let adt_res = self.resolve_ty_path(extend_id, &extend.adt_path);

        let trait_res = extend
            .trait_path
            .as_ref()
            .map(|path| self.resolve_ty_path(extend_id, path));

        for &ty_id in &extend.adt_generics {
            self.resolve_ty(extend_id, ty_id);
        }
        for &ty_id in &extend.trait_generics {
            self.resolve_ty(extend_id, ty_id);
        }

        // Unlike a struct/enum/trait, an `extend` block's `Self` isn't structural: it's whatever
        // its `adt_path` resolved to just above. Recording `Res::Err` when that failed keeps a
        // `Self` inside the block from being reported a second time.
        let self_ty = match adt_res {
            Res::Def(adt) => Res::SelfTy {
                adt,
                trait_: trait_res.and_then(|res| match res {
                    Res::Def(trait_id) => Some(trait_id),
                    _ => None,
                }),
            },
            _ => Res::Err,
        };
        self.results.add_self_ty(extend_id, self_ty);

        for &method_id in &extend.methods {
            self.resolve_function(method_id);
        }
    }
}
