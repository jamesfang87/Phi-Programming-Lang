use std::collections::HashMap;

use crate::ast::interner::Interner;
use crate::ast::{
    Ast, Expr, ExprKind, Function, Ident, Item, ItemKind, NodeId, ParsedSrcFile, Path, Payload,
    PayloadField, StmtKind, Symbol,
};
use crate::diagnostics::{DiagCtx, Diagnostic};
use crate::driver::emit_debug;
use crate::driver::source::{FileOrigin, SrcCollector, SrcMap, SrcSpan};
use crate::lexer::Lexer;
use crate::nameres::res::PrimTy;
use crate::nameres::res::{Local, Res, TyDef, Type};
use crate::nameres::resolve;
use crate::nameres::resolver::Resolver;
use crate::nameres::results::NameResolutions;
use crate::nameres::symbol_table::SymbolTable;
use crate::parser::Parser;

/// Builds an `Ident` naming `text`, with a throwaway span (for tests exercising scope
/// stacks, where the span is never inspected).
fn ident(text: &str) -> Ident {
    Ident {
        text: Interner::intern(text),
        span: SrcSpan::new(0, 1),
    }
}

// -----------------------------------------------------------------
// Driving the parser, for `SymbolTable::collect` tests
// -----------------------------------------------------------------

/// Lexes and parses `src` into a [`ParsedSrcFile`], asserting no diagnostics were raised.
/// `DiagCtx` and `Interner` are *not* cleared here -- callers building an `Ast` out of several
/// files need each one parsed against the same interner and source map.
fn parse_one(src: &str) -> ParsedSrcFile {
    let chars: Vec<char> = src.chars().collect();
    let offset = SrcMap::add_file("<test>".to_string(), chars.clone(), FileOrigin::User);
    let tokens = Lexer::new(&chars, offset).tokenize();
    Parser::new().parse(&tokens, offset)
}

/// Lexes, parses, and assembles `src` as a single-file `Ast`, asserting no diagnostics were
/// raised along the way.
fn ast_from(src: &str) -> Ast {
    ast_from_files(&[src])
}

/// Lexes, parses, and assembles `sources` into one `Ast` (built from multiple files, the way a
/// real build combines them). Asserts no diagnostics were raised.
fn ast_from_files(sources: &[&str]) -> Ast {
    DiagCtx::clear();
    Interner::clear();
    let files: Vec<ParsedSrcFile> = sources.iter().map(|src| parse_one(src)).collect();
    let diagnostics = DiagCtx::diagnostics();
    assert!(
        diagnostics.is_empty(),
        "unexpected diagnostics for {sources:?}: {diagnostics:?}"
    );
    Ast::new(files)
}

/// Walks `segments` from `ast`'s root module through `table`'s module namespace, resolving a
/// canonical path to its module the way tests need to but nothing in `SymbolTable` itself does.
fn module_by_path(ast: &Ast, table: &SymbolTable, segments: &[Symbol]) -> Option<NodeId> {
    segments.iter().try_fold(ast.root_id(), |current, &seg| {
        table.lookup_mod(current, seg)
    })
}

/// Builds `src`'s `Ast` and runs [`SymbolTable::collect`] over it, returning the table alongside
/// every diagnostic the collect walk itself raised (parsing is asserted clean by [`ast_from`],
/// so nothing from that stage leaks in).
fn collect_with_diags(src: &str) -> (SymbolTable<'_>, Vec<Diagnostic>) {
    fn inner(ast: &Ast) -> (SymbolTable<'_>, Vec<Diagnostic>) {
        DiagCtx::clear();
        let table = SymbolTable::collect(ast);
        (table, DiagCtx::diagnostics())
    }
    // `ast_from` is not inlined because the returned `Ast` must outlive the `SymbolTable<'_>`
    // that borrows it. A local in `collect_with_diags`'s stack frame can't. So this leaks it,
    // which is fine for a test helper.
    let ast: &'static Ast = Box::leak(Box::new(ast_from(src)));
    inner(ast)
}

// -----------------------------------------------------------------
// Driving the parser, for `SymbolTable::new` tests
// -----------------------------------------------------------------

/// Builds `ast`'s `SymbolTable` via [`SymbolTable::new`], returning it alongside every
/// diagnostic construction raised.
fn new_with_diags(ast: &Ast) -> (SymbolTable<'_>, Vec<Diagnostic>) {
    DiagCtx::clear();
    let table = SymbolTable::new(ast);
    (table, DiagCtx::diagnostics())
}

/// Builds a `SymbolTable` via [`SymbolTable::new`], but constructs the `Ast` from `sources`
/// first (see [`ast_from_files`]). Leaks the `Ast` for the same reason as [`collect_with_diags`].
fn new_with_diags_from(sources: &[&str]) -> (SymbolTable<'static>, Vec<Diagnostic>) {
    let ast: &'static Ast = Box::leak(Box::new(ast_from_files(sources)));
    new_with_diags(ast)
}

/// Builds an `Ast` containing the real core library (the way a full build does; see
/// [`SrcCollector::collect_core`]) so a [`SymbolTable`] built over it has `core::prelude` to find.
///
/// Only the files this call itself registers are lexed and parsed, not the whole process-wide
/// [`SrcMap`]. Other tests may register files before or concurrently. `SrcMap` sits behind a
/// single process-wide lock (unlike thread-local `Interner` and `DiagCtx`), so a length
/// snapshot would be racy under the default multi-threaded test runner. `collect_core`
/// sidesteps this by returning exactly the [`SrcFile`]s it registered, identified by the files
/// themselves (not a before/after count; see its doc comment). Re-parsing files beyond those
/// five would raise diagnostics (duplicate declarations, mostly) that belong to other tests.
fn ast_with_core() -> Ast {
    DiagCtx::clear();
    Interner::clear();
    let core_files = SrcCollector::collect_core();
    let files: Vec<ParsedSrcFile> = core_files
        .iter()
        .map(|file| {
            let tokens = Lexer::new(&file.content, file.global_offset).tokenize();
            Parser::new().parse(&tokens, file.global_offset)
        })
        .collect();
    let diagnostics = DiagCtx::diagnostics();
    assert!(
        diagnostics.is_empty(),
        "unexpected diagnostics loading the core library: {diagnostics:?}"
    );
    Ast::new(files)
}

/// Runs `f` against a freshly cleared `DiagCtx`, returning its result alongside every
/// diagnostic `f` raised.
///
/// Mirrors [`new_with_diags`]'s clear-then-collect pattern, but for a single call rather than
/// a whole `SymbolTable::new`, so a lookup entry point's own diagnostics can be checked in
/// isolation.
fn with_diags<T>(f: impl FnOnce() -> T) -> (T, Vec<Diagnostic>) {
    DiagCtx::clear();
    let result = f();
    (result, DiagCtx::diagnostics())
}

/// Filters out "missing lang item" diagnostics. `resolve` (unlike `SymbolTable::new` alone)
/// always runs `langitems::ast::collect`, which reports one for every lang item the core library
/// would declare. None of the fixtures below build a unit with a core library. That noise is
/// expected and unrelated to what these tests check.
fn non_lang_item_diags(diags: &[Diagnostic]) -> Vec<&Diagnostic> {
    diags
        .iter()
        .filter(|d| !d.message.contains("missing lang item"))
        .collect()
}

fn path(segments: &[&str]) -> Path {
    let span = SrcSpan::new(0, 1);
    Path {
        segments: segments
            .iter()
            .map(|s| Ident {
                text: Interner::intern(s),
                span,
            })
            .collect(),
        span,
    }
}

#[test]
fn get_returns_the_entry_matching_the_path() {
    let mut r = NameResolutions::new();
    let owner = NodeId::next();
    let target = NodeId::next();
    r.record(owner, path(&["Vec"]), Res::Err);
    r.record(owner, path(&["Show"]), Res::Local(Local::Param(target)));

    assert_eq!(
        r.get(owner, &path(&["Show"])),
        Some(Res::Local(Local::Param(target)))
    );
    assert_eq!(r.get(owner, &path(&["Vec"])), Some(Res::Err));
}

#[test]
fn get_is_none_for_a_path_the_node_does_not_own() {
    let mut r = NameResolutions::new();
    let owner = NodeId::next();
    r.record(owner, path(&["Vec"]), Res::Err);
    assert_eq!(r.get(owner, &path(&["Show"])), None);
}

#[test]
fn get_is_none_for_a_node_with_no_entries() {
    let r = NameResolutions::new();
    assert_eq!(r.get(NodeId::next(), &path(&["Vec"])), None);
}

#[test]
fn entries_are_returned_in_the_order_recorded() {
    let mut r = NameResolutions::new();
    let owner = NodeId::next();
    r.record(owner, path(&["a"]), Res::Err);
    r.record(owner, path(&["b"]), Res::Err);
    let got: Vec<_> = r.entries(owner).iter().map(|(p, _)| p.clone()).collect();
    assert_eq!(got, vec![path(&["a"]), path(&["b"])]);
}

#[test]
fn entries_is_empty_for_an_unrecorded_node() {
    let r = NameResolutions::new();
    assert!(r.entries(NodeId::next()).is_empty());
}

/// `paths` inlines two entries before spilling to the heap (see the `SmallVec<[_; 2]>` doc
/// comment on `NameResolutions::paths`). A third entry on one node exercises that spill, and
/// this confirms it isn't dropped in the process.
#[test]
fn a_node_with_three_recorded_paths_retrieves_all_three() {
    let mut r = NameResolutions::new();
    let owner = NodeId::next();
    r.record(owner, path(&["a"]), Res::Err);
    r.record(owner, path(&["b"]), Res::Err);
    r.record(owner, path(&["c"]), Res::Err);

    assert_eq!(r.get(owner, &path(&["a"])), Some(Res::Err));
    assert_eq!(r.get(owner, &path(&["b"])), Some(Res::Err));
    assert_eq!(r.get(owner, &path(&["c"])), Some(Res::Err));

    let got: Vec<_> = r.entries(owner).iter().map(|(p, _)| p.clone()).collect();
    assert_eq!(got, vec![path(&["a"]), path(&["b"]), path(&["c"])]);
}

// -----------------------------------------------------------------
// `SymbolTable::collect`
// -----------------------------------------------------------------

#[test]
fn collect_puts_a_function_in_the_value_namespace() {
    let ast = ast_from("fun f() {}");
    let table = SymbolTable::collect(&ast);
    assert!(
        table
            .lookup_function(ast.root_id(), Interner::intern("f"))
            .is_some()
    );
}

#[test]
fn collect_puts_a_struct_in_the_type_namespace() {
    let ast = ast_from("struct S {}");
    let table = SymbolTable::collect(&ast);
    assert!(matches!(
        table.lookup_type(ast.root_id(), Interner::intern("S")),
        Some(TyDef::Struct(_))
    ));
}

#[test]
fn collect_keeps_a_trait_and_an_enum_apart_by_tydef_kind() {
    let ast = ast_from("enum E { a } trait T {}");
    let table = SymbolTable::collect(&ast);
    assert!(matches!(
        table.lookup_type(ast.root_id(), Interner::intern("E")),
        Some(TyDef::Enum(_))
    ));
    assert!(matches!(
        table.lookup_type(ast.root_id(), Interner::intern("T")),
        Some(TyDef::Trait(_))
    ));
}

#[test]
fn two_declarations_of_one_name_in_one_namespace_conflict() {
    let (_, diags) = collect_with_diags("fun f() {} fun f() {}");
    assert_eq!(diags.len(), 1);
    assert!(diags[0].message.contains("is defined multiple times"));
}

#[test]
fn by_path_maps_a_canonical_path_to_its_module() {
    let ast = ast_from_files(&["module math::vector; fun dot() {}"]);
    let table = SymbolTable::collect(&ast);
    let id = module_by_path(
        &ast,
        &table,
        &[Interner::intern("math"), Interner::intern("vector")],
    );
    assert!(id.is_some());
}

// -----------------------------------------------------------------
// `SymbolTable::new` -- import resolution and the prelude
// -----------------------------------------------------------------

#[test]
fn an_import_binds_into_the_importing_modules_own_scope() {
    let ast = ast_from_files(&[
        "module math; public fun dot() {}",
        "module app; import math::dot;",
    ]);
    let table = SymbolTable::new(&ast);
    let app = module_by_path(&ast, &table, &[Interner::intern("app")]).unwrap();
    assert!(
        table
            .lookup_function(app, Interner::intern("dot"))
            .is_some()
    );
}

#[test]
fn an_import_resolves_absolutely_from_the_root_not_relative_to_where_it_is_written() {
    // `deep::inner` is written inside `app::nested`, and still resolves from the root.
    let ast = ast_from_files(&[
        "module deep; public fun inner() {}",
        "module app::nested; import deep::inner;",
    ]);
    let table = SymbolTable::new(&ast);
    let nested = module_by_path(
        &ast,
        &table,
        &[Interner::intern("app"), Interner::intern("nested")],
    )
    .unwrap();
    assert!(
        table
            .lookup_function(nested, Interner::intern("inner"))
            .is_some()
    );
}

#[test]
fn an_import_may_name_a_module_the_collect_pass_had_not_reached() {
    // The importing module is parsed first; the imported one second.
    let ast = ast_from_files(&[
        "module app; import later::thing;",
        "module later; public fun thing() {}",
    ]);
    let (table, diags) = new_with_diags(&ast);
    assert!(diags.is_empty(), "unexpected diagnostics: {diags:?}");
    let app = module_by_path(&ast, &table, &[Interner::intern("app")]).unwrap();
    assert!(
        table
            .lookup_function(app, Interner::intern("thing"))
            .is_some()
    );
}

#[test]
fn a_glob_import_copies_every_name_from_the_source_module() {
    let ast = ast_from_files(&[
        "module math; public fun dot() {} public struct Vec2 {}",
        "module app; import math::*;",
    ]);
    let table = SymbolTable::new(&ast);
    let app = module_by_path(&ast, &table, &[Interner::intern("app")]).unwrap();
    assert!(
        table
            .lookup_function(app, Interner::intern("dot"))
            .is_some()
    );
    assert!(table.lookup_type(app, Interner::intern("Vec2")).is_some());
}

#[test]
fn a_glob_import_colliding_with_a_declaration_conflicts() {
    let (_, diags) = new_with_diags_from(&[
        "module math; public fun dot() {}",
        "module app; import math::*; fun dot() {}",
    ]);
    assert_eq!(diags.len(), 1);
    assert!(diags[0].message.contains("is defined multiple times"));
}

#[test]
fn an_import_matching_two_namespaces_is_ambiguous() {
    let (_, diags) = new_with_diags_from(&[
        "module math; public fun thing() {} public struct thing {}",
        "module app; import math::thing;",
    ]);
    assert!(diags.iter().any(|d| d.message.contains("ambiguous import")));
}

#[test]
fn an_import_naming_nothing_reports_not_found() {
    let (_, diags) = new_with_diags_from(&["module app; import nowhere::gone;"]);
    assert_eq!(diags.len(), 1);
    assert!(diags[0].message.contains("cannot find"));
}

#[test]
fn the_prelude_is_found_after_imports_resolve() {
    let ast = ast_with_core();
    let table = SymbolTable::new(&ast);
    assert!(table.prelude().is_some());
}

#[test]
fn the_prelude_is_none_without_a_core_library() {
    let ast = ast_from("fun main() {}");
    let table = SymbolTable::new(&ast);
    assert!(table.prelude().is_none());
}

// -----------------------------------------------------------------
// The scope stacks -- locals, generics, `Self`
// -----------------------------------------------------------------

#[test]
fn a_local_shadows_an_outer_one_and_the_outer_is_restored_on_pop() {
    let ast = ast_from("fun main() {}");
    let mut t = SymbolTable::new(&ast);
    let outer = NodeId::next();
    let inner = NodeId::next();
    let x = Interner::intern("x");

    t.push_scope();
    t.insert_local(ident("x"), Local::Variable(outer));
    assert_eq!(t.lookup_local(x), Some(Local::Variable(outer)));

    t.push_scope();
    t.insert_local(ident("x"), Local::Variable(inner));
    assert_eq!(t.lookup_local(x), Some(Local::Variable(inner)));

    t.pop_scope();
    assert_eq!(t.lookup_local(x), Some(Local::Variable(outer)));
    t.pop_scope();
    assert_eq!(t.lookup_local(x), None);
}

#[test]
fn rebinding_in_one_scope_overwrites_rather_than_conflicting() {
    let ast = ast_from("fun main() {}");
    let mut t = SymbolTable::new(&ast);
    let first = NodeId::next();
    let second = NodeId::next();
    t.push_scope();
    t.insert_local(ident("x"), Local::Variable(first));
    t.insert_local(ident("x"), Local::Variable(second));
    assert_eq!(
        t.lookup_local(Interner::intern("x")),
        Some(Local::Variable(second))
    );
}

#[test]
fn a_generic_is_visible_inside_its_definition_and_not_outside() {
    let ast = ast_from("fun main() {}");
    let mut t = SymbolTable::new(&ast);
    let g = NodeId::next();
    let name = Interner::intern("T");

    t.push_generics(HashMap::from([(name, Type::Generic(g))]));
    assert_eq!(t.lookup_generic(name), Some(Type::Generic(g)));
    t.pop_generics();
    assert_eq!(t.lookup_generic(name), None);
}

#[test]
fn an_inner_generic_scope_shadows_an_outer_one() {
    let ast = ast_from("fun main() {}");
    let mut t = SymbolTable::new(&ast);
    let outer = NodeId::next();
    let inner = NodeId::next();
    let name = Interner::intern("T");

    t.push_generics(HashMap::from([(name, Type::Generic(outer))]));
    t.push_generics(HashMap::from([(name, Type::Generic(inner))]));
    assert_eq!(t.lookup_generic(name), Some(Type::Generic(inner)));
    t.pop_generics();
    assert_eq!(t.lookup_generic(name), Some(Type::Generic(outer)));
}

#[test]
fn self_reads_the_innermost_scope_and_is_none_when_the_stack_is_empty() {
    let ast = ast_from("fun main() {}");
    let mut t = SymbolTable::new(&ast);
    let s = NodeId::next();
    assert_eq!(t.current_self(), None);
    t.push_self(TyDef::Struct(s));
    assert_eq!(t.current_self(), Some(TyDef::Struct(s)));
    t.pop_self();
    assert_eq!(t.current_self(), None);
}

// -----------------------------------------------------------------
// Path lookup
// -----------------------------------------------------------------

#[test]
fn a_sibling_item_resolves_without_qualification() {
    let ast = ast_from_files(&["module app; fun helper() {} fun main() {}"]);
    let table = SymbolTable::new(&ast);
    let app = module_by_path(&ast, &table, &[Interner::intern("app")]).unwrap();
    assert!(matches!(
        table.lookup_value_path(app, &path(&["helper"])),
        Some(Res::Function(_))
    ));
}

#[test]
fn a_name_falls_back_to_an_ancestor_module() {
    let ast = ast_from_files(&[
        "module app; public fun shared() {}",
        "module app::inner; fun main() {}",
    ]);
    let table = SymbolTable::new(&ast);
    let inner = module_by_path(
        &ast,
        &table,
        &[Interner::intern("app"), Interner::intern("inner")],
    )
    .unwrap();
    assert!(table.lookup_value_path(inner, &path(&["shared"])).is_some());
}

#[test]
fn a_fully_qualified_path_resolves_from_anywhere() {
    let ast = ast_from_files(&[
        "module math::vector; public fun dot() {}",
        "module app::deep; fun main() {}",
    ]);
    let table = SymbolTable::new(&ast);
    let deep = module_by_path(
        &ast,
        &table,
        &[Interner::intern("app"), Interner::intern("deep")],
    )
    .unwrap();
    assert!(
        table
            .lookup_value_path(deep, &path(&["math", "vector", "dot"]))
            .is_some()
    );
}

#[test]
fn a_primitive_resolves_before_anything_else_in_type_position() {
    let ast = ast_from("fun main() {}");
    let table = SymbolTable::new(&ast);
    assert_eq!(
        table.lookup_type_path(ast.root_id(), &path(&["i32"])),
        Some(Type::Prim(PrimTy::I32))
    );
}

#[test]
fn a_generic_shadows_a_module_level_type() {
    let ast = ast_from_files(&["module app; struct T {}"]);
    let mut table = SymbolTable::new(&ast);
    let app = module_by_path(&ast, &table, &[Interner::intern("app")]).unwrap();
    let g = NodeId::next();
    table.push_generics(HashMap::from([(Interner::intern("T"), Type::Generic(g))]));
    assert_eq!(
        table.lookup_type_path(app, &path(&["T"])),
        Some(Type::Generic(g))
    );
    table.pop_generics();
    assert!(matches!(
        table.lookup_type_path(app, &path(&["T"])),
        Some(Type::Def(TyDef::Struct(_)))
    ));
}

#[test]
fn a_local_shadows_a_module_level_function_in_value_position() {
    let ast = ast_from_files(&["module app; fun x() {}"]);
    let mut table = SymbolTable::new(&ast);
    let app = module_by_path(&ast, &table, &[Interner::intern("app")]).unwrap();
    let local = NodeId::next();
    table.push_scope();
    table.insert_local(ident("x"), Local::Variable(local));
    assert_eq!(
        table.lookup_value_path(app, &path(&["x"])),
        Some(Res::Local(Local::Variable(local)))
    );
}

#[test]
fn a_multi_segment_path_walks_submodules_then_looks_up_the_last_segment() {
    let ast = ast_from_files(&[
        "module app; fun main() {}",
        "module app::inner; public struct S {}",
    ]);
    let table = SymbolTable::new(&ast);
    let app = module_by_path(&ast, &table, &[Interner::intern("app")]).unwrap();
    assert!(matches!(
        table.lookup_type_path(app, &path(&["inner", "S"])),
        Some(Type::Def(TyDef::Struct(_)))
    ));
}

#[test]
fn an_unresolvable_path_is_none() {
    let ast = ast_from("fun main() {}");
    let table = SymbolTable::new(&ast);
    assert!(
        table
            .lookup_value_path(ast.root_id(), &path(&["nope"]))
            .is_none()
    );
}

#[test]
fn pushing_generics_leaves_locals_and_self_untouched() {
    let ast = ast_from("fun main() {}");
    let mut t = SymbolTable::new(&ast);
    let local = NodeId::next();
    let self_def = TyDef::Struct(NodeId::next());
    let x = Interner::intern("x");

    t.push_scope();
    t.insert_local(ident("x"), Local::Variable(local));
    t.push_self(self_def);

    t.push_generics(HashMap::from([(
        Interner::intern("T"),
        Type::Generic(NodeId::next()),
    )]));
    assert_eq!(t.lookup_local(x), Some(Local::Variable(local)));
    assert_eq!(t.current_self(), Some(self_def));
    t.pop_generics();

    assert_eq!(t.lookup_local(x), Some(Local::Variable(local)));
    assert_eq!(t.current_self(), Some(self_def));
}

#[test]
fn pushing_a_local_scope_or_self_leaves_generics_untouched() {
    let ast = ast_from("fun main() {}");
    let mut t = SymbolTable::new(&ast);
    let name = Interner::intern("T");
    let g = NodeId::next();

    t.push_generics(HashMap::from([(name, Type::Generic(g))]));

    t.push_scope();
    t.insert_local(ident("x"), Local::Variable(NodeId::next()));
    assert_eq!(t.lookup_generic(name), Some(Type::Generic(g)));
    t.pop_scope();

    t.push_self(TyDef::Struct(NodeId::next()));
    assert_eq!(t.lookup_generic(name), Some(Type::Generic(g)));
    t.pop_self();

    assert_eq!(t.lookup_generic(name), Some(Type::Generic(g)));
}

// -----------------------------------------------------------------
// `Self`, bare traits, and `dyn`
// -----------------------------------------------------------------

#[test]
fn a_bare_trait_path_in_type_position_resolves_to_a_trait() {
    // Static dispatch: the function is monomorphized over the concrete type, as Rust's
    // `impl Trait` does. This is legal and is not an error.
    let ast = ast_from_files(&["module app; trait Show {}"]);
    let table = SymbolTable::new(&ast);
    let app = module_by_path(&ast, &table, &[Interner::intern("app")]).unwrap();
    let r = Resolver::new(table, app);
    assert!(matches!(
        r.resolve_type_path(&path(&["Show"])),
        Res::Type(Type::Def(TyDef::Trait(_)))
    ));
}

#[test]
fn dyn_on_a_trait_resolves() {
    let ast = ast_from_files(&["module app; trait Show {}"]);
    let table = SymbolTable::new(&ast);
    let app = module_by_path(&ast, &table, &[Interner::intern("app")]).unwrap();
    assert!(matches!(
        table.lookup_dyn_path(app, &path(&["Show"])),
        Res::Type(Type::Def(TyDef::Trait(_)))
    ));
}

#[test]
fn dyn_on_a_struct_errors() {
    let ast = ast_from_files(&["module app; struct S {}"]);
    let table = SymbolTable::new(&ast);
    let app = module_by_path(&ast, &table, &[Interner::intern("app")]).unwrap();
    let (res, diags) = with_diags(|| table.lookup_dyn_path(app, &path(&["S"])));
    assert_eq!(res, Res::Err);
    assert_eq!(diags.len(), 1);
    assert!(diags[0].message.contains("`dyn` requires a trait"));
}

#[test]
fn self_resolves_to_each_of_struct_enum_trait_and_extend() {
    let ast = ast_from("fun main() {}");
    let table = SymbolTable::new(&ast);
    let mut r = Resolver::new(table, ast.root_id());
    for def in [
        TyDef::Struct(NodeId::next()),
        TyDef::Enum(NodeId::next()),
        TyDef::Trait(NodeId::next()),
    ] {
        r.table.push_self(def);
        assert_eq!(
            r.resolve_type_path(&path(&["Self"])),
            Res::Type(Type::Def(def))
        );
        r.table.pop_self();
    }
}

#[test]
fn self_outside_a_definition_errors() {
    let ast = ast_from("fun main() {}");
    let table = SymbolTable::new(&ast);
    let r = Resolver::new(table, ast.root_id());
    let (res, diags) = with_diags(|| r.resolve_type_path(&path(&["Self"])));
    assert_eq!(res, Res::Err);
    assert_eq!(diags.len(), 1);
    assert!(diags[0].message.contains("`Self` is not available here"));
}

#[test]
fn an_unresolvable_type_path_reports_not_found_and_records_err() {
    let ast = ast_from("fun main() {}");
    let table = SymbolTable::new(&ast);
    let r = Resolver::new(table, ast.root_id());
    let (res, diags) = with_diags(|| r.resolve_type_path(&path(&["Nope"])));
    assert_eq!(res, Res::Err);
    assert_eq!(diags.len(), 1);
    assert!(diags[0].message.contains("cannot find"));
}

#[test]
fn lookup_variant_finds_a_variant_by_name() {
    let ast = ast_from_files(&["module app; enum Shape { circle, square }"]);
    let table = SymbolTable::new(&ast);
    let app = module_by_path(&ast, &table, &[Interner::intern("app")]).unwrap();
    let Some(Type::Def(TyDef::Enum(e))) = table.lookup_type_path(app, &path(&["Shape"])) else {
        panic!("Shape did not resolve to an enum");
    };
    assert!(
        table
            .lookup_variant(e, Interner::intern("circle"))
            .is_some()
    );
    assert!(
        table
            .lookup_variant(e, Interner::intern("triangle"))
            .is_none()
    );
}

// -----------------------------------------------------------------
// `resolve` -- the full AST traversal
// -----------------------------------------------------------------

/// Finds the one item in `ast` matching `pred`, across every module. Every fixture below
/// declares exactly the one item under test, so panicking when `pred` matches nothing (a fixture
/// that stopped parsing the way the test expects) is preferable to returning an `Option` every
/// caller would just `unwrap` anyway.
fn find_item(ast: &Ast, pred: impl Fn(&ItemKind) -> bool) -> &Item {
    ast.mod_ids()
        .find_map(|mod_id| {
            ast.module(mod_id)
                .items
                .iter()
                .find(|item| pred(&item.kind))
        })
        .expect("expected the fixture to declare a matching item")
}

fn extend_item_id(ast: &Ast) -> NodeId {
    find_item(ast, |kind| matches!(kind, ItemKind::Extend(_))).id
}

/// The first generic parameter declared anywhere in `ast`, wherever it's found -- every fixture
/// that uses this declares exactly one.
fn first_generic_id(ast: &Ast) -> NodeId {
    let item = find_item(
        ast,
        |kind| matches!(kind, ItemKind::Function(f) if !f.generics.is_empty()),
    );
    let ItemKind::Function(f) = &item.kind else {
        unreachable!("find_item's predicate only matches ItemKind::Function");
    };
    f.generics[0].id
}

/// The `NodeId` of the first parameter's type annotation in `ast`'s one function.
fn param_ty_id(ast: &Ast) -> NodeId {
    only_function(ast).params[0].ty.id
}

/// The one function `ast` declares, wherever it's found -- a top-level `fun`, an `extend`
/// block's method, or a trait's. Every fixture using this has exactly one, so the first found
/// (in item-declaration order) is the one under test.
fn only_function(ast: &Ast) -> &Function {
    for mod_id in ast.mod_ids() {
        for item in &ast.module(mod_id).items {
            match &item.kind {
                ItemKind::Function(f) => return f,
                ItemKind::Extend(e) if !e.methods.is_empty() => return &e.methods[0],
                ItemKind::Trait(t) if !t.functions.is_empty() => return &t.functions[0],
                _ => {}
            }
        }
    }
    panic!("expected the fixture to declare a function somewhere");
}

/// For a fixture of the shape `fun f() { let x = 1; let y = x; }`: the `NodeId` of the `x` path
/// expression on the right of the second `let`, and the `NodeId` of the `Pat` the first `let`
/// binds `x` with.
fn x_use_and_binding(ast: &Ast) -> (NodeId, NodeId) {
    let f = only_function(ast);
    let block = f
        .block
        .as_ref()
        .expect("expected the fixture's function to have a body");
    let StmtKind::Let { pat: binding, .. } = &block.stmts[0].kind else {
        panic!("expected the first statement to be a let binding");
    };
    let StmtKind::Let { init, .. } = &block.stmts[1].kind else {
        panic!("expected the second statement to be a let binding");
    };
    assert!(
        matches!(init.kind, ExprKind::Path(_)),
        "expected the second let's initializer to be a path expression, got {init:?}"
    );
    (init.id, binding.id)
}

#[test]
fn an_extend_blocks_two_paths_are_told_apart_by_what_they_name() {
    let ast =
        ast_from_files(&["module app; struct Vec2 {} trait Show {} extend Vec2 with Show {}"]);
    let r = resolve(&ast);
    let item = extend_item_id(&ast);
    assert!(matches!(
        r.get(item, &path(&["Vec2"])),
        Some(Res::Type(Type::Def(TyDef::Struct(_))))
    ));
    assert!(matches!(
        r.get(item, &path(&["Show"])),
        Some(Res::Type(Type::Def(TyDef::Trait(_))))
    ));
}

#[test]
fn an_extend_blocks_two_identical_paths_conflict_and_only_the_adt_path_is_recorded() {
    let ast = ast_from_files(&["module app; struct Vec2 {} extend Vec2 with Vec2 {}"]);
    let item = extend_item_id(&ast);
    let (r, diags) = with_diags(|| resolve(&ast));
    assert!(
        diags.iter().any(|d| d
            .message
            .contains("`extend` target and trait are the same type")),
        "expected a self-extend diagnostic, got {diags:?}"
    );
    assert!(matches!(
        r.get(item, &path(&["Vec2"])),
        Some(Res::Type(Type::Def(TyDef::Struct(_))))
    ));
    // Only one entry for `Vec2` -- the invariant `NameResolutions::record`'s `debug_assert!`
    // guards would otherwise be violated by recording the same path twice.
    assert_eq!(r.entries(item).len(), 1);
}

#[test]
fn a_generics_bounds_are_entries_on_the_generic_node_in_source_order() {
    let ast = ast_from_files(&["module app; trait A {} trait B {} fun f<T: A + B>() {}"]);
    let r = resolve(&ast);
    let g = first_generic_id(&ast);
    let names: Vec<_> = r
        .entries(g)
        .iter()
        .map(|(p, _)| Interner::resolve(p.segments[0].text))
        .collect();
    assert_eq!(names, vec!["A", "B"]);
}

#[test]
fn a_duplicate_bound_conflicts_and_only_the_first_writing_is_recorded() {
    let ast = ast_from_files(&["module app; trait A {} fun f<T: A + A>() {}"]);
    let g = first_generic_id(&ast);
    let (r, diags) = with_diags(|| resolve(&ast));
    assert!(
        diags.iter().any(|d| d.message.contains("duplicate bound")),
        "expected a duplicate-bound diagnostic, got {diags:?}"
    );
    assert_eq!(r.entries(g).len(), 1);
}

#[test]
fn a_block_scoped_binding_drops_at_the_closing_brace() {
    let ast = ast_from_files(&["module app; fun f() { { let x = 1; } let y = x; }"]);
    let (_, diags) = with_diags(|| resolve(&ast));
    assert!(diags.iter().any(|d| d.message.contains("cannot find `x`")));
}

#[test]
fn a_match_arm_binding_is_scoped_to_that_arm() {
    let ast = ast_from_files(&[
        "module app; enum E { a: i32 } fun f(e: E) { match e { .a(n) => n, } let y = n; }",
    ]);
    let (_, diags) = with_diags(|| resolve(&ast));
    assert!(diags.iter().any(|d| d.message.contains("cannot find `n`")));
}

/// A guard runs after its arm's pattern has matched, so the pattern's bindings have to be in
/// scope for it -- same as they are for the arm's body.
#[test]
fn a_match_arm_binding_is_visible_in_that_arms_guard() {
    let ast = ast_from_files(&[
        "module app; enum E { a: i32 } fun f(e: E) { match e { .a(n) if n > 0 => n, _ => 0 } }",
    ]);
    let (_, diags) = with_diags(|| resolve(&ast));
    assert!(
        non_lang_item_diags(&diags).is_empty(),
        "unexpected diagnostics: {diags:?}"
    );
}

/// A guard's bindings still drop at the arm's boundary, the same as the body's do.
#[test]
fn a_match_arm_guard_does_not_leak_its_own_scope() {
    let ast = ast_from_files(&[
        "module app; enum E { a: i32 } fun f(e: E) { match e { .a(n) if n > 0 => n, _ => 0, } let y = n; }",
    ]);
    let (_, diags) = with_diags(|| resolve(&ast));
    assert!(diags.iter().any(|d| d.message.contains("cannot find `n`")));
}

#[test]
fn a_generic_is_visible_in_a_method_of_the_extend_block_that_declares_it() {
    let ast = ast_from_files(&[
        "module app; struct S {} extend<T> S { fun get(self) -> T { let x = 1; } }",
    ]);
    let (_, diags) = with_diags(|| resolve(&ast));
    assert!(
        non_lang_item_diags(&diags).is_empty(),
        "unexpected diagnostics: {diags:?}"
    );
}

#[test]
fn an_unresolved_path_records_err_rather_than_leaving_the_entry_absent() {
    let ast = ast_from_files(&["module app; fun f(x: Nope) {}"]);
    let r = resolve(&ast);
    let ty = param_ty_id(&ast);
    assert_eq!(r.get(ty, &path(&["Nope"])), Some(Res::Err));
}

#[test]
fn a_path_expression_resolves_to_the_local_it_names() {
    let ast = ast_from_files(&["module app; fun f() { let x = 1; let y = x; }"]);
    let r = resolve(&ast);
    let (expr_id, pat_id) = x_use_and_binding(&ast);
    assert_eq!(
        r.get(expr_id, &path(&["x"])),
        Some(Res::Local(Local::Variable(pat_id)))
    );
}

#[test]
fn a_let_rhs_sees_the_outer_x_not_the_one_it_declares() {
    // The classic bug: binding the pattern before walking the initializer would make `x` on the
    // right resolve to itself instead of the outer binding.
    let ast = ast_from_files(&["module app; fun f() { let x = 1; { let x = x; } }"]);
    let r = resolve(&ast);
    let f = only_function(&ast);
    let block = f.block.as_ref().unwrap();
    let StmtKind::Let { pat: outer_pat, .. } = &block.stmts[0].kind else {
        panic!("expected the first statement to be a let binding");
    };
    let StmtKind::Expr { expr, .. } = &block.stmts[1].kind else {
        panic!("expected the second statement to be a block-bodied expression statement");
    };
    let ExprKind::Block(inner) = &expr.kind else {
        panic!("expected a bare block expression");
    };
    let StmtKind::Let { init, .. } = &inner.stmts[0].kind else {
        panic!("expected the inner statement to be a let binding");
    };
    assert_eq!(
        r.get(init.id, &path(&["x"])),
        Some(Res::Local(Local::Variable(outer_pat.id)))
    );
}

#[test]
fn a_closure_sees_its_enclosing_definitions_generic_and_self() {
    let ast = ast_from_files(&[
        "module app; struct S {} extend<T> S { fun get(self) -> T { let f = || -> T { self; }; } }",
    ]);
    let (_, diags) = with_diags(|| resolve(&ast));
    assert!(
        non_lang_item_diags(&diags).is_empty(),
        "unexpected diagnostics: {diags:?}"
    );
}

#[test]
fn self_outside_any_definition_records_err() {
    let ast = ast_from_files(&["module app; fun f() -> Self {}"]);
    let (r, diags) = with_diags(|| resolve(&ast));
    assert!(
        diags
            .iter()
            .any(|d| d.message.contains("`Self` is not available here"))
    );
    let f = only_function(&ast);
    let ty = f.ret.as_ref().unwrap().id;
    assert_eq!(r.get(ty, &path(&["Self"])), Some(Res::Err));
}

#[test]
fn dyn_on_a_non_trait_records_err() {
    let ast = ast_from_files(&["module app; struct S {} fun f(x: dyn S) {}"]);
    let (r, diags) = with_diags(|| resolve(&ast));
    assert!(
        diags
            .iter()
            .any(|d| d.message.contains("`dyn` requires a trait"))
    );
    let ty = param_ty_id(&ast);
    assert_eq!(r.get(ty, &path(&["S"])), Some(Res::Err));
}

/// When `adt_path` fails to resolve at all, a suppressed `Self` scope is pushed
/// (`SymbolTable::push_self_unresolved`), so a `Self` written inside the block records
/// `Res::Err` without reporting its own diagnostic -- only the one explaining why `Nope` itself
/// failed to resolve. Exactly one diagnostic, matching master's behavior for this case.
#[test]
fn an_extends_unresolved_adt_path_suppresses_the_self_diagnostic() {
    let ast = ast_from_files(&["module app; extend Nope { fun f(&self) -> Self {} }"]);
    let (r, diags) = with_diags(|| resolve(&ast));
    let diags = non_lang_item_diags(&diags);
    assert_eq!(
        diags.len(),
        1,
        "expected exactly one diagnostic, got {diags:?}"
    );
    assert!(
        diags[0].message.contains("cannot find `Nope`"),
        "expected `Nope` itself to fail to resolve, got {diags:?}"
    );
    let f = only_function(&ast);
    let ty = f.ret.as_ref().unwrap().id;
    assert_eq!(r.get(ty, &path(&["Self"])), Some(Res::Err));
}

// Note: the parser itself rejects `extend i32 ...` (a primitive is a dedicated token, not an
// `Identifier`, so it can never appear as `adt_path` -- see `typeck/traits/index.rs:224,433`),
// so the "adt_path resolved but not to a TyDef" branch above can't be exercised from source text.
// It is exercised indirectly: every extend fixture elsewhere that resolves cleanly (e.g.
// `self_resolves_to_each_of_struct_enum_trait_and_extend`) takes the `Res::Type(Type::Def(_))`
// arm, and the `_ => false` arm is straightforward enough by inspection not to need a dedicated
// (unreachable-from-source) fixture.

// -----------------------------------------------------------------
// `emit_debug::nameres_to_string`
// -----------------------------------------------------------------

/// `B` is written before `A` in source order, so it must come first in the dump regardless of
/// the `NodeId`s the global counter happened to hand out -- see the rationale on
/// `emit_debug::nameres_to_string`. Also guards the companion rule: the dump must never
/// print a `NodeId` at all.
#[test]
fn the_dump_is_ordered_by_span_and_contains_no_node_ids() {
    let ast = ast_from_files(&["module app; struct A {} struct B {} fun f(x: B, y: A) {}"]);
    let r = resolve(&ast);
    let dump = emit_debug::nameres_to_string(&ast, &r);

    let b = dump.find('B').expect("B missing from dump");
    let a = dump.find('A').expect("A missing from dump");
    assert!(b < a, "dump is not span-ordered:\n{dump}");

    assert!(
        !dump.contains("NodeId"),
        "NodeId leaked into the dump:\n{dump}"
    );
}

/// A more thorough version of the ordering check above: three entries, written out of
/// declaration order relative to their uses, must appear in the dump in the order their own
/// spans start -- not in the order `collect`/`resolve` happened to visit or record them.
#[test]
fn three_entries_out_of_declaration_order_still_print_span_ordered() {
    let ast = ast_from_files(&[
        "module app; struct Third {} struct Second {} struct First {} \
         fun f(a: First, b: Second, c: Third) {}",
    ]);
    let r = resolve(&ast);
    let dump = emit_debug::nameres_to_string(&ast, &r);

    let first = dump.find("First").expect("First missing from dump");
    let second = dump.find("Second").expect("Second missing from dump");
    let third = dump.find("Third").expect("Third missing from dump");
    assert!(
        first < second && second < third,
        "dump is not span-ordered:\n{dump}"
    );
}

/// Every `Res` variant renders as a name, not an id: a function, a local (param, self param,
/// and a `let`-bound variable), a generic parameter, a primitive, and each of struct/enum/trait
/// all appear in the dump by name.
#[test]
fn every_res_kind_renders_by_name_not_by_node_id() {
    let ast = ast_from_files(&["module app; \
         struct AStruct {} \
         enum AnEnum { a } \
         trait ATrait {} \
         fun helper() {} \
         fun f<TParam: ATrait>(z: TParam, w: i32, y: AStruct) -> AnEnum { \
             let local_var = y; \
             helper(); \
             local_var; \
         } \
         extend<T> AStruct { fun m(&self) -> T { self; } }"]);
    let (r, diags) = with_diags(|| resolve(&ast));
    assert!(
        non_lang_item_diags(&diags).is_empty(),
        "unexpected diagnostics: {diags:?}"
    );
    let dump = emit_debug::nameres_to_string(&ast, &r);

    for expected in [
        "Struct `AStruct`",
        "Enum `AnEnum`",
        "Trait `ATrait`",
        "Function `helper`",
        "Param `y`",
        "Variable `local_var`",
        "Generic `TParam`",
        "SelfParam `self`",
        "I32",
    ] {
        assert!(
            dump.contains(expected),
            "expected {expected:?} in dump:\n{dump}"
        );
    }
    assert!(
        !dump.contains("NodeId"),
        "NodeId leaked into the dump:\n{dump}"
    );
}

// -----------------------------------------------------------------
// `ExprKind::Ctor` and record-payload shorthand fields (fix round 2)
// -----------------------------------------------------------------

/// The one function `ast` declares's first `let`'s initializer expression -- every fixture below
/// declares exactly one function with exactly one `let` as its first statement.
fn first_let_init(ast: &Ast) -> &Expr {
    let f = only_function(ast);
    let block = f
        .block
        .as_ref()
        .expect("expected the fixture's function to have a body");
    let StmtKind::Let { init, .. } = &block.stmts[0].kind else {
        panic!("expected the first statement to be a let binding");
    };
    init
}

#[test]
fn a_struct_literals_path_resolves_to_its_struct() {
    let ast = ast_from_files(&["module app; struct S { a: i32 } fun f() { let v = S { a: 1 }; }"]);
    let r = resolve(&ast);
    let init = first_let_init(&ast);
    assert!(
        matches!(init.kind, ExprKind::Ctor { path: Some(_), .. }),
        "expected a struct literal, got {init:?}"
    );
    assert!(matches!(
        r.get(init.id, &path(&["S"])),
        Some(Res::Type(Type::Def(TyDef::Struct(_))))
    ));
}

#[test]
fn the_elided_ctor_forms_type_comes_from_context_so_nothing_is_recorded() {
    let ast =
        ast_from_files(&["module app; struct S { a: i32 } fun f() { let v: S = .{ a: 1 }; }"]);
    let r = resolve(&ast);
    let init = first_let_init(&ast);
    assert!(
        matches!(init.kind, ExprKind::Ctor { path: None, .. }),
        "expected the elided ctor form, got {init:?}"
    );
    assert!(
        r.entries(init.id).is_empty(),
        "the elided form has no path to record anything against"
    );
}

#[test]
fn an_unresolved_struct_literal_path_records_err() {
    let ast = ast_from_files(&["module app; fun f() { let v = Nope { a: 1 }; }"]);
    let r = resolve(&ast);
    let init = first_let_init(&ast);
    assert_eq!(r.get(init.id, &path(&["Nope"])), Some(Res::Err));
}

/// The fields of the one variant-construction expression in `ast`'s function -- the last `let`'s
/// initializer, an `ExprKind::Variant` with a record payload.
fn variant_record_fields(ast: &Ast) -> &[PayloadField<Expr>] {
    let f = only_function(ast);
    let block = f.block.as_ref().expect("expected a function body");
    let StmtKind::Let { init, .. } = &block.stmts.last().unwrap().kind else {
        panic!("expected the last statement to be a let binding");
    };
    let ExprKind::Variant { payload, .. } = &init.kind else {
        panic!("expected a variant construction expression, got {init:?}");
    };
    let Payload::Record(fields) = payload else {
        panic!("expected a record payload, got {payload:?}");
    };
    fields
}

/// A record payload's shorthand field (`{ w }`, meaning `{ w: w }`) has no `Expr` of its own for
/// the implicit value -- `ast::visit`'s `payload_values` helper silently drops it. This confirms
/// the fix keys the lookup off the field's own `NodeId` instead and still finds the local.
#[test]
fn a_variant_record_payloads_shorthand_field_resolves_its_implicit_value() {
    let ast = ast_from_files(&["module app; enum Shape { rect: { w: i32, h: i32 } } \
         fun f() { let w = 1; let h = 2; let s = .rect { w, h }; }"]);
    let r = resolve(&ast);
    let fields = variant_record_fields(&ast);
    assert_eq!(fields.len(), 2);
    for field in fields {
        assert!(
            field.value.is_none(),
            "expected {field:?} to be the shorthand form"
        );
        let name = Interner::resolve(field.name.text);
        assert!(
            matches!(
                r.get(field.id, &path(&[name])),
                Some(Res::Local(Local::Variable(_)))
            ),
            "expected the shorthand field {name:?} to resolve to a local"
        );
    }
}

/// A record *pattern* payload's shorthand field (`{ w }`) binds `w`, the same as an ordinary
/// `PatKind::Binding` would -- but again has no `Pat` of its own for `payload_values` to hand
/// back, so nothing binds it without the fix. Verified indirectly: `w` resolves inside the arm
/// (no diagnostic there) and is out of scope again immediately after it (one diagnostic, not
/// zero or two).
#[test]
fn a_match_arms_record_payload_shorthand_binds_its_fields() {
    let ast = ast_from_files(&["module app; enum Shape { rect: { w: i32, h: i32 } } \
         fun f(s: Shape) { match s { .rect { w, h } => w, } let y = w; }"]);
    let (_, diags) = with_diags(|| resolve(&ast));
    let not_found_w: Vec<_> = non_lang_item_diags(&diags)
        .into_iter()
        .filter(|d| d.message.contains("cannot find `w`"))
        .collect();
    assert_eq!(
        not_found_w.len(),
        1,
        "expected exactly one `cannot find \\`w\\`` (the use after the match, once the arm's \
         scope has popped) -- zero would mean the arm-scoped use also failed, two would mean the \
         shorthand never bound `w` at all: {diags:?}"
    );
}

// -----------------------------------------------------------------
// Namespaces and module structure
// -----------------------------------------------------------------

/// A function and a struct are declared into separate namespaces (see
/// `SymbolTable::collect_module`'s three-way match), so sharing a spelling is not a conflict --
/// only two declarations *in the same namespace* are.
#[test]
fn a_function_and_a_struct_of_the_same_name_do_not_conflict() {
    let ast = ast_from_files(&["module app; fun Point() {} struct Point {}"]);
    let (_, diags) = with_diags(|| resolve(&ast));
    assert!(
        non_lang_item_diags(&diags).is_empty(),
        "unexpected diagnostics: {diags:?}"
    );
}

/// Likewise a function and a module: `insert_mod` and `insert_function` write to different maps.
#[test]
fn a_function_and_a_submodule_of_the_same_name_do_not_conflict() {
    let ast = ast_from_files(&[
        "module app; fun helper() {}",
        "module app::helper; fun f() {}",
    ]);
    let (_, diags) = with_diags(|| resolve(&ast));
    assert!(
        non_lang_item_diags(&diags).is_empty(),
        "unexpected diagnostics: {diags:?}"
    );
}

/// Two modules may each declare a function of the same name: each module owns its own
/// namespace, so `collect_module`'s per-module `ModuleScope` never even compares the two.
#[test]
fn two_unrelated_modules_may_each_declare_a_function_of_the_same_name() {
    let ast = ast_from_files(&["module a; fun helper() {}", "module b; fun helper() {}"]);
    let (_, diags) = with_diags(|| resolve(&ast));
    assert!(
        non_lang_item_diags(&diags).is_empty(),
        "unexpected diagnostics: {diags:?}"
    );
}

/// `Visibility` is parsed onto every item (see `ast::Visibility`) and `SymbolTable` reads it
/// back at every lookup that can reach across module boundaries (`lookup_value_path`,
/// `lookup_type_path`, and import resolution): a private item (the default; there is no
/// `private` keyword, only the absence of `public`) is visible only from its own declaring
/// module and that module's descendants, so importing it into an unrelated module is rejected.
#[test]
fn a_private_item_is_not_importable_from_an_unrelated_module() {
    let ast = ast_from_files(&[
        "module math; fun secret() {}",
        "module app; import math::secret; fun f() { secret(); }",
    ]);
    let (_, diags) = with_diags(|| resolve(&ast));
    assert!(
        !non_lang_item_diags(&diags).is_empty(),
        "`secret` is declared without `public` in `math`, so importing it into the unrelated \
         module `app` should be rejected: {diags:?}"
    );
}

/// The module chain only walks upward through ancestors ([`SymbolTable::module_chain`]), so a
/// parent never sees a child module's declarations.
#[test]
fn a_parent_module_cannot_see_a_childs_declaration() {
    let ast = ast_from_files(&[
        "module app; fun f() { helper(); }",
        "module app::inner; fun helper() {}",
    ]);
    let (_, diags) = with_diags(|| resolve(&ast));
    assert!(
        non_lang_item_diags(&diags)
            .iter()
            .any(|d| d.message.contains("cannot find `helper`")),
        "expected `helper` to be unresolvable from the parent: {diags:?}"
    );
}

/// Two sibling modules -- neither an ancestor of the other -- do not see each other's
/// declarations without an explicit `import`.
#[test]
fn sibling_modules_do_not_see_each_other() {
    let ast = ast_from_files(&[
        "module a; fun helper() {}",
        "module b; fun f() { helper(); }",
    ]);
    let (_, diags) = with_diags(|| resolve(&ast));
    assert!(
        non_lang_item_diags(&diags)
            .iter()
            .any(|d| d.message.contains("cannot find `helper`")),
        "expected `helper` to be unresolvable from an unrelated module: {diags:?}"
    );
}

/// The module-chain fallback keeps walking past a direct parent to every ancestor, not just one
/// level up.
#[test]
fn a_name_falls_back_through_three_levels_of_ancestry() {
    let ast = ast_from_files(&[
        "module app; public fun shared() {}",
        "module app::mid; fun unused() {}",
        "module app::mid::deep; fun f() { shared(); }",
    ]);
    let (_, diags) = with_diags(|| resolve(&ast));
    assert!(
        non_lang_item_diags(&diags).is_empty(),
        "unexpected diagnostics: {diags:?}"
    );
}

/// A path segment that names a function rather than a module is not found by `walk_modules`
/// (which only ever consults the module namespace), so the whole path fails.
#[test]
fn a_function_used_as_a_path_qualifier_does_not_resolve() {
    let ast = ast_from_files(&["module app; fun helper() {} fun f() { helper::thing(); }"]);
    let (_, diags) = with_diags(|| resolve(&ast));
    assert!(
        non_lang_item_diags(&diags)
            .iter()
            .any(|d| d.message.contains("cannot find")),
        "expected the qualified path to fail: {diags:?}"
    );
}

/// A multi-segment path whose prefix resolves but whose final segment does not is still exactly
/// one "not found" -- not a separate diagnostic for each segment.
#[test]
fn an_existing_modules_missing_member_reports_once() {
    let ast = ast_from_files(&[
        "module math; public fun dot() {}",
        "module app; fun f() { math::cross(); }",
    ]);
    let (_, diags) = with_diags(|| resolve(&ast));
    let diags = non_lang_item_diags(&diags);
    assert_eq!(diags.len(), 1, "{diags:?}");
    assert!(diags[0].message.contains("cannot find `cross`"));
}

// -----------------------------------------------------------------
// Imports: aliasing and glob collisions
// -----------------------------------------------------------------

/// An aliased import binds under the alias; the original name is not also bound.
#[test]
fn an_aliased_import_binds_under_the_alias_only() {
    let ast = ast_from_files(&[
        "module math; public fun dot() {}",
        "module app; import math::dot as scalar_product;",
    ]);
    let table = SymbolTable::new(&ast);
    let app = module_by_path(&ast, &table, &[Interner::intern("app")]).unwrap();
    assert!(
        table
            .lookup_function(app, Interner::intern("scalar_product"))
            .is_some()
    );
    assert!(
        table
            .lookup_function(app, Interner::intern("dot"))
            .is_none()
    );
}

/// Two glob imports that each bring in a name of the same spelling conflict exactly as two
/// ordinary declarations would -- `import_glob` calls the same `insert_*` that reports it.
#[test]
fn two_glob_imports_colliding_on_a_name_conflict() {
    let (_, diags) = new_with_diags_from(&[
        "module a; public fun thing() {}",
        "module b; public fun thing() {}",
        "module app; import a::*; import b::*;",
    ]);
    assert_eq!(diags.len(), 1);
    assert!(diags[0].message.contains("is defined multiple times"));
}

/// An imported struct is usable in an ordinary type position, the same as a locally declared one.
#[test]
fn an_imported_struct_is_usable_in_a_type_position() {
    let ast = ast_from_files(&[
        "module shapes; public struct Circle { r: i32 }",
        "module app; import shapes::Circle; fun f(c: Circle) {}",
    ]);
    let (_, diags) = with_diags(|| resolve(&ast));
    assert!(
        non_lang_item_diags(&diags).is_empty(),
        "unexpected diagnostics: {diags:?}"
    );
}

/// An imported trait is usable as a bound.
#[test]
fn an_imported_trait_is_usable_as_a_bound() {
    let ast = ast_from_files(&[
        "module traits; public trait Show { fun show(&self); }",
        "module app; import traits::Show; fun f<T: Show>(x: T) {}",
    ]);
    let (_, diags) = with_diags(|| resolve(&ast));
    assert!(
        non_lang_item_diags(&diags).is_empty(),
        "unexpected diagnostics: {diags:?}"
    );
}

// -----------------------------------------------------------------
// Scoping edge cases
// -----------------------------------------------------------------

/// `visit_stmt`'s `With` arm visits each lend's pattern (binding it into whatever scope is
/// already open) and only then visits `block`, which pushes a scope of its own -- there is no
/// scope bracketing the `with` statement itself. So a lend's binding outlives the block written
/// after it, for the rest of the *enclosing* block. Documents this today; a design that meant a
/// lend to be scoped to its own `with` would need its own push/pop around the whole statement.
#[test]
fn a_with_lends_binding_outlives_its_own_written_block() {
    let ast = ast_from_files(&["module app; fun f() { with x = 1 { } let y = x; }"]);
    let (_, diags) = with_diags(|| resolve(&ast));
    assert!(
        non_lang_item_diags(&diags).is_empty(),
        "expected `x` to still be in scope after the `with`'s block: {diags:?}"
    );
}

/// A `for` loop's pattern binding is scoped to the loop: `visit_stmt` pushes a scope around the
/// whole `For` statement before `walk_stmt` binds the pattern, so (unlike `with`, above) nothing
/// leaks past the closing brace.
#[test]
fn a_for_loops_pattern_binding_does_not_outlive_the_loop() {
    let ast = ast_from_files(&["module app; fun f(xs: i32) { for x in xs { } let y = x; }"]);
    let (_, diags) = with_diags(|| resolve(&ast));
    assert!(
        non_lang_item_diags(&diags)
            .iter()
            .any(|d| d.message.contains("cannot find `x`")),
        "expected `x` to have dropped out of scope: {diags:?}"
    );
}

/// Likewise `while let`'s pattern binding.
#[test]
fn a_while_lets_pattern_binding_does_not_outlive_the_loop() {
    let ast =
        ast_from_files(&["module app; fun f(opt: bool) { while let x = opt { } let y = x; }"]);
    let (_, diags) = with_diags(|| resolve(&ast));
    assert!(
        non_lang_item_diags(&diags)
            .iter()
            .any(|d| d.message.contains("cannot find `x`")),
        "expected `x` to have dropped out of scope: {diags:?}"
    );
}

/// A closure's own parameter shadows an outer local of the same name for the closure's body, and
/// the outer one is unaffected once the closure literal ends.
#[test]
fn a_closure_parameter_shadows_an_outer_local_of_the_same_name() {
    let ast =
        ast_from_files(&["module app; fun f() { let x = 1; let g = |x: i32| { x }; let y = x; }"]);
    let (_, diags) = with_diags(|| resolve(&ast));
    assert!(
        non_lang_item_diags(&diags).is_empty(),
        "unexpected diagnostics: {diags:?}"
    );
}

/// Two generic parameters of the same name conflict the same way two module-level declarations
/// of one name do: `Resolver::push_generics` inserts them one at a time and calls
/// `report_conflict` on a repeat, rather than building the scope with a plain `HashMap::collect`
/// that would silently keep only the last.
#[test]
fn two_generic_parameters_of_the_same_name_conflict() {
    let ast = ast_from_files(&["module app; fun f<T, T>(x: T) {}"]);
    let (_, diags) = with_diags(|| resolve(&ast));
    assert!(
        non_lang_item_diags(&diags)
            .iter()
            .any(|d| d.message.contains("is defined multiple times")),
        "expected a duplicate generic parameter name to be reported: {diags:?}"
    );
}

// -----------------------------------------------------------------
// Forward references and self-reference
// -----------------------------------------------------------------

/// `SymbolTable::collect` walks every module's declarations before `resolve_module` looks inside
/// any of their bodies, so a function may call itself.
#[test]
fn a_function_may_call_itself_recursively() {
    let ast = ast_from_files(&["module app; fun fact(n: i32) -> i32 { return fact(n); }"]);
    let (_, diags) = with_diags(|| resolve(&ast));
    assert!(
        non_lang_item_diags(&diags).is_empty(),
        "unexpected diagnostics: {diags:?}"
    );
}

/// Two functions may call each other regardless of which is declared first in the file, for the
/// same reason: both are in the module's `ModuleScope` before either body is visited.
#[test]
fn two_functions_may_call_each_other_regardless_of_declaration_order() {
    let ast = ast_from_files(&["module app; fun a() { b(); } fun b() { a(); }"]);
    let (_, diags) = with_diags(|| resolve(&ast));
    assert!(
        non_lang_item_diags(&diags).is_empty(),
        "unexpected diagnostics: {diags:?}"
    );
}

/// A struct may reference itself through a field, so long as the reference is indirect (a
/// pointer-sized reference here, rather than the struct embedding itself by value).
#[test]
fn a_struct_may_reference_itself_through_a_field_type() {
    let ast = ast_from_files(&["module app; struct Node { next: &Node }"]);
    let (_, diags) = with_diags(|| resolve(&ast));
    assert!(
        non_lang_item_diags(&diags).is_empty(),
        "unexpected diagnostics: {diags:?}"
    );
}

/// Two structs may reference each other regardless of declaration order, for the same
/// forward-declaration reason functions can.
#[test]
fn two_structs_may_reference_each_other_regardless_of_declaration_order() {
    let ast = ast_from_files(&["module app; struct A { b: &B } struct B { a: &A }"]);
    let (_, diags) = with_diags(|| resolve(&ast));
    assert!(
        non_lang_item_diags(&diags).is_empty(),
        "unexpected diagnostics: {diags:?}"
    );
}

/// An enum variant may reference its own enum through a reference, the same as a struct field.
#[test]
fn an_enum_variant_may_reference_its_own_enum_through_a_reference() {
    let ast = ast_from_files(&["module app; enum List { cons: &List, nil }"]);
    let (_, diags) = with_diags(|| resolve(&ast));
    assert!(
        non_lang_item_diags(&diags).is_empty(),
        "unexpected diagnostics: {diags:?}"
    );
}
