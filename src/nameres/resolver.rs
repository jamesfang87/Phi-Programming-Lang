use std::collections::HashMap;

use crate::ast::interner::Interner;
use crate::ast::visit::{self, Visitor};
use crate::ast::{
    AccessArgs, Arm, Ast, Block, ClosureParam, Enum, Expr, ExprKind, Extend, Function, Generic,
    Ident, Item, NodeId, Pat, PatKind, Path, Payload, PayloadField, Stmt, StmtKind, Struct, Trait,
    Ty, TyKind,
};
use crate::driver::source::SrcSpan;
use crate::nameres::diagnostics::{
    report_duplicate_bound, report_not_found, report_self_extend, report_self_unavailable,
};
use crate::nameres::res::{Local, Res, TyDef, Type};
use crate::nameres::results::NameResolutions;
use crate::nameres::symbol_table::SymbolTable;

pub(super) struct Resolver<'ast> {
    pub(super) table: SymbolTable<'ast>,
    results: NameResolutions,
    /// The module the node currently inside is written in. This is used
    /// as `from` in lookups SymbolTable
    current_module: NodeId,
    /// The `Item` currently being considered, if any.
    current_item: Option<NodeId>,
}

pub fn resolve(ast: &Ast) -> NameResolutions {
    let mut r = Resolver::new(SymbolTable::new(ast), ast.root_id());

    for mod_id in ast.mod_ids() {
        r.current_module = mod_id;
        r.resolve_module(ast, mod_id);
    }

    let lang_items = crate::langitems::collect_ast(&r.table, ast.root_id());
    r.results.record_lang_items(lang_items);
    r.results
}

impl<'ast> Resolver<'ast> {
    pub(super) fn new(table: SymbolTable<'ast>, current_module: NodeId) -> Self {
        Resolver {
            table,
            results: NameResolutions::new(),
            current_module,
            current_item: None,
        }
    }

    /// This should be used instead of a lookup_type_path due to the case of
    /// `Self`, which can have multiple reasons of failing
    pub(super) fn resolve_type_path(&self, path: &Path) -> Res {
        let last = *path
            .segments
            .last()
            .expect("a path always has at least one segment");

        if path.segments.len() == 1 && last.text == Interner::intern("Self") {
            return match self.table.current_self_entry() {
                Some(Some(def)) => Res::Type(Type::Def(def)),
                Some(None) => Res::Err,
                None => {
                    report_self_unavailable(last.span);
                    Res::Err
                }
            };
        }

        match self.table.lookup_type_path(self.current_module, path) {
            Some(ty) => Res::Type(ty),
            None => {
                report_not_found(last);
                Res::Err
            }
        }
    }

    fn resolve_module(&mut self, ast: &'ast Ast, module_id: NodeId) {
        for item in &ast.module(module_id).items {
            self.visit_item(item);
        }
    }

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

    fn push_generics_opt(&mut self, generics: &'ast Option<Vec<Generic>>) {
        self.push_generics(generics.as_deref().unwrap_or(&[]));
    }

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
            let res = self.resolve_type_path(bound);
            self.results.record(g.id, bound.clone(), res);
        }
    }

    fn visit_record_fields(&mut self, fields: &'ast [PayloadField<Expr>]) {
        for field in fields {
            match &field.value {
                Some(value) => self.visit_expr(value),
                None => {
                    let path = single_segment_path(field.name);
                    let res = self
                        .table
                        .lookup_value_path(self.current_module, &path)
                        .unwrap_or_else(|| {
                            report_not_found(field.name);
                            Res::Err
                        });
                    self.results.record(field.id, path, res);
                }
            }
        }
    }

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

    fn visit_expr_payload(&mut self, payload: &'ast Payload<Expr>) {
        match payload {
            Payload::None => {}
            Payload::Single(value) => self.visit_expr(value),
            Payload::Record(fields) => self.visit_record_fields(fields),
        }
    }

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
    fn visit_item(&mut self, item: &'ast Item) {
        self.current_item = Some(item.id);
        visit::walk_item(self, item);
    }

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

    fn visit_extend(&mut self, e: &'ast Extend) {
        let item_id = self
            .current_item
            .expect("visit_extend is reached only through visit_item, which sets current_item");

        self.push_generics_opt(&e.extend_generics);

        let adt_res = self.resolve_type_path(&e.adt_path);
        self.results.record(item_id, e.adt_path.clone(), adt_res);

        if let Some(trait_path) = &e.trait_path {
            if *trait_path == e.adt_path {
                report_self_extend(
                    *trait_path
                        .segments
                        .last()
                        .expect("a path always has at least one segment"),
                );
            } else {
                let trait_res = self.resolve_type_path(trait_path);
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

    //------------------------------------------------------------------------

    fn visit_ty(&mut self, ty: &'ast Ty) {
        match &ty.kind {
            TyKind::Path { path, .. } => {
                let res = self.resolve_type_path(path);
                self.results.record(ty.id, path.clone(), res);
            }
            TyKind::Dyn { path, .. } => {
                let res = self.table.lookup_dyn_path(self.current_module, path);
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

    // ------------------------------------------------------------------------

    fn visit_expr(&mut self, expr: &'ast Expr) {
        match &expr.kind {
            ExprKind::Path(path) => {
                let res = self
                    .table
                    .lookup_value_path(self.current_module, path)
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
                        let res = self.resolve_type_path(path);
                        self.results.record(expr.id, path.clone(), res);
                    }
                    None => {
                        // The elided form, `.{ a: 1 }`, takes its type from context.
                    }
                }
                self.visit_record_fields(payload);
                return;
            }
            ExprKind::Access { base, args, .. } => {
                // We defer to after typeck
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
                // We defer to after typeck
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

    fn visit_arm(&mut self, arm: &'ast Arm) {
        self.table.push_scope();
        visit::walk_arm(self, arm);
        self.table.pop_scope();
    }

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
