//! The AST walk that populates `NameResolutions`.
//!
//! The traversal is `ast::visit`'s, so "what are this node's children" is answered in one
//! place for every AST pass rather than re-derived here. Only the nodes that need something
//! *around* or *instead of* the default walk are overridden: a block opens a scope, a path
//! records what it named, a binding pattern binds. Everything else (expressions with no path
//! of their own, a struct's fields, a variant's payload) uses `ast::visit`'s defaults untouched.

use std::collections::HashMap;

use crate::ast::interner::Interner;
use crate::ast::visit::{self, Visitor};
use crate::ast::{
    AccessArgs, Arm, Ast, Block, ClosureParam, Enum, Expr, ExprKind, Extend, Function, Generic,
    Ident, Item, NodeId, Pat, PatKind, Path, Payload, PayloadField, Stmt, StmtKind, Struct, Trait,
    Ty, TyKind,
};
use crate::driver::source::SrcSpan;
use crate::nameres::diagnostics::{report_duplicate_bound, report_not_found, report_self_extend};
use crate::nameres::res::{Local, Res, TyDef, Type};
use crate::nameres::results::NameResolutions;
use crate::nameres::symbol_table::SymbolTable;

/// Manages [`SymbolTable`]'s three scope stacks across the whole AST, recording an entry in
/// [`NameResolutions`] for every path written in the program.
struct Resolver<'ast> {
    table: SymbolTable<'ast>,
    results: NameResolutions,
    /// The module the node currently being walked is written in. `SymbolTable`'s lookups take
    /// this as `from`. Unlike on the HIR side, the traversal already tracks it: [`resolve`]
    /// updates it once per module, before that module's items are visited. No separate `module_of`
    /// walk is needed.
    module: NodeId,
    /// The `Item` currently being walked, if any. `Struct`, `Enum`, `Trait`, and `Extend` carry
    /// no `NodeId` (they sit inside `Item`, which does; see `src/ast.rs:85`).
    /// [`Visitor::visit_struct`]/`visit_enum`/`visit_trait`/`visit_extend` use this to know
    /// which node owns their path entries and `Self` scope. [`Visitor::visit_item`] sets it
    /// before dispatching. Since no `Item` nests inside another, this is a simple value, not a
    /// stack.
    current_item: Option<NodeId>,
}

/// The entry point for the debug dump (`--surface-nameres`, see `crate::driver::emit_debug`) and
/// pipeline (`crate::driver::pipeline`). Walks every module of `ast`, manages `SymbolTable`'s
/// scope stacks, and returns the completed `NameResolutions`.
pub fn resolve(ast: &Ast) -> NameResolutions {
    let table = SymbolTable::new(ast);
    let mut r = Resolver {
        module: ast.root_id(),
        table,
        results: NameResolutions::new(),
        current_item: None,
    };
    for mod_id in ast.mod_ids() {
        r.module = mod_id;
        r.resolve_module(ast, mod_id);
    }
    // Lang items can only be collected while the symbol table exists, but every consumer of
    // them is a later pass. See `langitems::collect_ast`.
    let lang_items = crate::langitems::collect_ast(&r.table, ast.root_id());
    r.results.record_lang_items(lang_items);
    r.results
}

impl<'ast> Resolver<'ast> {
    /// Visits every item `module_id` declares directly.
    ///
    /// Not a `Visitor::visit_module` override: `ast.mod_ids()` (see [`resolve`]) already yields
    /// every module flat. Recursing into `Module::children` here, as `ast::visit::walk_module`
    /// does, would visit every module's items twice.
    fn resolve_module(&mut self, ast: &'ast Ast, module_id: NodeId) {
        for item in &ast.module(module_id).items {
            self.visit_item(item);
        }
    }

    /// Opens a generic scope binding every one of `generics`, then resolves each one's bounds
    /// against it. A bound can see every sibling generic the same `<...>` clause declares,
    /// not just the ones written before it in source order.
    fn push_generics(&mut self, generics: &'ast [Generic]) {
        let params: HashMap<_, _> = generics
            .iter()
            .map(|g| (g.name.text, Type::Generic(g.id)))
            .collect();
        self.table.push_generics(params);
        for g in generics {
            self.resolve_bounds(g);
        }
    }

    /// Pushes generics for the `Option<Vec<Generic>>` shape `Struct`/`Enum`/`Trait`/`Extend`
    /// declare. When generics are present, `Some(Vec<Generic>)` wraps them; `None` indicates
    /// no `<...>` clause. `Function::generics` is always a `Vec`, never `Option`.
    fn push_generics_opt(&mut self, generics: &'ast Option<Vec<Generic>>) {
        self.push_generics(generics.as_deref().unwrap_or(&[]));
    }

    /// Records each of `g`'s bounds as an entry on `g` itself, in source order.
    ///
    /// Skips any bound that repeats one already recorded (e.g., `T: Show + Show`). Reports it
    /// as a duplicate. `NameResolutions::record`'s no-two-equal-paths invariant is a
    /// `debug_assert!` that compiles out in release. This check enforces it for the one case
    /// under this resolver's control.
    fn resolve_bounds(&mut self, g: &'ast Generic) {
        let Some(bounds) = &g.bounds else {
            return;
        };
        for bound in bounds {
            if self.results.get(g.id, bound).is_some() {
                report_duplicate_bound(
                    *bound
                        .segments
                        .last()
                        .expect("a path always has at least one segment"),
                );
                continue;
            }
            let res = self.table.resolve_type_path(self.module, bound);
            self.results.record(g.id, bound.clone(), res);
        }
    }

    /// Visits an expression payload's fields (a `Ctor`'s, an `Access`'s record args, or a
    /// `Variant`'s record payload): each field with an explicit value visits it as an ordinary
    /// expression, and each shorthand field (`{ l }`, meaning `{ l: l }`) resolves what its
    /// implicit value names.
    ///
    /// Not delegated to `ast::visit::walk_expr`. The default walk visits field values via
    /// [`crate::ast::visit::Visitor::visit_expr`] (or a helper for `Payload`), which works for
    /// fields with explicit values. Shorthand fields have no `Expr` behind them. Instead, resolve
    /// them as a value-position lookup, keyed on `PayloadField::id` rather than an `Expr`'s node
    /// ID. Every field, shorthand or not, carries a `NodeId`. This mirrors HIR lowering's
    /// desugaring of shorthand into `{ l: l }` (`src/hir/lower/expr.rs`), recorded here instead
    /// of synthesized there.
    fn visit_record_fields(&mut self, fields: &'ast [PayloadField<Expr>]) {
        for field in fields {
            match &field.value {
                Some(value) => self.visit_expr(value),
                None => {
                    let path = single_segment_path(field.name);
                    let res = self
                        .table
                        .lookup_value_path(self.module, &path)
                        .unwrap_or_else(|| {
                            report_not_found(field.name);
                            Res::Err
                        });
                    self.results.record(field.id, path, res);
                }
            }
        }
    }

    /// Visits a record pattern's shorthand fields. A shorthand like `{ l }` binds `l`, as
    /// `PatKind::Binding` would, but there is no `Pat` behind it. The field's `NodeId` stands
    /// in as the binding site, just as [`Self::visit_record_fields`] uses it for lookups on the
    /// expression side.
    fn visit_record_pat_fields(&mut self, fields: &'ast [PayloadField<Pat>]) {
        for field in fields {
            match &field.value {
                Some(value) => self.visit_pat(value),
                None => self
                    .table
                    .insert_local(field.name, Local::Variable(field.id)),
            }
        }
    }

    /// Visits an expression `Payload`: either a single value (`.circle(1.24)`) or record fields
    /// (`.square { l }`). Record fields go through [`Self::visit_record_fields`] instead of
    /// the default's value-only walk, so shorthand fields get resolved.
    fn visit_expr_payload(&mut self, payload: &'ast Payload<Expr>) {
        match payload {
            Payload::None => {}
            Payload::Single(value) => self.visit_expr(value),
            Payload::Record(fields) => self.visit_record_fields(fields),
        }
    }

    /// Visits a pattern's payload, handling all variants the same way [`Self::visit_expr_payload`] does.
    fn visit_pat_payload(&mut self, payload: &'ast Payload<Pat>) {
        match payload {
            Payload::None => {}
            Payload::Single(value) => self.visit_pat(value),
            Payload::Record(fields) => self.visit_record_pat_fields(fields),
        }
    }
}

/// Builds a single-segment [`Path`] for `ident`. Used when looking up a name that has no `Path`
/// in the source, such as a record payload shorthand field's implicit value.
fn single_segment_path(ident: Ident) -> Path {
    Path {
        segments: vec![ident],
        span: ident.span,
    }
}

/// Constructs an `Ident` for `self`. `ast::SelfParam` carries no `Ident`, only a `SelfMode`,
/// since the parser does not need to spell the name.
fn self_ident(span: SrcSpan) -> Ident {
    Ident {
        text: Interner::intern("self"),
        span,
    }
}

impl<'ast> Visitor<'ast> for Resolver<'ast> {
    // -----------------------------------------------------------------
    // Items -- each pushes whatever scopes it introduces, walks its children through the
    // matching `walk_*`, then pops. `current_item` is what lets `Struct`/`Enum`/`Trait`/
    // `Extend`, none of which carry a `NodeId` of their own, record path entries and a `Self`
    // scope against the right owner.
    // -----------------------------------------------------------------

    fn visit_item(&mut self, item: &'ast Item) {
        self.current_item = Some(item.id);
        visit::walk_item(self, item);
    }

    /// A function pushes its own generic scope. It does not push a `Self` scope: `Self` inside a
    /// method or closure refers to the enclosing struct/enum/trait/`extend`, already on the
    /// stack (see `SymbolTable::current_self`).
    ///
    /// Parameter and return types are resolved before the function's local scope opens. They
    /// can see the generics just pushed, but not `self` or the parameters themselves (those are
    /// bound after all signature types have been read). `self` and each parameter are inserted
    /// directly, bypassing `visit_self_param`/`visit_param` (which already ran to resolve types),
    /// to avoid re-walking a type.
    fn visit_function(&mut self, f: &'ast Function) {
        self.push_generics(&f.generics);

        for p in &f.params {
            self.visit_param(p);
        }
        if let Some(ret) = &f.ret {
            self.visit_ty(ret);
        }

        self.table.push_scope();
        if let Some(self_param) = &f.self_param {
            self.table
                .insert_local(self_ident(self_param.span), Local::SelfParam(self_param.id));
        }
        for p in &f.params {
            self.table.insert_local(p.name, Local::Param(p.id));
        }
        if let Some(block) = &f.block {
            self.visit_block(block);
        }
        self.table.pop_scope();

        self.table.pop_generics();
    }

    fn visit_struct(&mut self, s: &'ast Struct) {
        let item_id = self
            .current_item
            .expect("visit_struct is reached only through visit_item, which sets current_item");
        self.table.push_self(TyDef::Struct(item_id));
        self.push_generics_opt(&s.generics);
        visit::walk_struct(self, s);
        self.table.pop_generics();
        self.table.pop_self();
    }

    fn visit_enum(&mut self, e: &'ast Enum) {
        let item_id = self
            .current_item
            .expect("visit_enum is reached only through visit_item, which sets current_item");
        self.table.push_self(TyDef::Enum(item_id));
        self.push_generics_opt(&e.generics);
        visit::walk_enum(self, e);
        self.table.pop_generics();
        self.table.pop_self();
    }

    fn visit_trait(&mut self, t: &'ast Trait) {
        let item_id = self
            .current_item
            .expect("visit_trait is reached only through visit_item, which sets current_item");
        self.table.push_self(TyDef::Trait(item_id));
        self.push_generics_opt(&t.generics);
        visit::walk_trait(self, t);
        self.table.pop_generics();
        self.table.pop_self();
    }

    /// Resolves `adt_path` and `trait_path` against `item_id` (the `Extend` has no `NodeId` of
    /// its own to record them under), guards the `extend Foo with Foo` duplicate-path case, then
    /// pushes generics and `Self` for the method bodies.
    ///
    /// Three outcomes for `adt_path`, each with different behavior:
    /// - It resolved to a `TyDef` (`extend Foo with Show`): push that `Self`.
    /// - It resolved to something else, e.g. a primitive (`extend i32 with Show`): push nothing.
    ///   No diagnostic was raised for `adt_path` itself, so `Self` inside the block reports
    ///   "not available" (same as with no enclosing `extend`). This prevents duplicate errors.
    /// - It failed to resolve (`extend Nope with Show`): push a suppressed scope
    ///   ([`SymbolTable::push_self_unresolved`]). `resolve_type_path` already reported the
    ///   failure. A `Self` inside would otherwise duplicate that diagnostic.
    fn visit_extend(&mut self, e: &'ast Extend) {
        let item_id = self
            .current_item
            .expect("visit_extend is reached only through visit_item, which sets current_item");

        // The block's own generics have to be in scope before `adt_path`/`trait_path` are
        // resolved, not just before the method bodies are walked: `extend<T> T with Show` names
        // the type parameter `T` as the extended type itself, and `extend<T> Box<T>` names it as
        // a generic argument inside `adt_path`. Both are written in the same source span as the
        // `<T>` that introduces them, so both need it pushed first.
        self.push_generics_opt(&e.extend_generics);

        let adt_res = self.table.resolve_type_path(self.module, &e.adt_path);
        self.results.record(item_id, e.adt_path.clone(), adt_res);

        if let Some(trait_path) = &e.trait_path {
            if *trait_path == e.adt_path {
                // `extend Foo with Foo`: recording both would put two textually identical paths
                // on `item_id`, which `NameResolutions::record`'s invariant forbids. `adt_path`
                // is already recorded above, so only the second writing is reported.
                report_self_extend(
                    *trait_path
                        .segments
                        .last()
                        .expect("a path always has at least one segment"),
                );
            } else {
                let trait_res = self.table.resolve_type_path(self.module, trait_path);
                self.results.record(item_id, trait_path.clone(), trait_res);
            }
        }

        let pushed_self = match adt_res {
            Res::Type(Type::Def(def)) => {
                self.table.push_self(def);
                true
            }
            Res::Err => {
                self.table.push_self_unresolved();
                true
            }
            _ => false,
        };

        visit::walk_extend(self, e);

        if pushed_self {
            self.table.pop_self();
        }
        self.table.pop_generics();
    }

    // -----------------------------------------------------------------
    // Types -- a path in type position records what it named, then its own children (generic
    // arguments, array length, ...) are walked exactly as `ast::visit`'s default would.
    // -----------------------------------------------------------------

    fn visit_ty(&mut self, ty: &'ast Ty) {
        match &ty.kind {
            TyKind::Path { path, .. } => {
                let res = self.table.resolve_type_path(self.module, path);
                self.results.record(ty.id, path.clone(), res);
            }
            TyKind::Dyn { path, .. } => {
                let res = self.table.lookup_dyn_path(self.module, path);
                self.results.record(ty.id, path.clone(), res);
            }
            TyKind::Ref { .. }
            | TyKind::Any(_)
            | TyKind::Tuple(_)
            | TyKind::Array { .. }
            | TyKind::Function { .. }
            | TyKind::Error => {}
        }
        visit::walk_ty(self, ty);
    }

    // -----------------------------------------------------------------
    // Expressions, statements, patterns -- blocks and match arms open a local scope; a `Let`'s
    // ordering (initializer before the pattern binds) is already `ast::visit::walk_stmt`'s
    // default, so `let x = x;` reads the outer `x` without any extra work here.
    // -----------------------------------------------------------------

    /// `Ctor`, `Access`, and `Variant` return here without reaching the trailing `walk_expr`
    /// call. Other arms extend the default recording and still need `walk_expr` for child nodes.
    /// Why the difference: `PayloadField`. Record payloads need a field-aware walk
    /// (see [`Self::visit_record_fields`]) to handle shorthand fields. The default's
    /// `payload_values` helper skips fields without explicit values. Falling through to
    /// `walk_expr` would visit explicit-value fields again.
    fn visit_expr(&mut self, expr: &'ast Expr) {
        match &expr.kind {
            ExprKind::Path(path) => {
                let res = self
                    .table
                    .lookup_value_path(self.module, path)
                    .unwrap_or_else(|| {
                        report_not_found(
                            *path
                                .segments
                                .last()
                                .expect("a path always has at least one segment"),
                        );
                        Res::Err
                    });
                self.results.record(expr.id, path.clone(), res);
            }
            ExprKind::Ctor { path, payload } => {
                match path {
                    Some(path) => {
                        // A struct literal names its type, so its path is resolved in type position
                        // (like `TyKind::Path`). Although written on an expression, `Foo` here
                        // names the struct, not a value called `Foo`.
                        let res = self.table.resolve_type_path(self.module, path);
                        self.results.record(expr.id, path.clone(), res);
                    }
                    None => {
                        // The elided form, `.{ a: 1 }`, takes its type from context. There is no
                        // path here, so nothing is recorded.
                    }
                }
                self.visit_record_fields(payload);
                return;
            }
            ExprKind::Access { base, args, .. } => {
                // `member`'s reading as a field, a method, or a payload-carrying variant is not
                // resolved here. See the `AccessArgs` doc comment: the grammar can't tell the
                // three apart. Later analysis (typeck, once `base`'s type is known) does.
                self.visit_expr(base);
                match args {
                    AccessArgs::None => {}
                    AccessArgs::Call(args) => {
                        for arg in args {
                            self.visit_expr(arg);
                        }
                    }
                    AccessArgs::Record(fields) => self.visit_record_fields(fields),
                }
                return;
            }
            ExprKind::Variant { payload, .. } => {
                // `variant`'s own name is not resolved here. A bare `.variant` names no enum
                // until typeck knows the expected type. Scanning every enum in scope for a
                // matching name is the ambiguity the leading `.` avoids.
                self.visit_expr_payload(payload);
                return;
            }
            _ => {}
        }
        visit::walk_expr(self, expr);
    }

    fn visit_block(&mut self, block: &'ast Block) {
        self.table.push_scope();
        visit::walk_block(self, block);
        self.table.pop_scope();
    }

    /// A match arm's scope includes both its pattern and body. Bindings the pattern introduces
    /// must be visible in the body.
    fn visit_arm(&mut self, arm: &'ast Arm) {
        self.table.push_scope();
        visit::walk_arm(self, arm);
        self.table.pop_scope();
    }

    /// `While`/`WhileLet`/`For` each get a scope around their pattern (if any) and body, same
    /// as a match arm. A `WhileLet`/`For` pattern's bindings must outlive the block they guard.
    /// Plain `While` has no pattern to bind, so its scope is never written to. Wrapping all
    /// three uniformly keeps the logic consistent.
    fn visit_stmt(&mut self, stmt: &'ast Stmt) {
        match &stmt.kind {
            StmtKind::While { .. } | StmtKind::WhileLet { .. } | StmtKind::For { .. } => {
                self.table.push_scope();
                visit::walk_stmt(self, stmt);
                self.table.pop_scope();
            }
            _ => visit::walk_stmt(self, stmt),
        }
    }

    /// `Variant`'s record payload gets the same field-aware treatment `visit_expr` gives
    /// `Ctor`/`Access`/`Variant`, and for the same reason: a shorthand field (`{ w }`, binding
    /// `w`) has no `Pat` behind it for the default walk's `payload_values` helper to hand back,
    /// so it is silently never bound without this. Returns early like those do, to avoid
    /// re-visiting an explicit-value field a second time through `walk_pat`'s own handling.
    fn visit_pat(&mut self, pat: &'ast Pat) {
        match &pat.kind {
            PatKind::Binding(name) => {
                self.table.insert_local(*name, Local::Variable(pat.id));
            }
            PatKind::Variant { payload, .. } => {
                self.visit_pat_payload(payload);
                return;
            }
            _ => {}
        }
        visit::walk_pat(self, pat);
    }

    /// A closure pushes a local scope for its own parameters. Like a function, it has no
    /// generic or `Self` scope, so it sees whatever its enclosing definition already has on those
    /// stacks. `visit_closure_param` (below) does the binding, since
    /// `walk_closure` is what reaches each parameter.
    fn visit_closure(
        &mut self,
        params: &'ast [ClosureParam],
        ret: Option<&'ast Ty>,
        body: &'ast Expr,
    ) {
        self.table.push_scope();
        visit::walk_closure(self, params, ret, body);
        self.table.pop_scope();
    }

    fn visit_closure_param(&mut self, p: &'ast ClosureParam) {
        self.table.insert_local(p.name, Local::Param(p.id));
        visit::walk_closure_param(self, p);
    }
}
