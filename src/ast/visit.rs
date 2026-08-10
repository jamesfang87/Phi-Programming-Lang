//! A shared walk over the AST.
//!
//! Every AST pass needs the same answer to "what are this node's children", and until now each
//! pass re-derived it independently. The HIR's own visitor module built the same walk for the HIR
//! and gives the rationale, from the same problem there: name resolution once reached an `if`'s else
//! branch with the expression walk instead of the block walk and crashed the compiler on every
//! `if`/`else`, and separately never walked a `let`'s type annotation, its `else` block, or a
//! `with` binding's annotation at all. One walk that every pass shares turns a later addition to
//! the tree into a compile error here (an unmatched enum variant) rather than a subtree that some
//! passes visit and others silently skip. This module is the same idea applied to the AST, which
//! name resolution is moving onto.
//!
//! A pass implements [`Visitor`] and overrides only the nodes it cares about. Each `visit_*`
//! defaults to the matching free `walk_*` function, which visits the node's children and nothing
//! else -- so an override that still wants the subtree calls `walk_*` itself, and one that does
//! not simply omits it. Overriding is also how a pass gets *between* the children: opening a
//! scope around a block, say, calls `walk_block` bracketed by the scope push and pop.
//!
//! Two things differ from the HIR version, both forced by the AST being a `Box`-linked tree
//! rather than an arena addressed by id:
//!
//! - Every `visit_*`/`walk_*` pair takes a `&'ast` reference to the node itself, not an id --
//!   there is no arena to resolve an id against.
//! - A [`Module`]'s children are `NodeId`s (see `src/ast.rs`), resolved through [`Ast::module`],
//!   so [`Visitor::visit_module`] and [`walk_module`] are the only pair that also take the
//!   [`Ast`].
//!
//! One more difference is specific to closures. The HIR gives a closure its own arena and its own
//! `DefId`, so the HIR's own visitor took that id like every other owner. The AST
//! has no such wrapper: `ExprKind::Closure` is an inline variant holding its params, return type,
//! and body directly, with no `Closure` struct to point at. So [`Visitor::visit_closure`] and
//! [`walk_closure`] here take those three fields separately rather than a single node reference.
//! `visit_closure` still exists as its own hook, not as an arm matched inside an overridden
//! `visit_expr`: a pass that opens a scope per closure needs a hook that fires exactly once per
//! closure, and matching `ExprKind::Closure` inside `visit_expr` would put that check on the hot
//! path of every expression, closures or not.

use crate::ast::{
    AccessArgs, Arm, Ast, Block, ClosureParam, Enum, Expr, ExprKind, Extend, Field, Function,
    Generic, Import, Item, ItemKind, Module, Param, Pat, PatKind, Payload, SelfParam, Stmt,
    StmtKind, Struct, Trait, Ty, TyKind, Variant, VariantPayload,
};

/// A traversal over the AST. See the [module docs](self) for how overriding works.
pub trait Visitor<'ast>: Sized {
    fn visit_module(&mut self, module: &'ast Module, ast: &'ast Ast) {
        walk_module(self, module, ast);
    }
    fn visit_item(&mut self, item: &'ast Item) {
        walk_item(self, item);
    }
    fn visit_function(&mut self, f: &'ast Function) {
        walk_function(self, f);
    }
    fn visit_struct(&mut self, s: &'ast Struct) {
        walk_struct(self, s);
    }
    fn visit_enum(&mut self, e: &'ast Enum) {
        walk_enum(self, e);
    }
    fn visit_trait(&mut self, t: &'ast Trait) {
        walk_trait(self, t);
    }
    fn visit_extend(&mut self, e: &'ast Extend) {
        walk_extend(self, e);
    }
    fn visit_import(&mut self, _import: &'ast Import) {}
    fn visit_generic(&mut self, g: &'ast Generic) {
        walk_generic(self, g);
    }
    fn visit_param(&mut self, p: &'ast Param) {
        walk_param(self, p);
    }
    fn visit_self_param(&mut self, _p: &'ast SelfParam) {}
    fn visit_closure_param(&mut self, p: &'ast ClosureParam) {
        walk_closure_param(self, p);
    }
    fn visit_field(&mut self, f: &'ast Field) {
        walk_field(self, f);
    }
    fn visit_variant(&mut self, v: &'ast Variant) {
        walk_variant(self, v);
    }
    fn visit_block(&mut self, b: &'ast Block) {
        walk_block(self, b);
    }
    fn visit_stmt(&mut self, s: &'ast Stmt) {
        walk_stmt(self, s);
    }
    fn visit_arm(&mut self, a: &'ast Arm) {
        walk_arm(self, a);
    }
    fn visit_expr(&mut self, e: &'ast Expr) {
        walk_expr(self, e);
    }
    fn visit_pat(&mut self, p: &'ast Pat) {
        walk_pat(self, p);
    }
    fn visit_ty(&mut self, t: &'ast Ty) {
        walk_ty(self, t);
    }
    /// A closure's params, return type annotation, and body. Kept as its own hook rather than an
    /// arm inside an overridden `visit_expr` because a pass that opens a scope per closure needs
    /// a hook that fires exactly once per closure, and matching `ExprKind::Closure` inside
    /// `visit_expr` would put that check on the hot path of every expression. See the
    /// [module docs](self) for why the signature takes the three fields separately rather than a
    /// single node: the AST, unlike the HIR, has no `Closure` struct to point at.
    fn visit_closure(
        &mut self,
        params: &'ast [ClosureParam],
        ret: Option<&'ast Ty>,
        body: &'ast Expr,
    ) {
        walk_closure(self, params, ret, body);
    }
}

// -----------------------------------------------------------------
// Modules and items
// -----------------------------------------------------------------

pub fn walk_module<'ast, V: Visitor<'ast>>(v: &mut V, module: &'ast Module, ast: &'ast Ast) {
    for import in &module.imports {
        v.visit_import(import);
    }
    for item in &module.items {
        v.visit_item(item);
    }
    for &child in &module.children {
        v.visit_module(ast.module(child), ast);
    }
}

/// Dispatches on what kind of item `item` is.
pub fn walk_item<'ast, V: Visitor<'ast>>(v: &mut V, item: &'ast Item) {
    match &item.kind {
        // A file's own `module foo::bar;` header is sorted out of `items` before the AST is
        // built (see `Ast::new`), so this arm is here for exhaustiveness rather than because it
        // fires in practice.
        ItemKind::ModuleDecl(_) => {}
        ItemKind::Import(import) => v.visit_import(import),
        ItemKind::Function(f) => v.visit_function(f),
        ItemKind::Struct(s) => v.visit_struct(s),
        ItemKind::Enum(e) => v.visit_enum(e),
        ItemKind::Trait(t) => v.visit_trait(t),
        ItemKind::Extend(e) => v.visit_extend(e),
        ItemKind::Error => {}
    }
}

pub fn walk_function<'ast, V: Visitor<'ast>>(v: &mut V, f: &'ast Function) {
    for g in &f.generics {
        v.visit_generic(g);
    }
    if let Some(self_param) = &f.self_param {
        v.visit_self_param(self_param);
    }
    for p in &f.params {
        v.visit_param(p);
    }
    if let Some(ret) = &f.ret {
        v.visit_ty(ret);
    }
    if let Some(block) = &f.block {
        v.visit_block(block);
    }
}

pub fn walk_struct<'ast, V: Visitor<'ast>>(v: &mut V, s: &'ast Struct) {
    if let Some(generics) = &s.generics {
        for g in generics {
            v.visit_generic(g);
        }
    }
    for f in &s.fields {
        v.visit_field(f);
    }
}

pub fn walk_enum<'ast, V: Visitor<'ast>>(v: &mut V, e: &'ast Enum) {
    if let Some(generics) = &e.generics {
        for g in generics {
            v.visit_generic(g);
        }
    }
    for variant in &e.variants {
        v.visit_variant(variant);
    }
}

pub fn walk_trait<'ast, V: Visitor<'ast>>(v: &mut V, t: &'ast Trait) {
    if let Some(generics) = &t.generics {
        for g in generics {
            v.visit_generic(g);
        }
    }
    for f in &t.functions {
        v.visit_function(f);
    }
}

/// Visits `extend_generics`, `adt_generics`, `trait_generics`, and `methods`. Deliberately does
/// not visit `adt_path` or `trait_path`: a `Path` is not itself a visitable node, and a pass that
/// needs what an `extend` block extends or implements reads those two fields directly off the
/// `Extend`.
pub fn walk_extend<'ast, V: Visitor<'ast>>(v: &mut V, e: &'ast Extend) {
    if let Some(generics) = &e.extend_generics {
        for g in generics {
            v.visit_generic(g);
        }
    }
    if let Some(generics) = &e.adt_generics {
        for ty in generics {
            v.visit_ty(ty);
        }
    }
    if let Some(generics) = &e.trait_generics {
        for ty in generics {
            v.visit_ty(ty);
        }
    }
    for m in &e.methods {
        v.visit_function(m);
    }
}

// -----------------------------------------------------------------
// Declarations nested in an item
// -----------------------------------------------------------------

/// A generic parameter has nothing to visit: its bounds are `Path`s, and a `Path` is not itself a
/// visitable node. Exists for the same reason `visit_generic` has a default at all -- so a caller
/// that wants the "no children" behavior spelled out can call it, and so the free-function
/// convention holds uniformly across every declaration kind.
pub fn walk_generic<'ast, V: Visitor<'ast>>(_v: &mut V, _g: &'ast Generic) {}

pub fn walk_param<'ast, V: Visitor<'ast>>(v: &mut V, p: &'ast Param) {
    v.visit_ty(&p.ty);
}

/// A `self` parameter has nothing to visit: its binding mode (`&self`, `&mut self`, ...) is not
/// itself a node.
pub fn walk_self_param<'ast, V: Visitor<'ast>>(_v: &mut V, _p: &'ast SelfParam) {}

pub fn walk_closure_param<'ast, V: Visitor<'ast>>(v: &mut V, p: &'ast ClosureParam) {
    if let Some(ty) = &p.ty {
        v.visit_ty(ty);
    }
}

pub fn walk_field<'ast, V: Visitor<'ast>>(v: &mut V, f: &'ast Field) {
    v.visit_ty(&f.ty);
}

pub fn walk_variant<'ast, V: Visitor<'ast>>(v: &mut V, variant: &'ast Variant) {
    match &variant.payload {
        VariantPayload::Unit => {}
        VariantPayload::Type(ty) => v.visit_ty(ty),
        VariantPayload::Record(fields) => {
            for f in fields {
                v.visit_field(f);
            }
        }
    }
}

// -----------------------------------------------------------------
// Blocks and statements
// -----------------------------------------------------------------

pub fn walk_block<'ast, V: Visitor<'ast>>(v: &mut V, block: &'ast Block) {
    for stmt in &block.stmts {
        v.visit_stmt(stmt);
    }
}

pub fn walk_stmt<'ast, V: Visitor<'ast>>(v: &mut V, stmt: &'ast Stmt) {
    match &stmt.kind {
        StmtKind::While { cond, block } => {
            v.visit_expr(cond);
            v.visit_block(block);
        }
        StmtKind::WhileLet {
            pat,
            scrutinee,
            block,
        } => {
            v.visit_expr(scrutinee);
            v.visit_pat(pat);
            v.visit_block(block);
        }
        StmtKind::For { pat, iter, block } => {
            v.visit_expr(iter);
            v.visit_pat(pat);
            v.visit_block(block);
        }
        StmtKind::Return(expr) | StmtKind::Defer(expr) => {
            v.visit_expr(expr);
        }
        StmtKind::Let {
            pat,
            ty,
            init,
            else_block,
            ..
        } => {
            // The initializer is visited before the pattern binds, so that `let x = x;` reads
            // the outer `x` rather than the one being declared.
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
            for lend in lends {
                v.visit_expr(&lend.init);
                if let Some(ty) = &lend.ty {
                    v.visit_ty(ty);
                }
                v.visit_pat(&lend.pat);
            }
            v.visit_block(block);
        }
        StmtKind::Expr { expr, .. } => v.visit_expr(expr),
        StmtKind::Break | StmtKind::Continue | StmtKind::Error => {}
    }
}

pub fn walk_arm<'ast, V: Visitor<'ast>>(v: &mut V, arm: &'ast Arm) {
    v.visit_pat(&arm.pat);
    v.visit_expr(&arm.body);
}

// -----------------------------------------------------------------
// Expressions, patterns, types
// -----------------------------------------------------------------

pub fn walk_expr<'ast, V: Visitor<'ast>>(v: &mut V, expr: &'ast Expr) {
    match &expr.kind {
        ExprKind::Unary { operand, .. }
        | ExprKind::Borrow { operand, .. }
        | ExprKind::Try(operand) => {
            v.visit_expr(operand);
        }
        ExprKind::Binary { lhs, rhs, .. }
        | ExprKind::Assign { lhs, rhs }
        | ExprKind::AssignOp { lhs, rhs, .. } => {
            v.visit_expr(lhs);
            v.visit_expr(rhs);
        }
        ExprKind::Call { callee, args } => {
            v.visit_expr(callee);
            for arg in args {
                v.visit_expr(arg);
            }
        }
        ExprKind::Access { base, args, .. } => {
            v.visit_expr(base);
            match args {
                AccessArgs::None => {}
                AccessArgs::Call(args) => {
                    for arg in args {
                        v.visit_expr(arg);
                    }
                }
                AccessArgs::Record(fields) => {
                    for field in fields {
                        if let Some(value) = &field.value {
                            v.visit_expr(value);
                        }
                    }
                }
            }
        }
        ExprKind::Index { base, index } => {
            v.visit_expr(base);
            v.visit_expr(index);
        }
        ExprKind::Ctor { payload, .. } => {
            for field in payload {
                if let Some(value) = &field.value {
                    v.visit_expr(value);
                }
            }
        }
        ExprKind::Variant { payload, .. } => {
            for value in payload_values(payload) {
                v.visit_expr(value);
            }
        }
        ExprKind::Tuple(elems) => {
            for elem in elems {
                v.visit_expr(elem);
            }
        }
        ExprKind::Range { lo, hi, .. } => {
            if let Some(lo) = lo {
                v.visit_expr(lo);
            }
            if let Some(hi) = hi {
                v.visit_expr(hi);
            }
        }
        ExprKind::If {
            cond,
            then_block,
            else_expr,
        } => {
            v.visit_expr(cond);
            v.visit_block(then_block);
            if let Some(else_expr) = else_expr {
                v.visit_expr(else_expr);
            }
        }
        ExprKind::IfLet {
            pat,
            scrutinee,
            then_block,
            else_expr,
        } => {
            v.visit_expr(scrutinee);
            v.visit_pat(pat);
            v.visit_block(then_block);
            if let Some(else_expr) = else_expr {
                v.visit_expr(else_expr);
            }
        }
        ExprKind::Match { scrutinee, arms } => {
            v.visit_expr(scrutinee);
            for arm in arms {
                v.visit_arm(arm);
            }
        }
        ExprKind::Spawn(block) | ExprKind::Concurrent(block) | ExprKind::Block(block) => {
            v.visit_block(block);
        }
        ExprKind::Closure { params, ret, body } => {
            v.visit_closure(params, ret.as_ref(), body);
        }
        ExprKind::Literal(_) | ExprKind::Path(_) | ExprKind::Error => {}
    }
}

pub fn walk_pat<'ast, V: Visitor<'ast>>(v: &mut V, pat: &'ast Pat) {
    match &pat.kind {
        PatKind::Variant { payload, .. } => {
            for value in payload_values(payload) {
                v.visit_pat(value);
            }
        }
        PatKind::Tuple(elems) => {
            for elem in elems {
                v.visit_pat(elem);
            }
        }
        PatKind::Wildcard | PatKind::Binding(_) | PatKind::Literal(_) | PatKind::Error => {}
    }
}

/// An array's length is a constant *expression*, not a type.
pub fn walk_ty<'ast, V: Visitor<'ast>>(v: &mut V, ty: &'ast Ty) {
    match &ty.kind {
        TyKind::Path { args, .. } | TyKind::Dyn { args, .. } => {
            for arg in args {
                v.visit_ty(arg);
            }
        }
        TyKind::Ref { base, .. } | TyKind::Any(base) => {
            v.visit_ty(base);
        }
        TyKind::Tuple(elems) => {
            for elem in elems {
                v.visit_ty(elem);
            }
        }
        TyKind::Array { elem, len } => {
            v.visit_ty(elem);
            if let Some(len) = len {
                v.visit_expr(len);
            }
        }
        TyKind::Function { params, ret } => {
            for param in params {
                v.visit_ty(param);
            }
            if let Some(ret) = ret {
                v.visit_ty(ret);
            }
        }
        TyKind::Error => {}
    }
}

pub fn walk_closure<'ast, V: Visitor<'ast>>(
    v: &mut V,
    params: &'ast [ClosureParam],
    ret: Option<&'ast Ty>,
    body: &'ast Expr,
) {
    for p in params {
        v.visit_closure_param(p);
    }
    if let Some(ret) = ret {
        v.visit_ty(ret);
    }
    v.visit_expr(body);
}

/// The nodes a variant's payload holds -- expressions when it is being built
/// ([`ExprKind::Variant`]), patterns when it is being matched ([`PatKind::Variant`]). [`Payload`]
/// is shared between the two, so this is too.
fn payload_values<T>(payload: &Payload<T>) -> Vec<&T> {
    match payload {
        Payload::None => Vec::new(),
        Payload::Single(value) => vec![value.as_ref()],
        Payload::Record(fields) => fields.iter().filter_map(|f| f.value.as_ref()).collect(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testing::parse_src;

    fn ast_from(src: &str) -> Ast {
        Ast::new(vec![parse_src(src)])
    }

    #[derive(Default)]
    struct Counter {
        tys: usize,
        exprs: usize,
        pats: usize,
        generics: usize,
        params: usize,
        fields: usize,
        /// Counts `visit_closure` calls, one per closure. Kept separate from `exprs` because a
        /// closure's `ExprKind::Closure` arm dispatches to `visit_closure` rather than being
        /// counted as an ordinary expression a second time.
        closures: usize,
        closure_params: usize,
    }

    impl<'ast> Visitor<'ast> for Counter {
        fn visit_ty(&mut self, ty: &'ast Ty) {
            self.tys += 1;
            walk_ty(self, ty);
        }
        fn visit_expr(&mut self, e: &'ast Expr) {
            self.exprs += 1;
            walk_expr(self, e);
        }
        fn visit_pat(&mut self, p: &'ast Pat) {
            self.pats += 1;
            walk_pat(self, p);
        }
        fn visit_generic(&mut self, g: &'ast Generic) {
            self.generics += 1;
            walk_generic(self, g);
        }
        fn visit_param(&mut self, p: &'ast Param) {
            self.params += 1;
            walk_param(self, p);
        }
        fn visit_field(&mut self, f: &'ast Field) {
            self.fields += 1;
            walk_field(self, f);
        }
        fn visit_closure(
            &mut self,
            params: &'ast [ClosureParam],
            ret: Option<&'ast Ty>,
            body: &'ast Expr,
        ) {
            self.closures += 1;
            walk_closure(self, params, ret, body);
        }
        fn visit_closure_param(&mut self, p: &'ast ClosureParam) {
            self.closure_params += 1;
            walk_closure_param(self, p);
        }
    }

    #[test]
    fn the_walk_reaches_a_lets_annotation_its_initializer_and_its_pattern() {
        let ast = ast_from("fun f() { let x: i32 = 1; }");
        let mut c = Counter::default();
        c.visit_module(ast.root(), &ast);
        assert_eq!(c.tys, 1, "the `let`'s annotation was not visited");
        assert_eq!(c.exprs, 1, "the initializer was not visited");
        assert_eq!(c.pats, 1, "the binding pattern was not visited");
    }

    #[test]
    fn the_walk_reaches_generics_params_and_fields() {
        let ast = ast_from("struct S { a: i32 } fun f<T>(x: T) {}");
        let mut c = Counter::default();
        c.visit_module(ast.root(), &ast);
        assert_eq!(c.generics, 1);
        assert_eq!(c.params, 1);
        assert_eq!(c.fields, 1);
    }

    #[test]
    fn the_walk_reaches_an_extend_blocks_generics_and_methods() {
        let ast = ast_from("struct S {} extend<T> S { fun get(self) -> T { return self; } }");
        let mut c = Counter::default();
        c.visit_module(ast.root(), &ast);
        assert!(c.generics >= 1, "extend's own generics were not visited");
        assert!(c.tys >= 1, "the method's return type was not visited");
    }

    #[test]
    fn the_walk_reaches_a_match_arms_pattern_and_body() {
        let ast = ast_from("fun f(e: i32) { match e { 1 => 2, } }");
        let mut c = Counter::default();
        c.visit_module(ast.root(), &ast);
        assert!(c.pats >= 1, "the arm's pattern was not visited");
    }

    /// The one place this module departs from the HIR visitor's literal shape: `ExprKind::Closure`
    /// has no wrapping node, so `visit_closure` takes its params, return annotation, and body as
    /// separate arguments. Asserting *exact* counts, not just "at least one", is the point: a
    /// `walk_expr` that both called `visit_closure` and separately looped the params would
    /// double-visit, and only an exact count catches that.
    #[test]
    fn the_walk_reaches_a_closures_params_return_type_and_body_exactly_once() {
        let ast = ast_from("fun f() { let g = |x: i32| -> i32 { x }; }");
        let mut c = Counter::default();
        c.visit_module(ast.root(), &ast);
        assert_eq!(
            c.closures, 1,
            "visit_closure should fire exactly once per closure"
        );
        assert_eq!(
            c.closure_params, 1,
            "the closure's one param should be visited exactly once"
        );
        assert_eq!(
            c.tys, 2,
            "the param's annotation and the return annotation should each be visited exactly once"
        );
        assert_eq!(
            c.exprs, 3,
            "the closure itself, its body block, and the body's inner expression should each be \
             visited exactly once"
        );
    }

    #[test]
    fn the_walk_reaches_a_with_lends_annotation_pattern_and_initializer() {
        // The old HIR-side resolver skipped a `with` lend's type annotation silently; this
        // guards against that gap reappearing here.
        let ast = ast_from("fun f() { with x: i32 = 1 { g(x); } }");
        let mut c = Counter::default();
        c.visit_module(ast.root(), &ast);
        assert_eq!(c.tys, 1, "the with lend's type annotation was not visited");
        assert_eq!(c.pats, 1, "the with lend's pattern was not visited");
        assert_eq!(
            c.exprs, 4,
            "the lend's initializer, the call, its callee, and its argument should each be \
             visited exactly once"
        );
    }

    #[test]
    fn the_walk_reaches_an_if_lets_pattern_scrutinee_and_both_branches() {
        let ast = ast_from("fun f(o: i32) { if let .some(x) = o { x } else { 0 } }");
        let mut c = Counter::default();
        c.visit_module(ast.root(), &ast);
        assert_eq!(
            c.pats, 2,
            "the if-let's pattern and its nested payload binding should each be visited exactly \
             once"
        );
        assert_eq!(
            c.exprs, 5,
            "the scrutinee, the then-branch, and the else-branch should each be visited exactly \
             once"
        );
    }
}
