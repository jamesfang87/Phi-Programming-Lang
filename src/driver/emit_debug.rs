use std::collections::HashMap;

use crate::ast::interner::Interner;
use crate::ast::visit::{self, Visitor as AstVisitor};
use crate::ast::{
    Ast, Expr as AstExpr, Extend as AstExtend, Generic as AstGeneric, Ident as AstIdent,
    Item as AstItem, ItemKind as AstItemKind, Module, NodeId as AstNodeId, Param as AstParam,
    Pat as AstPat, PatKind as AstPatKind, Path as AstPath, SelfParam as AstSelfParam, Symbol,
    Ty as AstTy,
};
use crate::driver::source::{FileOrigin, SrcMap, SrcSpan};
use crate::hir::{DefId, Hir, HirId, Node, OwnerNode};
use crate::mir::{Body, Instance};
use crate::nameres::results::NameResolutions;
use crate::nameres::{Local as NameResLocal, Res as NameResRes, TyDef, Type as NameResType};
use crate::typeck::results::TypeResolutions;
use crate::typeck::ty::{Ty, TyKind};
use crate::typeck::tyctx::TyCtx;

fn is_user_def(hir: &Hir, def_id: DefId) -> bool {
    is_user_span(hir.def(def_id).span())
}

fn def_name(hir: &Hir, def_id: DefId) -> &'static str {
    match hir.def(def_id) {
        OwnerNode::Module(m) => m
            .path
            .segments
            .last()
            .map(|seg| Interner::resolve(seg.text))
            .unwrap_or("<root>"),
        OwnerNode::Function(f) => Interner::resolve(f.name.text),
        OwnerNode::Struct(s) => Interner::resolve(s.name.text),
        OwnerNode::Enum(e) => Interner::resolve(e.name.text),
        OwnerNode::Trait(t) => Interner::resolve(t.name.text),
        OwnerNode::Extend(_) => "<extend>",
        OwnerNode::Closure(_) => "<closure>",
    }
}

fn fmt_def(hir: &Hir, def_id: DefId) -> String {
    let hir_id = def_id.owner_id();
    format!("{} ({hir_id:?})", def_name(hir, def_id))
}

fn node_kind(node: &Node) -> String {
    fn variant_name<T: std::fmt::Debug>(value: &T) -> String {
        let debug = format!("{value:?}");
        let end = debug
            .find(|c: char| !(c.is_alphanumeric() || c == '_'))
            .unwrap_or(debug.len());
        debug[..end].to_string()
    }

    match node {
        Node::Owner(owner) => format!("Owner::{}", variant_name(owner)),
        Node::Stmt(stmt) => format!("Stmt::{}", variant_name(&stmt.kind)),
        Node::Expr(expr) => format!("Expr::{}", variant_name(&expr.kind)),
        Node::Pat(pat) => format!("Pat::{}", variant_name(&pat.kind)),
        Node::Ty(ty) => format!("Ty::{}", variant_name(&ty.kind)),
        other => other.kind_name().to_string(),
    }
}

/// The longest a source snippet in a summary is allowed to be before it's cut off with `...`,
/// so a summary line stays a summary line even when the node it describes spans a whole
/// function body.
const MAX_SNIPPET_LEN: usize = 60;

/// Collapses `text` to a single line (removing newlines and indentation from multi-line spans)
/// and truncates it to [`MAX_SNIPPET_LEN`].
fn snippet(text: &str) -> String {
    let one_line = text.split_whitespace().collect::<Vec<_>>().join(" ");
    if one_line.chars().count() > MAX_SNIPPET_LEN {
        format!(
            "{}...",
            one_line.chars().take(MAX_SNIPPET_LEN).collect::<String>()
        )
    } else {
        one_line
    }
}

fn fmt_node_summary(hir: &Hir, hir_id: HirId) -> String {
    let node = hir.node(hir_id);
    let kind = node_kind(node);
    let span = node.span();

    let Some(file) = SrcMap::file_containing(span.get_begin()) else {
        return kind;
    };
    let (line, col) = file.line_col(span.get_begin());
    let location = format!("{}:{line}:{col}", file.name);

    match SrcMap::text_of(span) {
        Some(text) => format!("{kind} `{}` ({location})", snippet(&text)),
        None => format!("{kind} ({location})"),
    }
}

/// One indent level, in spaces.
const INDENT: &str = "    ";

fn pad(indent: usize) -> String {
    INDENT.repeat(indent)
}

/// Joins already-rendered `items` into a parenthesized/braced block, one per line, indented one
/// level deeper than `indent`. Empty `items` collapses to `open` and `close` stuck together with
/// nothing between them, e.g. `()`.
fn block(open: &str, close: &str, items: &[String], indent: usize) -> String {
    if items.is_empty() {
        return format!("{open}{close}");
    }

    let inner = pad(indent + 1);
    let body: String = items
        .iter()
        .map(|item| format!("{inner}{item},\n"))
        .collect();
    format!("{open}\n{body}{}{close}", pad(indent))
}

fn fmt_ty(hir: &Hir, tcx: &TyCtx, ty: Ty, indent: usize) -> String {
    match tcx.kind(ty) {
        TyKind::Var(var) => format!("{var:?}"),
        TyKind::Primitive(prim) => format!("{prim:?}"),
        TyKind::Adt { def, args } => {
            let name = fmt_def(hir, *def);
            if args.is_empty() {
                return name;
            }
            let args: Vec<_> = args
                .iter()
                .map(|a| fmt_ty(hir, tcx, *a, indent + 1))
                .collect();
            format!("{name}{}", block("<", ">", &args, indent))
        }
        TyKind::Generic(id) => format!("Generic({id:?})"),
        TyKind::SelfTy(def) => format!("SelfTy({})", fmt_def(hir, *def)),
        TyKind::Ref { base, mutability } => {
            let inner = pad(indent + 1);
            format!(
                "Ref {{\n{inner}base: {},\n{inner}mutability: {mutability:?},\n{}}}",
                fmt_ty(hir, tcx, *base, indent + 1),
                pad(indent)
            )
        }
        TyKind::Any(base) => format!("Any({})", fmt_ty(hir, tcx, *base, indent)),
        TyKind::Tuple(elems) => {
            let elems: Vec<_> = elems
                .iter()
                .map(|e| fmt_ty(hir, tcx, *e, indent + 1))
                .collect();
            block("(", ")", &elems, indent)
        }
        TyKind::Array { elem, len } => {
            let inner = pad(indent + 1);
            format!(
                "Array {{\n{inner}elem: {},\n{inner}len: {len:?},\n{}}}",
                fmt_ty(hir, tcx, *elem, indent + 1),
                pad(indent)
            )
        }
        TyKind::Fun { params, ret } => {
            let params: Vec<_> = params
                .iter()
                .map(|p| fmt_ty(hir, tcx, *p, indent + 1))
                .collect();
            let ret = ret
                .map(|r| fmt_ty(hir, tcx, r, indent))
                .unwrap_or_else(|| "()".to_string());
            format!("{} -> {ret}", block("fun(", ")", &params, indent))
        }
        TyKind::Dyn { trait_, args } => {
            let name = format!("dyn {}", fmt_def(hir, *trait_));
            if args.is_empty() {
                return name;
            }
            let args: Vec<_> = args
                .iter()
                .map(|a| fmt_ty(hir, tcx, *a, indent + 1))
                .collect();
            format!("{name}{}", block("<", ">", &args, indent))
        }
        TyKind::Never => "Never".to_string(),
        TyKind::Unit => "Unit".to_string(),
        TyKind::Error => "Error".to_string(),
    }
}

fn is_user_span(span: SrcSpan) -> bool {
    matches!(
        SrcMap::file_containing(span.get_begin()).map(|file| file.origin),
        Some(FileOrigin::User)
    )
}

pub fn print_ast(ast: &Ast) {
    for mod_id in ast.mod_ids() {
        let module = ast.module(mod_id);
        let imports: Vec<_> = module
            .imports
            .iter()
            .filter(|import| is_user_span(import.span))
            .collect();
        let items: Vec<_> = module
            .items
            .iter()
            .filter(|item| is_user_span(item.span))
            .collect();
        if imports.is_empty() && items.is_empty() {
            continue;
        }

        println!("// module {}", fmt_mod_path(module));
        for import in imports {
            println!("{import:#?}");
        }
        for item in items {
            println!("{item:#?}");
        }
    }
}

/// A module's dotted path, or `<root>` for the root module, which has none.
fn fmt_mod_path(module: &Module) -> String {
    if module.path.segments.is_empty() {
        return "<root>".to_string();
    }
    module
        .path
        .segments
        .iter()
        .map(|seg| Interner::resolve(seg.text))
        .collect::<Vec<_>>()
        .join("::")
}

pub fn print_hir(hir: &Hir, exclude_core_in_emit: bool) {
    if !exclude_core_in_emit {
        println!("{hir:#?}");
        return;
    }

    for def_id in hir.def_ids() {
        if !is_user_def(hir, def_id) {
            continue;
        }
        println!("--- {} ---", fmt_def(hir, def_id));
        println!("{:#?}", hir.arena(def_id));
    }
}

pub fn print_typeck(hir: &Hir, tcx: &TyCtx, results: &TypeResolutions, exclude_core_in_emit: bool) {
    let keep = |def_id: DefId| !exclude_core_in_emit || is_user_def(hir, def_id);

    println!("=== TypeCk results ===");
    for (hir_id, ty) in results.iter().filter(|(id, _)| keep(id.owner)) {
        println!(
            "{hir_id:?} :: {} ->\n{}{}",
            fmt_node_summary(hir, hir_id),
            pad(1),
            fmt_ty(hir, tcx, ty, 1)
        );
    }
}

/// Dumps the monomorphized MIR: one `--- {mangled name} ---` section per [`Instance`], each
/// giving its `def_id`/`arg_count`, its locals with their types rendered through [`fmt_ty`]
/// (matching [`print_typeck`]'s own convention, since a bare `Ty` prints only its interned
/// index), and its basic blocks via their derived `Debug`, which is already self-describing for
/// everything but the `Ty`s a `LocalDecl`/`Constant`/`Cast` carries. Sections are sorted by
/// mangled name, for deterministic output across runs.
pub fn print_mir(
    hir: &Hir,
    tcx: &TyCtx,
    instances: &HashMap<Instance, Body>,
    exclude_core_in_emit: bool,
) {
    println!("=== MIR ===");

    let mut sections: Vec<(String, &Instance, &Body)> = instances
        .iter()
        .filter(|(instance, _)| !exclude_core_in_emit || is_user_def(hir, instance.def))
        .map(|(instance, body)| {
            (
                crate::mir::mangle::mangle(hir, tcx, instance),
                instance,
                body,
            )
        })
        .collect();
    sections.sort_by(|(a, ..), (b, ..)| a.cmp(b));

    for (name, instance, body) in sections {
        println!("--- {name} ---");
        println!(
            "def_id: {:?}, any_mode: {:?}, args: {}, arg_count: {}",
            instance.def,
            instance.any_mode,
            instance
                .args
                .iter()
                .map(|&arg| fmt_ty(hir, tcx, arg, 0))
                .collect::<Vec<_>>()
                .join(", "),
            body.arg_count
        );
        for (index, decl) in body.local_decls.iter().enumerate() {
            let name = decl
                .name
                .map(|ident| Interner::resolve(ident.text).to_string())
                .unwrap_or_else(|| "_".to_string());
            println!(
                "{}_{index} ({name}): {}",
                pad(1),
                fmt_ty(hir, tcx, decl.ty, 1)
            );
        }
        for (index, block) in body.basic_blocks.iter().enumerate() {
            println!("{}bb{index}: {block:#?}", pad(1));
        }
    }
}

// ===========================================================================
// Name resolution dump
//
// `crate::nameres::resolve` runs on the `Ast`, before lowering -- see `pipeline.rs`. This dump
// makes its output inspectable directly, without going through the `hir::Path`s it ends up
// attached to.
// ===========================================================================

/// Everything [`fmt_res`] needs to turn a `Res` into readable text without ever
/// printing the `NodeId` it carries: a lookup from that id back to the declaration it names,
/// built by a single traversal of the whole AST.
///
/// `NameResolutions` records only ids -- `Res::Function(NodeId)`,
/// `Res::Local(Local::Variable(NodeId))`, and so on -- so turning one back into a name needs
/// somewhere to look the id up. `SymbolTable` already keeps a `NodeId -> &Item` map for
/// exactly this reason (see its doc comment), but a `Generic`, a `Param`, a `SelfParam`, and a
/// pattern binding are none of them `Item`s, and `nameres_to_string` only receives an
/// `Ast`, not a `SymbolTable`. So this dump builds its own table, covering every kind of node a
/// `Res` can name, in the one walk that also drives [`Self::owners`].
struct Names<'ast> {
    items: HashMap<AstNodeId, &'ast AstItem>,
    generics: HashMap<AstNodeId, &'ast AstGeneric>,
    params: HashMap<AstNodeId, &'ast AstParam>,
    self_params: HashMap<AstNodeId, &'ast AstSelfParam>,
    bindings: HashMap<AstNodeId, AstIdent>,
    /// Every node that can own an entry in `NameResolutions`: a `Generic`'s own id (bounds), an
    /// `extend` item's id (`adt_path`/`trait_path`), a `Ty`'s id, an `Expr`'s id --  see
    /// `resolver.rs`'s calls to `results.record`, which this list mirrors. `entries(owner)`
    /// needs the *owner*, not the `Res`, so collecting every owner here in one walk is what lets
    /// [`nameres_to_string`] find every entry there is without re-walking the AST a
    /// second time just to rediscover them.
    owners: Vec<AstNodeId>,
    /// The `Item` currently being walked, if any. Mirrors `resolver.rs`'s own `current_item`:
    /// `Extend` has no `NodeId` of its own, so [`Self::visit_extend`] reads this to know which
    /// owner its `adt_path`/`trait_path` entries were recorded under.
    current_item: Option<AstNodeId>,
}

impl<'ast> Names<'ast> {
    fn new() -> Self {
        Self {
            items: HashMap::new(),
            generics: HashMap::new(),
            params: HashMap::new(),
            self_params: HashMap::new(),
            bindings: HashMap::new(),
            owners: Vec::new(),
            current_item: None,
        }
    }
}

impl<'ast> AstVisitor<'ast> for Names<'ast> {
    fn visit_item(&mut self, item: &'ast AstItem) {
        self.items.insert(item.id, item);
        self.current_item = Some(item.id);
        visit::walk_item(self, item);
    }

    fn visit_generic(&mut self, g: &'ast AstGeneric) {
        self.generics.insert(g.id, g);
        self.owners.push(g.id);
        visit::walk_generic(self, g);
    }

    fn visit_extend(&mut self, e: &'ast AstExtend) {
        let item_id = self
            .current_item
            .expect("visit_extend is reached only through visit_item, which sets current_item");
        self.owners.push(item_id);
        visit::walk_extend(self, e);
    }

    fn visit_param(&mut self, p: &'ast AstParam) {
        self.params.insert(p.id, p);
        visit::walk_param(self, p);
    }

    fn visit_self_param(&mut self, p: &'ast AstSelfParam) {
        self.self_params.insert(p.id, p);
    }

    fn visit_pat(&mut self, p: &'ast AstPat) {
        if let AstPatKind::Binding(name) = &p.kind {
            self.bindings.insert(p.id, *name);
        }
        visit::walk_pat(self, p);
    }

    fn visit_ty(&mut self, t: &'ast AstTy) {
        self.owners.push(t.id);
        visit::walk_ty(self, t);
    }

    fn visit_expr(&mut self, e: &'ast AstExpr) {
        self.owners.push(e.id);
        visit::walk_expr(self, e);
    }
}

/// A written name's `file:line:col`, or `<unknown>` if `span` doesn't land in any registered
/// file -- which should not happen for anything actually parsed, but keeps this printable
/// rather than panicking if it's ever asked about a synthetic span.
fn res_location(span: SrcSpan) -> String {
    match SrcMap::file_containing(span.get_begin()) {
        Some(file) => {
            let (line, col) = file.line_col(span.get_begin());
            format!("{}:{line}:{col}", file.name)
        }
        None => "<unknown>".to_string(),
    }
}

/// Renders a resolved name as `` Kind `name` (location) `` -- what every arm of
/// [`fmt_res`] reduces to once the `NodeId` it started from has been swapped for the
/// declaration it names.
fn fmt_named(kind: &str, name: Symbol, span: SrcSpan) -> String {
    format!(
        "{kind} `{}` ({})",
        Interner::resolve(name),
        res_location(span)
    )
}

/// Renders a `TyDef`'s target -- a struct, enum, or trait item -- by its declared name, not its
/// `NodeId`. `kind` is one of `"Struct"`, `"Enum"`, `"Trait"`, matching which `TyDef` variant
/// `id` came from.
fn fmt_ty_def(names: &Names, kind: &str, id: AstNodeId) -> String {
    let item =
        names.items.get(&id).copied().unwrap_or_else(|| {
            panic!("a `TyDef::{kind}` names an item the AST walk should collect")
        });
    let name = match &item.kind {
        AstItemKind::Struct(s) => s.name,
        AstItemKind::Enum(e) => e.name,
        AstItemKind::Trait(t) => t.name,
        other => {
            panic!("a `TyDef::{kind}` should name a struct, enum, or trait item, got {other:?}")
        }
    };
    fmt_named(kind, name.text, name.span)
}

/// Renders `res` by what it names, never by the `NodeId` it carries.
///
/// This is the essential half of the no-`NodeId` rule described on
/// [`nameres_to_string`]: every arm below reaches into `names` (or, for
/// `Res::Module`, `ast` directly) to recover a written name and span, and formats *that*.
/// `names` is [`Names`], built by walking the whole AST once; `ast` is only needed here
/// for `Res::Module`, whose target is an `ast::Module`, not an `Item`.
fn fmt_res(names: &Names, ast: &Ast, res: NameResRes) -> String {
    match res {
        NameResRes::Err => "Err".to_string(),
        NameResRes::Module(id) => format!("Module `{}`", fmt_mod_path(ast.module(id))),
        NameResRes::Function(id) => {
            let item = names
                .items
                .get(&id)
                .copied()
                .expect("a `Res::Function` names an item the AST walk should collect");
            let AstItemKind::Function(f) = &item.kind else {
                panic!(
                    "a `Res::Function` should name a function item, got {:?}",
                    item.kind
                );
            };
            fmt_named("Function", f.name.text, f.name.span)
        }
        NameResRes::Local(NameResLocal::Param(id)) => {
            let p = names
                .params
                .get(&id)
                .copied()
                .expect("a `Local::Param` names a parameter the AST walk should collect");
            fmt_named("Param", p.name.text, p.name.span)
        }
        NameResRes::Local(NameResLocal::SelfParam(id)) => {
            let p =
                names.self_params.get(&id).copied().expect(
                    "a `Local::SelfParam` names a self parameter the AST walk should collect",
                );
            format!("SelfParam `self` ({})", res_location(p.span))
        }
        NameResRes::Local(NameResLocal::Variable(id)) => {
            let name = names
                .bindings
                .get(&id)
                .copied()
                .expect("a `Local::Variable` names a binding the AST walk should collect");
            fmt_named("Variable", name.text, name.span)
        }
        NameResRes::Type(NameResType::Prim(prim)) => format!("{prim:?}"),
        NameResRes::Type(NameResType::Generic(id)) => {
            let g = names
                .generics
                .get(&id)
                .copied()
                .expect("a `Type::Generic` names a generic the AST walk should collect");
            fmt_named("Generic", g.name.text, g.name.span)
        }
        NameResRes::Type(NameResType::Def(TyDef::Struct(id))) => fmt_ty_def(names, "Struct", id),
        NameResRes::Type(NameResType::Def(TyDef::Enum(id))) => fmt_ty_def(names, "Enum", id),
        NameResRes::Type(NameResType::Def(TyDef::Trait(id))) => fmt_ty_def(names, "Trait", id),
    }
}

/// A span-ordered, `NodeId`-free rendering of name resolution's output.
///
/// **Span-ordered.** [`crate::ast::NodeId`] comes from a single global atomic counter, and its
/// assignment order is deterministic only because parsing currently runs sequentially, file by
/// file. The counter is global specifically to enable parallel parsing later. Once it does, the
/// id order for nodes across files becomes unpredictable. A dump ordered by `NodeId` (or by
/// hash-map iteration order) would vary between runs for no reason a diff could explain. Sorting
/// by source span instead costs nothing today (while parsing is sequential, span order and id
/// order agree) and pins the dump to the one thing about a program that remains stable when
/// parsing parallelizes: where its text sits in its own file.
///
/// **Never prints a `NodeId`.** Same reason: an id that isn't stable across parallel parsing
/// should not appear in a file meant to be diffed for meaning rather than mechanism. Every `Res`
/// is rendered by what it names instead of by its id -- the declaration's own written name and
/// span, recovered through [`Names`], the lookup table this function builds by walking
/// `ast` once. See [`fmt_res`] for how each `Res` variant does that.
///
/// Structured as a pure string builder -- with [`print_nameres`] as the thin `println!`
/// wrapper around it -- so a test can assert on the string directly instead of capturing stdout.
pub fn nameres_to_string(ast: &Ast, results: &NameResolutions) -> String {
    let mut names = Names::new();
    names.visit_module(ast.module(ast.root_id()), ast);

    let mut entries: Vec<(SrcSpan, &AstPath, NameResRes)> = names
        .owners
        .iter()
        .flat_map(|&owner| {
            results
                .entries(owner)
                .iter()
                .map(|(path, res)| (path.span, path, *res))
        })
        .collect();
    // Rule 1: sorted by source span -- never by `NodeId`, never by hash-map iteration order.
    // See the doc comment above.
    entries.sort_by_key(|(span, _, _)| span.get_begin());

    let mut out = String::new();
    out.push_str("=== NameResolution results ===\n");
    for (span, path, res) in entries {
        let path_text = path
            .segments
            .iter()
            .map(|seg| Interner::resolve(seg.text))
            .collect::<Vec<_>>()
            .join("::");
        out.push_str(&format!(
            "{path_text} ({}) ->\n{}{}\n",
            res_location(span),
            pad(1),
            // Rule 2: rendered by what `res` names, never by its `NodeId`. See the doc comment
            // above and `fmt_res`.
            fmt_res(&names, ast, res)
        ));
    }
    out
}

/// Thin `println!` wrapper around [`nameres_to_string`]; see its doc comment for what
/// the dump guarantees (span order, no `NodeId`) and why.
pub fn print_nameres(ast: &Ast, results: &NameResolutions) {
    println!("{}", nameres_to_string(ast, results));
}
