//! The AST walk that populates `NameResolutions`.
//!
//! The traversal is `ast::visit`'s, so that "what are this node's children" is answered in one
//! place for every AST pass rather than re-derived here. Only the nodes that need something
//! *around* or *instead of* the default walk are overridden: a block opens a scope, a path
//! records what it named, a binding pattern binds. Everything else -- expressions with no path
//! of their own, a struct's fields, a variant's payload -- falls through to `ast::visit`'s
//! defaults untouched.

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

/// Drives [`SymbolTable`]'s three scope stacks across the whole AST, recording an entry in
/// [`NameResolutions`] for every path written in the program.
struct Resolver<'ast> {
    table: SymbolTable<'ast>,
    results: NameResolutions,
    /// The module the node currently being walked is written in. `SymbolTable`'s lookups take
    /// this as `from`, which is why no `module_of` walk is needed -- unlike on the HIR side,
    /// the traversal already tracks it: [`resolve`] updates it once per module, before that
    /// module's items are visited.
    module: NodeId,
    /// The `Item` currently being walked, if any. `Struct`, `Enum`, `Trait`, and `Extend` have
    /// no `NodeId` of their own -- they sit inside `Item`, which does (`src/ast.rs:85`) -- so
    /// [`Visitor::visit_struct`]/`visit_enum`/`visit_trait`/`visit_extend` read this to know
    /// which node their own path entries and `Self` scope belong to. [`Visitor::visit_item`]
    /// sets it before dispatching to whichever of those the item is; nothing nests one `Item`
    /// inside another, so there is no stack to maintain here, only a value to overwrite.
    current_item: Option<NodeId>,
}

/// The entry point the debug dump (`--surface-nameres`, see `crate::driver::emit_debug`) and the
/// pipeline (`crate::driver::pipeline`) both call: walks every module of `ast`, driving
/// `SymbolTable`'s scope stacks as it goes, and returns the completed `NameResolutions`.
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
    // them is a later pass -- see `langitems::collect_ast`.
    let lang_items = crate::langitems::collect_ast(&r.table, ast.root_id());
    r.results.record_lang_items(lang_items);
    r.results
}

impl<'ast> Resolver<'ast> {
    /// Visits every item `module_id` declares directly.
    ///
    /// Not a `Visitor::visit_module` override: `ast.mod_ids()` (see [`resolve`]) already yields
    /// every module in the tree flat, so nothing here needs to recurse into `Module::children`
    /// the way `ast::visit::walk_module` does -- doing so as well would visit every module's
    /// items twice.
    fn resolve_module(&mut self, ast: &'ast Ast, module_id: NodeId) {
        for item in &ast.module(module_id).items {
            self.visit_item(item);
        }
    }

    /// Opens a generic scope binding every one of `generics`, then resolves each one's bounds
    /// against it -- so a bound can see every sibling generic the same `<...>` clause declares,
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

    /// Same as [`Self::push_generics`], for the `Option<Vec<Generic>>` shape `Struct`/`Enum`/
    /// `Trait`/`Extend` declare their own generics with (`None` when there is no `<...>` at
    /// all, as opposed to `Function::generics`, which is always a `Vec`, empty or not).
    fn push_generics_opt(&mut self, generics: &'ast Option<Vec<Generic>>) {
        self.push_generics(generics.as_deref().unwrap_or(&[]));
    }

    /// Records each of `g`'s bounds as an entry on `g` itself, in source order.
    ///
    /// Skips (and reports as a duplicate bound) a bound that repeats one already recorded on `g`
    /// -- `T: Show + Show` -- rather than recording it a second time. `NameResolutions::record`'s
    /// no-two-equal-paths invariant is only a `debug_assert!`, which compiles out in release, so
    /// this check is what actually enforces it for the one case under this resolver's control.
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
    /// Not delegated to `ast::visit::walk_expr`'s own handling of these three shapes: that
    /// default reaches every field's value through [`crate::ast::visit::Visitor::visit_expr`]
    /// (or, for `Payload`, through a helper that only ever hands back existing values), which is
    /// exactly right for a field with one, but has nothing to call for a shorthand field, since
    /// there is no `Expr` behind it to visit. A shorthand field's implicit value is instead
    /// resolved as a value-position lookup of its own name, keyed on the field's own `NodeId`
    /// (`PayloadField::id`) rather than an `Expr`'s -- no `Expr` exists for it, but every field,
    /// shorthand or not, already carries a `NodeId` of its own, so nothing new needs inventing.
    /// This mirrors HIR lowering's own desugaring of the same shorthand into `{ l: l }`
    /// (`src/hir/lower/expr.rs`), just recorded here instead of synthesized there.
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

    /// Same as [`Self::visit_record_fields`], for a record *pattern* payload's shorthand fields:
    /// `{ l }` binds `l`, exactly as `PatKind::Binding` would, but there is no `Pat` behind it to
    /// bind through -- so the field's own `NodeId` stands in as the binding site, the same way
    /// [`Self::visit_record_fields`] uses it as the lookup site on the expression side.
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

    /// An expression [`Payload`] -- `.circle(1.24)`'s single value, or `.square { l }`'s record
    /// fields -- visited the same way `ast::visit::walk_expr`'s `Variant` arm would, except a
    /// record payload's fields go through [`Self::visit_record_fields`] instead of the default's
    /// value-only walk, so a shorthand field still gets resolved.
    fn visit_expr_payload(&mut self, payload: &'ast Payload<Expr>) {
        match payload {
            Payload::None => {}
            Payload::Single(value) => self.visit_expr(value),
            Payload::Record(fields) => self.visit_record_fields(fields),
        }
    }

    /// Same as [`Self::visit_expr_payload`], for a pattern's payload.
    fn visit_pat_payload(&mut self, payload: &'ast Payload<Pat>) {
        match payload {
            Payload::None => {}
            Payload::Single(value) => self.visit_pat(value),
            Payload::Record(fields) => self.visit_record_pat_fields(fields),
        }
    }
}

/// Builds a single-segment [`Path`] naming `ident`, for looking up a name that has no `Path` of
/// its own in the source -- a record payload shorthand field's implicit value, in particular.
fn single_segment_path(ident: Ident) -> Path {
    Path {
        segments: vec![ident],
        span: ident.span,
    }
}

/// An `Ident` naming `self`, built to bind the parameter as a local -- `self` has no `Ident` of
/// its own in `ast::SelfParam`, only a `SelfMode`, since the parser never needs to spell its
/// name out.
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

    /// A function pushes its own generic scope, but **not** a `Self` scope: `Self` inside a
    /// method or a closure nested in one is the enclosing struct/enum/trait/`extend`'s, which is
    /// already on the stack by the time this runs (see `SymbolTable::current_self`).
    ///
    /// Parameter and return types are resolved *before* the function's own local scope opens --
    /// they can see the generics just pushed, but not `self` or the parameters themselves, which
    /// are bound only once every type in the signature has already been read. `self` and each
    /// parameter are inserted directly rather than through `visit_self_param`/`visit_param`
    /// (both already ran, above, to resolve types) so that binding doesn't re-walk a type a
    /// second time.
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
    /// Three outcomes for `adt_path`, each pushing something different:
    /// - It resolved to a `TyDef` (`extend Foo with Show`): push that `Self`, ordinarily.
    /// - It resolved to something else, e.g. a primitive (`extend i32 with Show`): push nothing.
    ///   `adt_path` itself is fine -- no diagnostic was raised for it -- so a `Self` written
    ///   inside the block should report "not available" same as it would with no enclosing
    ///   `extend` at all, exactly once.
    /// - It failed to resolve at all (`extend Nope with Show`): push a suppressed scope
    ///   ([`SymbolTable::push_self_unresolved`]). `resolve_type_path` already reported why
    ///   `adt_path` failed; a `Self` written inside the block would otherwise report a second,
    ///   redundant diagnostic for the same root cause.
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

    /// `Ctor`, `Access`, and `Variant` are each handled fully here rather than falling through to
    /// `ast::visit::walk_expr`'s default, and each returns without reaching the trailing
    /// `walk_expr` call at the bottom -- unlike every other arm below, which layers extra
    /// recording on top of the default and still needs it for the rest of the node's children.
    /// The reason is `PayloadField`: a record payload's fields need their own field-aware walk
    /// (see [`Self::visit_record_fields`]) to catch a shorthand field the default's
    /// `payload_values` helper silently drops (it only ever returns fields that already have a
    /// value). Falling through to `walk_expr` afterward would re-visit every field that *does*
    /// have a value a second time, double-recording it.
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
                        // A struct literal names its type, so its one path is resolved in *type*
                        // position, exactly like a `TyKind::Path` -- even though it is written on
                        // an expression, `Foo` here names the struct, not a value called `Foo`.
                        let res = self.table.resolve_type_path(self.module, path);
                        self.results.record(expr.id, path.clone(), res);
                    }
                    None => {
                        // The elided form, `.{ a: 1 }`, takes its type from context -- there is
                        // no path here at all, so deliberately nothing is recorded.
                    }
                }
                self.visit_record_fields(payload);
                return;
            }
            ExprKind::Access { base, args, .. } => {
                // `member`'s reading as a field, a method, or a payload-carrying variant is
                // deliberately not resolved here -- see the `AccessArgs` doc comment: the
                // grammar can't tell the three apart, and "later analysis" (typeck, once
                // `base`'s type is known) is what does.
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
                // `variant`'s own name is deliberately never resolved here -- see the design
                // spec: a bare `.variant` names no enum of its own until typeck knows the
                // expected type, and scanning every enum in scope for a matching name is exactly
                // the ambiguity the leading `.` exists to avoid.
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

    /// A match arm's scope covers its pattern *and* its body together -- `walk_arm` visits both
    /// -- since a binding the pattern introduces has to be visible in the body it guards.
    fn visit_arm(&mut self, arm: &'ast Arm) {
        self.table.push_scope();
        visit::walk_arm(self, arm);
        self.table.pop_scope();
    }

    /// `While`/`WhileLet`/`For` each get a scope around their pattern (if any) and body, same
    /// shape as a match arm -- a `WhileLet`/`For` pattern's bindings must outlive the block they
    /// guard. Plain `While` has no pattern to bind, so the scope it opens is simply never
    /// written into; wrapping it anyway keeps the three statement kinds handled uniformly.
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

    /// A closure pushes a local scope for its own parameters, but -- like a function -- no
    /// generic or `Self` scope of its own, so it sees whatever its enclosing definition already
    /// has on those stacks. `visit_closure_param` (below) does the actual binding, since
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
