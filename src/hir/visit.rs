//! A shared walk over the HIR.
//!
//! Every pass that traverses the tree used to carry its own copy of "what are this node's
//! children", each re-deriving the same structure from the same fields and each free to get it
//! wrong independently. That is not a hypothetical: name resolution reached an `if`'s else branch
//! with the expression walk instead of the block walk and crashed the compiler on every
//! `if`/`else`, and separately never walked a `let`'s type annotation, its `else` block, or a
//! `with` binding's annotation at all -- three subtrees that simply went unresolved. One walk that
//! every pass shares is what makes those a single edit rather than a per-pass oversight.
//!
//! A pass implements [`Visitor`] and overrides only the nodes it cares about. Each `visit_*`
//! defaults to the matching free `walk_*` function, which visits the node's children and nothing
//! else -- so an override that still wants the subtree calls `walk_*` itself, and one that does
//! not simply omits it. Overriding is also how a pass gets *between* the children: name
//! resolution opens a scope, calls `walk_block`, and closes it again.
//!
//! The walk deliberately does not descend into a nested owner. A closure body, a trait's methods
//! and an `extend` block's methods each live in their own arena and are their own `DefId`, and a
//! pass reaches them through [`Visitor::visit_nested_owner`], which does nothing by default.
//! Passes differ on whether they want that -- name resolution follows a closure immediately to
//! resolve its body in the enclosing scope, while type checking collects every signature in the
//! program before it checks any body -- so the choice belongs to the pass rather than the walk.

use crate::hir::{
    AccessArgs, DefId, ExprKind, Hir, HirId, OwnerNode, Payload, PatKind, StmtKind, TyKind,
    VariantPayload,
};

/// A traversal over the HIR. See the [module docs](self) for how overriding works.
pub trait Visitor<'hir>: Sized {
    /// The tree being walked. Every `walk_*` reads the node it was handed through this.
    fn hir(&self) -> &'hir Hir;

    /// Called for a definition that owns an arena of its own: a closure, or a method of a trait
    /// or `extend` block. Does nothing by default; see the [module docs](self) for why the walk
    /// does not descend on its own.
    fn visit_nested_owner(&mut self, _def_id: DefId) {}

    fn visit_module(&mut self, def_id: DefId) {
        walk_module(self, def_id);
    }
    fn visit_item(&mut self, def_id: DefId) {
        walk_item(self, def_id);
    }
    fn visit_function(&mut self, def_id: DefId) {
        walk_function(self, def_id);
    }
    fn visit_struct(&mut self, def_id: DefId) {
        walk_struct(self, def_id);
    }
    fn visit_enum(&mut self, def_id: DefId) {
        walk_enum(self, def_id);
    }
    fn visit_trait(&mut self, def_id: DefId) {
        walk_trait(self, def_id);
    }
    fn visit_extend(&mut self, def_id: DefId) {
        walk_extend(self, def_id);
    }
    fn visit_closure(&mut self, def_id: DefId) {
        walk_closure(self, def_id);
    }

    fn visit_generic(&mut self, id: HirId) {
        let _ = id;
    }
    fn visit_self_param(&mut self, id: HirId) {
        let _ = id;
    }
    fn visit_import(&mut self, id: HirId) {
        let _ = id;
    }
    fn visit_param(&mut self, id: HirId) {
        walk_param(self, id);
    }
    fn visit_closure_param(&mut self, id: HirId) {
        walk_closure_param(self, id);
    }
    fn visit_field(&mut self, id: HirId) {
        walk_field(self, id);
    }
    fn visit_variant(&mut self, id: HirId) {
        walk_variant(self, id);
    }

    fn visit_block(&mut self, id: HirId) {
        walk_block(self, id);
    }
    fn visit_stmt(&mut self, id: HirId) {
        walk_stmt(self, id);
    }
    fn visit_arm(&mut self, id: HirId) {
        walk_arm(self, id);
    }
    fn visit_expr(&mut self, id: HirId) {
        walk_expr(self, id);
    }
    fn visit_pat(&mut self, id: HirId) {
        walk_pat(self, id);
    }
    fn visit_ty(&mut self, id: HirId) {
        walk_ty(self, id);
    }
}

// -----------------------------------------------------------------
// Owners
// -----------------------------------------------------------------

pub fn walk_module<'hir, V: Visitor<'hir>>(v: &mut V, def_id: DefId) {
    let OwnerNode::Module(module) = v.hir().def(def_id) else {
        unreachable!("{def_id:?} does not name a module");
    };

    for &id in &module.imports {
        v.visit_import(id);
    }
    for &item in &module.items {
        v.visit_item(item);
    }
}

/// Dispatches on what kind of definition `def_id` names.
pub fn walk_item<'hir, V: Visitor<'hir>>(v: &mut V, def_id: DefId) {
    match v.hir().def(def_id) {
        OwnerNode::Module(_) => v.visit_module(def_id),
        OwnerNode::Function(_) => v.visit_function(def_id),
        OwnerNode::Struct(_) => v.visit_struct(def_id),
        OwnerNode::Enum(_) => v.visit_enum(def_id),
        OwnerNode::Trait(_) => v.visit_trait(def_id),
        OwnerNode::Extend(_) => v.visit_extend(def_id),
        OwnerNode::Closure(_) => v.visit_closure(def_id),
    }
}

pub fn walk_function<'hir, V: Visitor<'hir>>(v: &mut V, def_id: DefId) {
    let OwnerNode::Function(function) = v.hir().def(def_id) else {
        unreachable!("root of a Function owner is always OwnerNode::Function");
    };
    let (generics, self_param, params, ret, block) = (
        &function.generics,
        function.self_param,
        &function.params,
        function.ret,
        function.block,
    );

    for &id in generics {
        v.visit_generic(id);
    }
    if let Some(id) = self_param {
        v.visit_self_param(id);
    }
    for &id in params {
        v.visit_param(id);
    }
    if let Some(id) = ret {
        v.visit_ty(id);
    }
    if let Some(id) = block {
        v.visit_block(id);
    }
}

pub fn walk_struct<'hir, V: Visitor<'hir>>(v: &mut V, def_id: DefId) {
    let OwnerNode::Struct(struct_) = v.hir().def(def_id) else {
        unreachable!("root of a Struct owner is always OwnerNode::Struct");
    };
    let (generics, fields) = (&struct_.generics, &struct_.fields);

    for &id in generics {
        v.visit_generic(id);
    }
    for &id in fields {
        v.visit_field(id);
    }
}

pub fn walk_enum<'hir, V: Visitor<'hir>>(v: &mut V, def_id: DefId) {
    let OwnerNode::Enum(enum_) = v.hir().def(def_id) else {
        unreachable!("root of an Enum owner is always OwnerNode::Enum");
    };
    let (generics, variants) = (&enum_.generics, &enum_.variants);

    for &id in generics {
        v.visit_generic(id);
    }
    for &id in variants {
        v.visit_variant(id);
    }
}

pub fn walk_trait<'hir, V: Visitor<'hir>>(v: &mut V, def_id: DefId) {
    let OwnerNode::Trait(trait_) = v.hir().def(def_id) else {
        unreachable!("root of a Trait owner is always OwnerNode::Trait");
    };
    let (generics, functions) = (&trait_.generics, &trait_.functions);

    for &id in generics {
        v.visit_generic(id);
    }
    for &method in functions {
        v.visit_nested_owner(method);
    }
}

pub fn walk_extend<'hir, V: Visitor<'hir>>(v: &mut V, def_id: DefId) {
    let OwnerNode::Extend(extend) = v.hir().def(def_id) else {
        unreachable!("root of an Extend owner is always OwnerNode::Extend");
    };
    let (extend_generics, adt_generics, trait_generics, methods) = (
        &extend.extend_generics,
        &extend.adt_generics,
        &extend.trait_generics,
        &extend.methods,
    );

    // The first group declares parameters; the other two apply arguments.
    for &id in extend_generics {
        v.visit_generic(id);
    }
    for &id in adt_generics.iter().chain(trait_generics) {
        v.visit_ty(id);
    }
    for &method in methods {
        v.visit_nested_owner(method);
    }
}

pub fn walk_closure<'hir, V: Visitor<'hir>>(v: &mut V, def_id: DefId) {
    let OwnerNode::Closure(closure) = v.hir().def(def_id) else {
        unreachable!("root of a Closure owner is always OwnerNode::Closure");
    };
    let (params, ret, block) = (&closure.params, closure.ret, closure.block);

    for &id in params {
        v.visit_closure_param(id);
    }
    if let Some(id) = ret {
        v.visit_ty(id);
    }
    v.visit_block(block);
}

// -----------------------------------------------------------------
// Declarations nested in an owner
// -----------------------------------------------------------------

pub fn walk_param<'hir, V: Visitor<'hir>>(v: &mut V, id: HirId) {
    let ty = v.hir().param(id).ty;
    v.visit_ty(ty);
}

pub fn walk_closure_param<'hir, V: Visitor<'hir>>(v: &mut V, id: HirId) {
    if let Some(ty) = v.hir().closure_param(id).ty {
        v.visit_ty(ty);
    }
}

pub fn walk_field<'hir, V: Visitor<'hir>>(v: &mut V, id: HirId) {
    let ty = v.hir().field(id).ty;
    v.visit_ty(ty);
}

pub fn walk_variant<'hir, V: Visitor<'hir>>(v: &mut V, id: HirId) {
    match &v.hir().variant(id).payload {
        VariantPayload::Unit => {}
        VariantPayload::Type(ty) => {
            let ty = *ty;
            v.visit_ty(ty);
        }
        VariantPayload::Record(fields) => {
            for id in fields.clone() {
                v.visit_field(id);
            }
        }
    }
}

// -----------------------------------------------------------------
// Blocks and statements
// -----------------------------------------------------------------

pub fn walk_block<'hir, V: Visitor<'hir>>(v: &mut V, id: HirId) {
    let block = v.hir().block(id);
    let (stmts, expr) = (block.stmts.clone(), block.expr);

    for id in stmts {
        v.visit_stmt(id);
    }
    if let Some(id) = expr {
        v.visit_expr(id);
    }
}

pub fn walk_stmt<'hir, V: Visitor<'hir>>(v: &mut V, id: HirId) {
    match &v.hir().stmt(id).kind {
        StmtKind::Let {
            pat,
            ty,
            init,
            else_block,
            ..
        } => {
            let (pat, ty, init, else_block) = (*pat, *ty, *init, *else_block);
            // The initializer is visited before the pattern binds, so that `let x = x;` reads the
            // outer `x` rather than the one being declared.
            v.visit_expr(init);
            if let Some(ty) = ty {
                v.visit_ty(ty);
            }
            v.visit_pat(pat);
            if let Some(block) = else_block {
                v.visit_block(block);
            }
        }
        StmtKind::With { lends, block } => {
            let lends: Vec<_> = lends.iter().map(|l| (l.pat, l.ty, l.init)).collect();
            let block = *block;
            for (pat, ty, init) in lends {
                v.visit_expr(init);
                if let Some(ty) = ty {
                    v.visit_ty(ty);
                }
                v.visit_pat(pat);
            }
            v.visit_block(block);
        }
        StmtKind::Return(expr) => {
            if let Some(expr) = *expr {
                v.visit_expr(expr);
            }
        }
        StmtKind::Defer(expr) | StmtKind::Expr(expr) => {
            let expr = *expr;
            v.visit_expr(expr);
        }
        StmtKind::Break | StmtKind::Continue | StmtKind::Error => {}
    }
}

pub fn walk_arm<'hir, V: Visitor<'hir>>(v: &mut V, id: HirId) {
    let arm = v.hir().arm(id);
    let (pat, block) = (arm.pat, arm.block);

    v.visit_pat(pat);
    v.visit_block(block);
}

// -----------------------------------------------------------------
// Expressions, patterns, types
// -----------------------------------------------------------------

pub fn walk_expr<'hir, V: Visitor<'hir>>(v: &mut V, id: HirId) {
    match &v.hir().expr(id).kind {
        ExprKind::Unary { operand, .. }
        | ExprKind::Borrow { operand, .. }
        | ExprKind::Try(operand) => {
            let operand = *operand;
            v.visit_expr(operand);
        }
        ExprKind::Binary { lhs, rhs, .. }
        | ExprKind::Assign { lhs, rhs }
        | ExprKind::AssignOp { lhs, rhs, .. } => {
            let (lhs, rhs) = (*lhs, *rhs);
            v.visit_expr(lhs);
            v.visit_expr(rhs);
        }
        ExprKind::Call { callee, args } => {
            let (callee, args) = (*callee, args.clone());
            v.visit_expr(callee);
            for arg in args {
                v.visit_expr(arg);
            }
        }
        ExprKind::Access { base, args, .. } => {
            let base = *base;
            let args = match args {
                AccessArgs::None => Vec::new(),
                AccessArgs::Call(args) => args.clone(),
                AccessArgs::Record(fields) => fields.iter().map(|f| f.value).collect(),
            };
            v.visit_expr(base);
            for arg in args {
                v.visit_expr(arg);
            }
        }
        ExprKind::Index { base, index } => {
            let (base, index) = (*base, *index);
            v.visit_expr(base);
            v.visit_expr(index);
        }
        ExprKind::Ctor { payload, .. } => {
            let values: Vec<_> = payload.iter().map(|f| f.value).collect();
            for value in values {
                v.visit_expr(value);
            }
        }
        ExprKind::Variant { payload, .. } => {
            let values = payload_values(payload);
            for value in values {
                v.visit_expr(value);
            }
        }
        ExprKind::Tuple(elems) => {
            for elem in elems.clone() {
                v.visit_expr(elem);
            }
        }
        ExprKind::Range { lo, hi, .. } => {
            let (lo, hi) = (*lo, *hi);
            for bound in [lo, hi].into_iter().flatten() {
                v.visit_expr(bound);
            }
        }
        ExprKind::If {
            cond,
            then_block,
            else_block,
        } => {
            let (cond, then_block, else_block) = (*cond, *then_block, *else_block);
            v.visit_expr(cond);
            v.visit_block(then_block);
            // Both branches are blocks; an `else if` lowers to `else { if .. }`. Reaching this
            // one with `visit_expr` is what used to crash the compiler.
            if let Some(else_block) = else_block {
                v.visit_block(else_block);
            }
        }
        ExprKind::Match { scrutinee, arms } => {
            let (scrutinee, arms) = (*scrutinee, arms.clone());
            v.visit_expr(scrutinee);
            for arm in arms {
                v.visit_arm(arm);
            }
        }
        ExprKind::Loop { block, .. }
        | ExprKind::Spawn(block)
        | ExprKind::Concurrent(block)
        | ExprKind::Block(block) => {
            let block = *block;
            v.visit_block(block);
        }
        ExprKind::Closure(def_id) => {
            let def_id = *def_id;
            v.visit_nested_owner(def_id);
        }
        ExprKind::Literal(_) | ExprKind::Path(_) | ExprKind::Error => {}
    }
}

pub fn walk_pat<'hir, V: Visitor<'hir>>(v: &mut V, id: HirId) {
    match &v.hir().pat(id).kind {
        PatKind::Variant { payload, .. } => {
            let values = payload_values(payload);
            for value in values {
                v.visit_pat(value);
            }
        }
        PatKind::Tuple(elems) => {
            for elem in elems.clone() {
                v.visit_pat(elem);
            }
        }
        PatKind::Wildcard | PatKind::Binding { .. } | PatKind::Literal(_) | PatKind::Error => {}
    }
}

pub fn walk_ty<'hir, V: Visitor<'hir>>(v: &mut V, id: HirId) {
    match &v.hir().ty(id).kind {
        TyKind::Path { args, .. } => {
            for arg in args.clone() {
                v.visit_ty(arg);
            }
        }
        TyKind::Ref { base, .. } | TyKind::Any(base) => {
            let base = *base;
            v.visit_ty(base);
        }
        TyKind::Tuple(elems) => {
            for elem in elems.clone() {
                v.visit_ty(elem);
            }
        }
        TyKind::Array { elem, len } => {
            let (elem, len) = (*elem, *len);
            v.visit_ty(elem);
            // An array's length is a constant *expression*, not a type.
            if let Some(len) = len {
                v.visit_expr(len);
            }
        }
        TyKind::Function { params, ret } => {
            let (params, ret) = (params.clone(), *ret);
            for param in params {
                v.visit_ty(param);
            }
            if let Some(ret) = ret {
                v.visit_ty(ret);
            }
        }
        TyKind::SelfType | TyKind::Dyn(_) | TyKind::Error => {}
    }
}

/// The nodes a variant's payload holds -- expressions when it is being built, patterns when it is
/// being matched. [`Payload`] is shared between the two, so this is too.
fn payload_values(payload: &Payload) -> Vec<HirId> {
    match payload {
        Payload::None => Vec::new(),
        Payload::Single(value) => vec![*value],
        Payload::Record(fields) => fields.iter().map(|f| f.value).collect(),
    }
}
