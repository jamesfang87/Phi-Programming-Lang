//! Temporary human-readable dumps of the compiler's intermediate results, for debugging the
//! passes that produce them. Hooked up to `phi build --ast`, `--hir`, and the catch-all
//! `--debug`, which runs the whole pipeline (through type checking) and prints all of it.
//!
//! A raw [`Symbol`] or [`DefId`] is just an integer, which makes the derived `Debug` output of
//! [`ValueRes`] and [`TyKind`] tedious to read by hand. The HIR, name resolution, and type checking
//! dumps here resolve a `Symbol` back to its interned string and a `DefId` back to the name and
//! [`HirId`] of the definition it addresses, instead of leaving either as a bare number.
//!
//! `--no-core` filters all of that down to definitions from the user's own files, leaving out
//! the core library that's linked into every build.

use crate::ast::interner::Interner;
use crate::ast::{Ast, AstModule, Symbol};
use crate::driver::source::{FileOrigin, SrcMap, SrcSpan};
use crate::hir::{DefId, Hir, HirId, Node, OwnerNode};
use crate::nameres::results::{NameResolutions, SelfTyRes, TypeRes, ValueRes};
use crate::typeck::results::TypeResolutions;
use crate::typeck::ty::{Ty, TyKind};
use crate::typeck::tyctx::TyCtx;

/// Whether `def_id` was declared in a file the user wrote, as opposed to the core library that's
/// linked into every build.
fn is_user_def(hir: &Hir, def_id: DefId) -> bool {
    is_user_span(hir.def(def_id).span())
}

fn fmt_symbol(sym: Symbol) -> String {
    format!("{} (symbol: {})", Interner::resolve(sym), sym.id())
}

/// The name a `DefId` was declared with. `Extend` and `Closure` never have one, so they get a
/// placeholder instead.
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

/// The kind of HIR node, as `Category::Variant` (e.g. `Expr::Call`, `Pat::Binding`).
///
/// The category comes from [`Node::kind_name`], so a new node kind needs no edit here. Only the
/// five kinds that carry an inner `*Kind` enum worth naming are listed, and their inner variant
/// is read off the derived `Debug` rather than matched by hand, so those stay in sync
/// automatically as variants are added or renamed.
fn node_kind(node: &Node) -> String {
    fn variant_name<T: std::fmt::Debug>(value: &T) -> String {
        // A derived `Debug` on an enum always starts with the bare variant name, whether it's a
        // unit, tuple, or struct variant -- so the leading identifier is exactly what we want,
        // and nothing here needs to know the enum's actual variants.
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

/// Collapses `text` down to a single line (folding away the newlines and indentation a
/// multi-line span would otherwise carry) and truncates it to [`MAX_SNIPPET_LEN`].
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

/// A one-line "what is this" summary for a [`HirId`]: its node kind, the source text its span
/// covers, and where that span is. Falls back to just the kind if the span doesn't land inside
/// any file the compiler has read, which shouldn't happen for a real HIR node but isn't worth a
/// panic in a debug dump.
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

/// One indent level, in spaces. Everything below builds its own multi-line output by hand
/// rather than going through `{:#?}`, since the whole point is substituting a resolved name in
/// where a derived `Debug` would print a bare `Symbol` or `DefId` -- so nesting depth has to be
/// tracked manually too.
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

fn fmt_value_res(hir: &Hir, res: ValueRes) -> String {
    match res {
        ValueRes::Local(id) => format!("Local({id:?})"),
        ValueRes::SelfVal(id) => format!("SelfVal({id:?})"),
        ValueRes::Variant(id) => format!("Variant({id:?})"),
        ValueRes::Def(id) => format!("Def({})", fmt_def(hir, id)),
        ValueRes::Err => "Err".to_string(),
    }
}

fn fmt_type_res(hir: &Hir, res: TypeRes) -> String {
    match res {
        TypeRes::PrimTy(prim) => format!("PrimTy({prim:?})"),
        TypeRes::Generic(id) => format!("Generic({id:?})"),
        TypeRes::Def(id) => format!("Def({})", fmt_def(hir, id)),
        TypeRes::Err => "Err".to_string(),
    }
}

fn fmt_self_ty_res(hir: &Hir, res: SelfTyRes, indent: usize) -> String {
    match res {
        SelfTyRes::Ty { adt, trait_ } => {
            let trait_ = match trait_ {
                Some(t) => fmt_def(hir, t),
                None => "None".to_string(),
            };
            let inner = pad(indent + 1);
            format!(
                "SelfTy {{\n{inner}adt: {},\n{inner}trait_: {},\n{}}}",
                fmt_def(hir, adt),
                trait_,
                pad(indent)
            )
        }
        SelfTyRes::Err => "Err".to_string(),
    }
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

/// Whether `span` sits in a file the user wrote, as opposed to the core library that's linked
/// into every build.
fn is_user_span(span: SrcSpan) -> bool {
    matches!(
        SrcMap::file_containing(span.get_begin()).map(|file| file.origin),
        Some(FileOrigin::User)
    )
}

/// Pretty-prints the parsed AST, module by module. The core library is always left out, since it
/// isn't part of the program the user asked to see. This is the hook `phi build --ast` (and
/// `--debug`) uses, and what the golden tests under `tests/` snapshot.
///
/// Modules, not files, are the unit here: the parser hands back an [`Ast`], which has already
/// merged every file declaring into the same module. A module the user contributed nothing to is
/// skipped entirely, so a build's own modules aren't buried under the core library's.
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
fn fmt_mod_path(module: &AstModule) -> String {
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

/// Pretty-prints the lowered HIR for the whole unit. This is the hook `phi build --hir` uses.
///
/// With `exclude_core_in_emit` unset, this is a single `{hir:#?}` dump of everything, core library
/// included. With it set, the core library is left out by walking every definition
/// individually and skipping the ones it owns, since [`Hir`]'s derived `Debug` has no way to
/// filter partway through.
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

/// Prints every entry of `results`, resolving each [`Symbol`] to its interned string and each
/// [`DefId`] to its name and [`HirId`]. This is part of the `phi build --debug` dump.
///
/// With `exclude_core_in_emit` set, entries belonging to the core library are left out.
pub fn print_nameres(hir: &Hir, results: &NameResolutions, exclude_core_in_emit: bool) {
    let keep = |def_id: DefId| !exclude_core_in_emit || is_user_def(hir, def_id);

    println!("=== NameResolution results: values ===");
    for (hir_id, res) in results.iter_values().filter(|(id, _)| keep(id.owner)) {
        println!(
            "{hir_id:?} :: {} ->\n{}{}",
            fmt_node_summary(hir, hir_id),
            pad(1),
            fmt_value_res(hir, res)
        );
    }

    println!("=== NameResolution results: types ===");
    for (hir_id, res) in results.iter_types().filter(|(id, _)| keep(id.owner)) {
        println!(
            "{hir_id:?} :: {} ->\n{}{}",
            fmt_node_summary(hir, hir_id),
            pad(1),
            fmt_type_res(hir, res)
        );
    }

    println!("--- Self types ---");
    for (def_id, res) in results.iter_self_tys().filter(|(id, _)| keep(*id)) {
        let hir_id = def_id.owner_id();
        println!(
            "{} :: {} ->\n{}{}",
            fmt_def(hir, def_id),
            fmt_node_summary(hir, hir_id),
            pad(1),
            fmt_self_ty_res(hir, res, 1)
        );
    }

    println!("--- Generics ---");
    for (def_id, params) in results.iter_generics().filter(|(id, _)| keep(*id)) {
        let hir_id = def_id.owner_id();
        for (&name, &res) in params {
            println!(
                "{} :: {} :: {} ->\n{}{}",
                fmt_def(hir, def_id),
                fmt_symbol(name),
                fmt_node_summary(hir, hir_id),
                pad(1),
                fmt_type_res(hir, res)
            );
        }
    }
}

/// Prints every entry of `results`, resolving each [`Ty`] handle to its structure and each
/// [`DefId`] inside it to its name and [`HirId`]. This is part of the `phi build --debug` dump.
///
/// With `exclude_core_in_emit` set, entries belonging to the core library are left out.
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
