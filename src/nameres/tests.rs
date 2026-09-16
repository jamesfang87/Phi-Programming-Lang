use crate::ast::{
    Ast, Expr, ExprKind, Function, Ident, Item, ItemKind, NodeId, ParsedSrcFile, Path, Payload,
    PayloadField, StmtKind, Symbol,
};
use crate::diagnostics::Diagnostic;
use crate::driver::emit_debug;
use crate::driver::source::{FileOrigin, SrcSpan};
use crate::lexer::Lexer;
use crate::nameres::res::PrimTy;
use crate::nameres::res::{Local, Res, TyDef, Type};
use crate::nameres::resolve;
use crate::nameres::resolver::Resolver;
use crate::nameres::results::NameResolutions;
use crate::nameres::symbol_table::SymbolTable;
use crate::parser::Parser;

fn ident(text: &str) -> Ident {
    Ident {
        text: crate::testing::intern(text),
        span: SrcSpan::new(0, 1),
    }
}

fn parse_one(src: &str) -> ParsedSrcFile {
    let chars: Vec<char> = src.chars().collect();
    let offset = crate::testing::add_file("<test>".to_string(), chars.clone(), FileOrigin::User);
    let tokens = Lexer::new(crate::testing::session(), &chars, offset).tokenize();
    Parser::new(crate::testing::session()).parse(&tokens, offset)
}

fn ast_from(src: &str) -> Ast {
    ast_from_files(&[src])
}

fn ast_from_files(sources: &[&str]) -> Ast {
    crate::testing::clear_diagnostics();
    crate::testing::clear_interner();
    let files: Vec<ParsedSrcFile> = sources.iter().map(|src| parse_one(src)).collect();
    let diagnostics = crate::testing::diagnostics();
    assert!(
        diagnostics.is_empty(),
        "unexpected diagnostics for {sources:?}: {diagnostics:?}"
    );
    Ast::from(files)
}

fn module_by_path(ast: &Ast, table: &SymbolTable, segments: &[Symbol]) -> Option<NodeId> {
    segments.iter().try_fold(ast.root_id(), |current, &seg| {
        table.lookup_mod(current, seg)
    })
}

fn collect_with_diags(src: &str) -> (SymbolTable<'_>, Vec<Diagnostic>) {
    fn inner(ast: &Ast) -> (SymbolTable<'_>, Vec<Diagnostic>) {
        crate::testing::clear_diagnostics();
        let table = SymbolTable::collect(crate::testing::session(), ast);
        (table, crate::testing::diagnostics())
    }
    let ast: &'static Ast = Box::leak(Box::new(ast_from(src)));
    inner(ast)
}

fn new_with_diags(ast: &Ast) -> (SymbolTable<'_>, Vec<Diagnostic>) {
    crate::testing::clear_diagnostics();
    let table = SymbolTable::new(crate::testing::session(), ast);
    (table, crate::testing::diagnostics())
}

fn new_with_diags_from(sources: &[&str]) -> (SymbolTable<'static>, Vec<Diagnostic>) {
    let ast: &'static Ast = Box::leak(Box::new(ast_from_files(sources)));
    new_with_diags(ast)
}

fn ast_with_core() -> Ast {
    crate::testing::clear_diagnostics();
    crate::testing::clear_interner();
    let core_files = crate::testing::collect_core();
    let files: Vec<ParsedSrcFile> = core_files
        .iter()
        .map(|file| {
            let tokens =
                Lexer::new(crate::testing::session(), &file.content, file.global_offset).tokenize();
            Parser::new(crate::testing::session()).parse(&tokens, file.global_offset)
        })
        .collect();
    let diagnostics = crate::testing::diagnostics();
    assert!(
        diagnostics.is_empty(),
        "unexpected diagnostics loading the core library: {diagnostics:?}"
    );
    Ast::from(files)
}

fn with_diags<T>(f: impl FnOnce() -> T) -> (T, Vec<Diagnostic>) {
    crate::testing::clear_diagnostics();
    let result = f();
    (result, crate::testing::diagnostics())
}

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
                text: crate::testing::intern(s),
                span,
            })
            .collect(),
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

#[test]
fn collect_puts_a_function_in_the_value_namespace() {
    let ast = ast_from("fun f() {}");
    let table = SymbolTable::collect(crate::testing::session(), &ast);
    assert!(
        table
            .lookup_function(ast.root_id(), crate::testing::intern("f"))
            .is_some()
    );
}

#[test]
fn collect_puts_a_struct_in_the_type_namespace() {
    let ast = ast_from("struct S {}");
    let table = SymbolTable::collect(crate::testing::session(), &ast);
    assert!(matches!(
        table.lookup_type(ast.root_id(), crate::testing::intern("S")),
        Some(TyDef::Struct(_))
    ));
}

#[test]
fn collect_keeps_a_trait_and_an_enum_apart_by_tydef_kind() {
    let ast = ast_from("enum E { a } trait T {}");
    let table = SymbolTable::collect(crate::testing::session(), &ast);
    assert!(matches!(
        table.lookup_type(ast.root_id(), crate::testing::intern("E")),
        Some(TyDef::Enum(_))
    ));
    assert!(matches!(
        table.lookup_type(ast.root_id(), crate::testing::intern("T")),
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
    let table = SymbolTable::collect(crate::testing::session(), &ast);
    let id = module_by_path(
        &ast,
        &table,
        &[
            crate::testing::intern("math"),
            crate::testing::intern("vector"),
        ],
    );
    assert!(id.is_some());
}

#[test]
fn an_import_binds_into_the_importing_modules_own_scope() {
    let ast = ast_from_files(&[
        "module math; public fun dot() {}",
        "module app; import math::dot;",
    ]);
    let table = SymbolTable::new(crate::testing::session(), &ast);
    let app = module_by_path(&ast, &table, &[crate::testing::intern("app")]).unwrap();
    assert!(
        table
            .lookup_function(app, crate::testing::intern("dot"))
            .is_some()
    );
}

#[test]
fn an_import_resolves_absolutely_from_the_root_not_relative_to_where_it_is_written() {
    let ast = ast_from_files(&[
        "module deep; public fun inner() {}",
        "module app::nested; import deep::inner;",
    ]);
    let table = SymbolTable::new(crate::testing::session(), &ast);
    let nested = module_by_path(
        &ast,
        &table,
        &[
            crate::testing::intern("app"),
            crate::testing::intern("nested"),
        ],
    )
    .unwrap();
    assert!(
        table
            .lookup_function(nested, crate::testing::intern("inner"))
            .is_some()
    );
}

#[test]
fn an_import_may_name_a_module_the_collect_pass_had_not_reached() {
    let ast = ast_from_files(&[
        "module app; import later::thing;",
        "module later; public fun thing() {}",
    ]);
    let (table, diags) = new_with_diags(&ast);
    assert!(diags.is_empty(), "unexpected diagnostics: {diags:?}");
    let app = module_by_path(&ast, &table, &[crate::testing::intern("app")]).unwrap();
    assert!(
        table
            .lookup_function(app, crate::testing::intern("thing"))
            .is_some()
    );
}

#[test]
fn a_glob_import_copies_every_name_from_the_source_module() {
    let ast = ast_from_files(&[
        "module math; public fun dot() {} public struct Vec2 {}",
        "module app; import math::*;",
    ]);
    let table = SymbolTable::new(crate::testing::session(), &ast);
    let app = module_by_path(&ast, &table, &[crate::testing::intern("app")]).unwrap();
    assert!(
        table
            .lookup_function(app, crate::testing::intern("dot"))
            .is_some()
    );
    assert!(
        table
            .lookup_type(app, crate::testing::intern("Vec2"))
            .is_some()
    );
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
    let table = SymbolTable::new(crate::testing::session(), &ast);
    assert!(table.prelude().is_some());
}

#[test]
fn the_prelude_is_none_without_a_core_library() {
    let ast = ast_from("fun main() {}");
    let table = SymbolTable::new(crate::testing::session(), &ast);
    assert!(table.prelude().is_none());
}

#[test]
fn a_local_shadows_an_outer_one_and_the_outer_is_restored_on_pop() {
    let ast = ast_from("fun main() {}");
    let mut t = SymbolTable::new(crate::testing::session(), &ast);
    let outer = NodeId::next();
    let inner = NodeId::next();
    let x = crate::testing::intern("x");

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
    let mut t = SymbolTable::new(crate::testing::session(), &ast);
    let first = NodeId::next();
    let second = NodeId::next();
    t.push_scope();
    t.insert_local(ident("x"), Local::Variable(first));
    t.insert_local(ident("x"), Local::Variable(second));
    assert_eq!(
        t.lookup_local(crate::testing::intern("x")),
        Some(Local::Variable(second))
    );
}

#[test]
fn a_generic_is_visible_inside_its_definition_and_not_outside() {
    let ast = ast_from("fun main() {}");
    let mut t = SymbolTable::new(crate::testing::session(), &ast);
    let g = NodeId::next();
    let name = crate::testing::intern("T");

    t.push_generics();
    t.insert_generic(name, Type::Generic(g));
    assert_eq!(t.lookup_generic(name), Some(Type::Generic(g)));
    t.pop_generics();
    assert_eq!(t.lookup_generic(name), None);
}

#[test]
fn an_inner_generic_scope_shadows_an_outer_one() {
    let ast = ast_from("fun main() {}");
    let mut t = SymbolTable::new(crate::testing::session(), &ast);
    let outer = NodeId::next();
    let inner = NodeId::next();
    let name = crate::testing::intern("T");

    t.push_generics();
    t.insert_generic(name, Type::Generic(outer));
    t.push_generics();
    t.insert_generic(name, Type::Generic(inner));
    assert_eq!(t.lookup_generic(name), Some(Type::Generic(inner)));
    t.pop_generics();
    assert_eq!(t.lookup_generic(name), Some(Type::Generic(outer)));
}

#[test]
fn self_reads_the_innermost_scope_and_is_none_when_the_stack_is_empty() {
    let ast = ast_from("fun main() {}");
    let mut t = SymbolTable::new(crate::testing::session(), &ast);
    let s = NodeId::next();
    assert_eq!(t.lookup_self(), None);
    t.insert_self(Type::Def(TyDef::Struct(s)));
    assert_eq!(t.lookup_self(), Some(Type::Def(TyDef::Struct(s))));
    t.pop_self();
    assert_eq!(t.lookup_self(), None);
}

#[test]
fn a_sibling_item_resolves_without_qualification() {
    let ast = ast_from_files(&["module app; fun helper() {} fun main() {}"]);
    let table = SymbolTable::new(crate::testing::session(), &ast);
    let app = module_by_path(&ast, &table, &[crate::testing::intern("app")]).unwrap();
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
    let table = SymbolTable::new(crate::testing::session(), &ast);
    let inner = module_by_path(
        &ast,
        &table,
        &[
            crate::testing::intern("app"),
            crate::testing::intern("inner"),
        ],
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
    let table = SymbolTable::new(crate::testing::session(), &ast);
    let deep = module_by_path(
        &ast,
        &table,
        &[
            crate::testing::intern("app"),
            crate::testing::intern("deep"),
        ],
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
    let table = SymbolTable::new(crate::testing::session(), &ast);
    assert_eq!(
        table.lookup_type_path(ast.root_id(), &path(&["i32"])),
        Res::Type(Type::Prim(PrimTy::I32))
    );
}

#[test]
fn a_generic_shadows_a_module_level_type() {
    let ast = ast_from_files(&["module app; struct T {}"]);
    let mut table = SymbolTable::new(crate::testing::session(), &ast);
    let app = module_by_path(&ast, &table, &[crate::testing::intern("app")]).unwrap();
    let g = NodeId::next();
    table.push_generics();
    table.insert_generic(crate::testing::intern("T"), Type::Generic(g));
    assert_eq!(
        table.lookup_type_path(app, &path(&["T"])),
        Res::Type(Type::Generic(g))
    );
    table.pop_generics();
    assert!(matches!(
        table.lookup_type_path(app, &path(&["T"])),
        Res::Type(Type::Def(TyDef::Struct(_)))
    ));
}

#[test]
fn a_local_shadows_a_module_level_function_in_value_position() {
    let ast = ast_from_files(&["module app; fun x() {}"]);
    let mut table = SymbolTable::new(crate::testing::session(), &ast);
    let app = module_by_path(&ast, &table, &[crate::testing::intern("app")]).unwrap();
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
    let table = SymbolTable::new(crate::testing::session(), &ast);
    let app = module_by_path(&ast, &table, &[crate::testing::intern("app")]).unwrap();
    assert!(matches!(
        table.lookup_type_path(app, &path(&["inner", "S"])),
        Res::Type(Type::Def(TyDef::Struct(_)))
    ));
}

#[test]
fn an_unresolvable_path_is_none() {
    let ast = ast_from("fun main() {}");
    let table = SymbolTable::new(crate::testing::session(), &ast);
    assert!(
        table
            .lookup_value_path(ast.root_id(), &path(&["nope"]))
            .is_none()
    );
}

#[test]
fn pushing_generics_leaves_locals_and_self_untouched() {
    let ast = ast_from("fun main() {}");
    let mut t = SymbolTable::new(crate::testing::session(), &ast);
    let local = NodeId::next();
    let self_def = Type::Def(TyDef::Struct(NodeId::next()));
    let x = crate::testing::intern("x");

    t.push_scope();
    t.insert_local(ident("x"), Local::Variable(local));
    t.insert_self(self_def);

    t.push_generics();
    t.insert_generic(crate::testing::intern("T"), Type::Generic(NodeId::next()));
    assert_eq!(t.lookup_local(x), Some(Local::Variable(local)));
    assert_eq!(t.lookup_self(), Some(self_def));
    t.pop_generics();

    assert_eq!(t.lookup_local(x), Some(Local::Variable(local)));
    assert_eq!(t.lookup_self(), Some(self_def));
}

#[test]
fn pushing_a_local_scope_or_self_leaves_generics_untouched() {
    let ast = ast_from("fun main() {}");
    let mut t = SymbolTable::new(crate::testing::session(), &ast);
    let name = crate::testing::intern("T");
    let g = NodeId::next();

    t.push_generics();
    t.insert_generic(name, Type::Generic(g));

    t.push_scope();
    t.insert_local(ident("x"), Local::Variable(NodeId::next()));
    assert_eq!(t.lookup_generic(name), Some(Type::Generic(g)));
    t.pop_scope();

    t.insert_self(Type::Def(TyDef::Struct(NodeId::next())));
    assert_eq!(t.lookup_generic(name), Some(Type::Generic(g)));
    t.pop_self();

    assert_eq!(t.lookup_generic(name), Some(Type::Generic(g)));
}

#[test]
fn a_bare_trait_path_in_type_position_resolves_to_a_trait() {
    let ast = ast_from_files(&["module app; trait Show {}"]);
    let table = SymbolTable::new(crate::testing::session(), &ast);
    let app = module_by_path(&ast, &table, &[crate::testing::intern("app")]).unwrap();
    let r = Resolver::new(crate::testing::session(), table, app);
    assert!(matches!(
        r.table.lookup_type_path(app, &path(&["Show"])),
        Res::Type(Type::Def(TyDef::Trait(_)))
    ));
}

#[test]
fn dyn_on_a_trait_resolves() {
    let ast = ast_from_files(&["module app; trait Show {}"]);
    let table = SymbolTable::new(crate::testing::session(), &ast);
    let app = module_by_path(&ast, &table, &[crate::testing::intern("app")]).unwrap();
    assert!(matches!(
        table.lookup_dyn_path(app, &path(&["Show"])),
        Res::Type(Type::Def(TyDef::Trait(_)))
    ));
}

#[test]
fn dyn_on_a_struct_errors() {
    let ast = ast_from_files(&["module app; struct S {}"]);
    let table = SymbolTable::new(crate::testing::session(), &ast);
    let app = module_by_path(&ast, &table, &[crate::testing::intern("app")]).unwrap();
    let (res, diags) = with_diags(|| table.lookup_dyn_path(app, &path(&["S"])));
    assert_eq!(res, Res::Err);
    assert_eq!(diags.len(), 1);
    assert!(diags[0].message.contains("`dyn` requires a trait"));
}

#[test]
fn self_resolves_to_each_of_struct_enum_trait_and_extend() {
    let ast = ast_from("fun main() {}");
    let table = SymbolTable::new(crate::testing::session(), &ast);
    let mut r = Resolver::new(crate::testing::session(), table, ast.root_id());
    for def in [
        TyDef::Struct(NodeId::next()),
        TyDef::Enum(NodeId::next()),
        TyDef::Trait(NodeId::next()),
    ] {
        r.table.insert_self(Type::Def(def));
        assert_eq!(
            r.table.lookup_self_res(SrcSpan::new(0, 0)),
            Res::SelfTy(Type::Def(def))
        );
        r.table.pop_self();
    }
}

#[test]
fn self_as_an_access_base_records_self_ty() {
    let ast = ast_from_files(&[
        "module app; enum Shape { unit } extend Shape { fun f() { let s = Self.unit; } }",
    ]);
    let r = resolve(crate::testing::session(), &ast);
    let base = access_base_of_first_let(&ast);
    assert!(matches!(
        r.get(base.id, &path(&["Self"])),
        Some(Res::SelfTy(Type::Def(TyDef::Enum(_))))
    ));
}

#[test]
fn a_type_named_outright_is_not_recorded_as_self_ty() {
    let ast = ast_from_files(&[
        "module app; enum Shape { unit } extend Shape { fun f() { let s = Shape.unit; } }",
    ]);
    let r = resolve(crate::testing::session(), &ast);
    let base = access_base_of_first_let(&ast);
    assert!(matches!(
        r.get(base.id, &path(&["Shape"])),
        Some(Res::Type(Type::Def(TyDef::Enum(_))))
    ));
}

#[test]
fn self_resolves_to_a_primitive_inside_a_primitive_extend_block() {
    let ast = ast_from("extend i32 { fun f(x: Self) {} }");
    let (_, diags) = with_diags(|| resolve(crate::testing::session(), &ast));
    assert!(
        !diags.iter().any(|d| d.message.contains("`Self`")),
        "unexpected diagnostics: {diags:?}"
    );
}

#[test]
fn self_outside_a_definition_errors() {
    let ast = ast_from("fun main() {}");
    let table = SymbolTable::new(crate::testing::session(), &ast);
    let r = Resolver::new(crate::testing::session(), table, ast.root_id());
    let (res, diags) = with_diags(|| r.table.lookup_self_res(SrcSpan::new(0, 0)));
    assert_eq!(res, Res::Err);
    assert_eq!(diags.len(), 1);
    assert!(diags[0].message.contains("`Self` is not available here"));
}

#[test]
fn an_unresolvable_type_path_reports_not_found_and_records_err() {
    let ast = ast_from("fun main() {}");
    let table = SymbolTable::new(crate::testing::session(), &ast);
    let r = Resolver::new(crate::testing::session(), table, ast.root_id());
    let (res, diags) = with_diags(|| r.table.lookup_type_path(ast.root_id(), &path(&["Nope"])));
    assert_eq!(res, Res::Err);
    assert_eq!(diags.len(), 1);
    assert!(diags[0].message.contains("cannot find"));
}

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

fn extend_self_ty_id(ast: &Ast) -> NodeId {
    let ItemKind::Extend(e) = &find_item(ast, |kind| matches!(kind, ItemKind::Extend(_))).kind
    else {
        unreachable!("just matched on ItemKind::Extend");
    };
    e.self_ty.id
}

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

fn param_ty_id(ast: &Ast) -> NodeId {
    only_function(ast).params[0].ty.id
}

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
    let r = resolve(crate::testing::session(), &ast);
    let item = extend_item_id(&ast);
    let self_ty = extend_self_ty_id(&ast);
    assert!(matches!(
        r.get(self_ty, &path(&["Vec2"])),
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
    let self_ty = extend_self_ty_id(&ast);
    let (r, diags) = with_diags(|| resolve(crate::testing::session(), &ast));
    assert!(
        diags.iter().any(|d| d
            .message
            .contains("`extend` target and trait are the same type")),
        "expected a self-extend diagnostic, got {diags:?}"
    );
    assert!(matches!(
        r.get(self_ty, &path(&["Vec2"])),
        Some(Res::Type(Type::Def(TyDef::Struct(_))))
    ));
    assert_eq!(r.entries(item).len(), 0);
}

#[test]
fn a_generics_bounds_are_entries_on_the_generic_node_in_source_order() {
    let ast = ast_from_files(&["module app; trait A {} trait B {} fun f<T: A + B>() {}"]);
    let r = resolve(crate::testing::session(), &ast);
    let g = first_generic_id(&ast);
    let names: Vec<_> = r
        .entries(g)
        .iter()
        .map(|(p, _)| crate::testing::resolve(p.segments[0].text))
        .collect();
    assert_eq!(names, vec!["A", "B"]);
}

#[test]
fn a_duplicate_bound_conflicts_and_only_the_first_writing_is_recorded() {
    let ast = ast_from_files(&["module app; trait A {} fun f<T: A + A>() {}"]);
    let g = first_generic_id(&ast);
    let (r, diags) = with_diags(|| resolve(crate::testing::session(), &ast));
    assert!(
        diags.iter().any(|d| d.message.contains("duplicate bound")),
        "expected a duplicate-bound diagnostic, got {diags:?}"
    );
    assert_eq!(r.entries(g).len(), 1);
}

#[test]
fn a_block_scoped_binding_drops_at_the_closing_brace() {
    let ast = ast_from_files(&["module app; fun f() { { let x = 1; } let y = x; }"]);
    let (_, diags) = with_diags(|| resolve(crate::testing::session(), &ast));
    assert!(diags.iter().any(|d| d.message.contains("cannot find `x`")));
}

#[test]
fn a_match_arm_binding_is_scoped_to_that_arm() {
    let ast = ast_from_files(&[
        "module app; enum E { a: i32 } fun f(e: E) { match e { .a(n) => n, } let y = n; }",
    ]);
    let (_, diags) = with_diags(|| resolve(crate::testing::session(), &ast));
    assert!(diags.iter().any(|d| d.message.contains("cannot find `n`")));
}

#[test]
fn a_match_arm_binding_is_visible_in_that_arms_guard() {
    let ast = ast_from_files(&[
        "module app; enum E { a: i32 } fun f(e: E) { match e { .a(n) if n > 0 => n, _ => 0 } }",
    ]);
    let (_, diags) = with_diags(|| resolve(crate::testing::session(), &ast));
    assert!(
        non_lang_item_diags(&diags).is_empty(),
        "unexpected diagnostics: {diags:?}"
    );
}

#[test]
fn a_match_arm_guard_does_not_leak_its_own_scope() {
    let ast = ast_from_files(&[
        "module app; enum E { a: i32 } fun f(e: E) { match e { .a(n) if n > 0 => n, _ => 0, } let y = n; }",
    ]);
    let (_, diags) = with_diags(|| resolve(crate::testing::session(), &ast));
    assert!(diags.iter().any(|d| d.message.contains("cannot find `n`")));
}

#[test]
fn a_generic_is_visible_in_a_method_of_the_extend_block_that_declares_it() {
    let ast = ast_from_files(&[
        "module app; struct S {} extend<T> S { fun get(self) -> T { let x = 1; } }",
    ]);
    let (_, diags) = with_diags(|| resolve(crate::testing::session(), &ast));
    assert!(
        non_lang_item_diags(&diags).is_empty(),
        "unexpected diagnostics: {diags:?}"
    );
}

#[test]
fn an_unresolved_path_records_err_rather_than_leaving_the_entry_absent() {
    let ast = ast_from_files(&["module app; fun f(x: Nope) {}"]);
    let r = resolve(crate::testing::session(), &ast);
    let ty = param_ty_id(&ast);
    assert_eq!(r.get(ty, &path(&["Nope"])), Some(Res::Err));
}

#[test]
fn a_path_expression_resolves_to_the_local_it_names() {
    let ast = ast_from_files(&["module app; fun f() { let x = 1; let y = x; }"]);
    let r = resolve(crate::testing::session(), &ast);
    let (expr_id, pat_id) = x_use_and_binding(&ast);
    assert_eq!(
        r.get(expr_id, &path(&["x"])),
        Some(Res::Local(Local::Variable(pat_id)))
    );
}

fn access_base_of_first_let(ast: &Ast) -> &Expr {
    let f = only_function(ast);
    let block = f.block.as_ref().expect("the fixture's function has a body");
    let StmtKind::Let { init, .. } = &block.stmts[0].kind else {
        panic!("expected the first statement to be a let binding");
    };
    let ExprKind::Access { base, .. } = &init.kind else {
        panic!("expected the initializer to be a `.` access");
    };
    base
}

#[test]
fn an_access_base_falls_back_to_the_type_namespace() {
    let ast = ast_from_files(&["module app; enum Shape { unit } fun f() { let s = Shape.unit; }"]);
    let r = resolve(crate::testing::session(), &ast);
    let base = access_base_of_first_let(&ast);
    assert!(matches!(
        r.get(base.id, &path(&["Shape"])),
        Some(Res::Type(Type::Def(TyDef::Enum(_))))
    ));
}

#[test]
fn a_local_shadows_a_type_of_the_same_name_as_an_access_base() {
    let ast = ast_from_files(&[
        "module app; enum Shape { unit } fun f(Shape: i32) { let s = Shape.unit; }",
    ]);
    let r = resolve(crate::testing::session(), &ast);
    let base = access_base_of_first_let(&ast);
    assert!(matches!(
        r.get(base.id, &path(&["Shape"])),
        Some(Res::Local(Local::Param(_)))
    ));
}

#[test]
fn an_access_base_in_neither_namespace_is_reported_once() {
    let ast = ast_from_files(&["module app; fun f() { let s = Nope.unit; }"]);
    let (_, diags) = with_diags(|| resolve(crate::testing::session(), &ast));
    let reported = non_lang_item_diags(&diags);
    assert_eq!(reported.len(), 1, "{reported:?}");
    assert!(reported[0].message.contains("cannot find"), "{reported:?}");
}

#[test]
fn a_let_rhs_sees_the_outer_x_not_the_one_it_declares() {
    let ast = ast_from_files(&["module app; fun f() { let x = 1; { let x = x; } }"]);
    let r = resolve(crate::testing::session(), &ast);
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
    let (_, diags) = with_diags(|| resolve(crate::testing::session(), &ast));
    assert!(
        non_lang_item_diags(&diags).is_empty(),
        "unexpected diagnostics: {diags:?}"
    );
}

#[test]
fn self_outside_any_definition_records_err() {
    let ast = ast_from_files(&["module app; fun f() -> Self {}"]);
    let (r, diags) = with_diags(|| resolve(crate::testing::session(), &ast));
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
    let (r, diags) = with_diags(|| resolve(crate::testing::session(), &ast));
    assert!(
        diags
            .iter()
            .any(|d| d.message.contains("`dyn` requires a trait"))
    );
    let ty = param_ty_id(&ast);
    assert_eq!(r.get(ty, &path(&["S"])), Some(Res::Err));
}

#[test]
fn an_extends_unresolved_adt_path_suppresses_the_self_diagnostic() {
    let ast = ast_from_files(&["module app; extend Nope { fun f(&self) -> Self {} }"]);
    let (r, diags) = with_diags(|| resolve(crate::testing::session(), &ast));
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

#[test]
fn the_dump_is_ordered_by_span_and_contains_no_node_ids() {
    let ast = ast_from_files(&["module app; struct A {} struct B {} fun f(x: B, y: A) {}"]);
    let r = resolve(crate::testing::session(), &ast);
    let dump = emit_debug::nameres_to_string(crate::testing::session(), &ast, &r);

    let b = dump.find('B').expect("B missing from dump");
    let a = dump.find('A').expect("A missing from dump");
    assert!(b < a, "dump is not span-ordered:\n{dump}");

    assert!(
        !dump.contains("NodeId"),
        "NodeId leaked into the dump:\n{dump}"
    );
}

#[test]
fn three_entries_out_of_declaration_order_still_print_span_ordered() {
    let ast = ast_from_files(&[
        "module app; struct Third {} struct Second {} struct First {} \
         fun f(a: First, b: Second, c: Third) {}",
    ]);
    let r = resolve(crate::testing::session(), &ast);
    let dump = emit_debug::nameres_to_string(crate::testing::session(), &ast, &r);

    let first = dump.find("First").expect("First missing from dump");
    let second = dump.find("Second").expect("Second missing from dump");
    let third = dump.find("Third").expect("Third missing from dump");
    assert!(
        first < second && second < third,
        "dump is not span-ordered:\n{dump}"
    );
}

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
    let (r, diags) = with_diags(|| resolve(crate::testing::session(), &ast));
    assert!(
        non_lang_item_diags(&diags).is_empty(),
        "unexpected diagnostics: {diags:?}"
    );
    let dump = emit_debug::nameres_to_string(crate::testing::session(), &ast, &r);

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
    let r = resolve(crate::testing::session(), &ast);
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
    let r = resolve(crate::testing::session(), &ast);
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
    let r = resolve(crate::testing::session(), &ast);
    let init = first_let_init(&ast);
    assert_eq!(r.get(init.id, &path(&["Nope"])), Some(Res::Err));
}

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

#[test]
fn a_variant_record_payloads_shorthand_field_resolves_its_implicit_value() {
    let ast = ast_from_files(&["module app; enum Shape { rect: { w: i32, h: i32 } } \
         fun f() { let w = 1; let h = 2; let s = .rect { w, h }; }"]);
    let r = resolve(crate::testing::session(), &ast);
    let fields = variant_record_fields(&ast);
    assert_eq!(fields.len(), 2);
    for field in fields {
        assert!(
            field.value.is_none(),
            "expected {field:?} to be the shorthand form"
        );
        let name = crate::testing::resolve(field.name.text);
        assert!(
            matches!(
                r.get(field.id, &path(&[name])),
                Some(Res::Local(Local::Variable(_)))
            ),
            "expected the shorthand field {name:?} to resolve to a local"
        );
    }
}

#[test]
fn a_match_arms_record_payload_shorthand_binds_its_fields() {
    let ast = ast_from_files(&["module app; enum Shape { rect: { w: i32, h: i32 } } \
         fun f(s: Shape) { match s { .rect { w, h } => w, } let y = w; }"]);
    let (_, diags) = with_diags(|| resolve(crate::testing::session(), &ast));
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

#[test]
fn a_function_and_a_struct_of_the_same_name_do_not_conflict() {
    let ast = ast_from_files(&["module app; fun Point() {} struct Point {}"]);
    let (_, diags) = with_diags(|| resolve(crate::testing::session(), &ast));
    assert!(
        non_lang_item_diags(&diags).is_empty(),
        "unexpected diagnostics: {diags:?}"
    );
}

#[test]
fn a_function_and_a_submodule_of_the_same_name_do_not_conflict() {
    let ast = ast_from_files(&[
        "module app; fun helper() {}",
        "module app::helper; fun f() {}",
    ]);
    let (_, diags) = with_diags(|| resolve(crate::testing::session(), &ast));
    assert!(
        non_lang_item_diags(&diags).is_empty(),
        "unexpected diagnostics: {diags:?}"
    );
}

#[test]
fn two_unrelated_modules_may_each_declare_a_function_of_the_same_name() {
    let ast = ast_from_files(&["module a; fun helper() {}", "module b; fun helper() {}"]);
    let (_, diags) = with_diags(|| resolve(crate::testing::session(), &ast));
    assert!(
        non_lang_item_diags(&diags).is_empty(),
        "unexpected diagnostics: {diags:?}"
    );
}

#[test]
fn a_private_item_is_not_importable_from_an_unrelated_module() {
    let ast = ast_from_files(&[
        "module math; fun secret() {}",
        "module app; import math::secret; fun f() { secret(); }",
    ]);
    let (_, diags) = with_diags(|| resolve(crate::testing::session(), &ast));
    assert!(
        !non_lang_item_diags(&diags).is_empty(),
        "`secret` is declared without `public` in `math`, so importing it into the unrelated \
         module `app` should be rejected: {diags:?}"
    );
}

#[test]
fn a_glob_import_does_not_copy_a_private_name() {
    let ast = ast_from_files(&[
        "module math; fun secret() {}",
        "module app; import math::*;",
    ]);
    let table = SymbolTable::new(crate::testing::session(), &ast);
    let app = module_by_path(&ast, &table, &[crate::testing::intern("app")]).unwrap();
    assert!(
        table
            .lookup_function(app, crate::testing::intern("secret"))
            .is_none(),
        "a glob import must not leak a private function into the importing module"
    );
}

#[test]
fn a_glob_import_sees_names_a_later_glob_brings_in() {
    let ast = ast_from_files(&[
        "module a; import b::*;",
        "module b; import c::*;",
        "module c; public fun f() {}",
    ]);
    let table = SymbolTable::new(crate::testing::session(), &ast);
    let a = module_by_path(&ast, &table, &[crate::testing::intern("a")]).unwrap();
    assert!(
        table
            .lookup_function(a, crate::testing::intern("f"))
            .is_some(),
        "a glob imported through another glob should still reach `f`"
    );
}

#[test]
fn a_parent_module_cannot_see_a_childs_declaration() {
    let ast = ast_from_files(&[
        "module app; fun f() { helper(); }",
        "module app::inner; fun helper() {}",
    ]);
    let (_, diags) = with_diags(|| resolve(crate::testing::session(), &ast));
    assert!(
        non_lang_item_diags(&diags)
            .iter()
            .any(|d| d.message.contains("cannot find `helper`")),
        "expected `helper` to be unresolvable from the parent: {diags:?}"
    );
}

#[test]
fn sibling_modules_do_not_see_each_other() {
    let ast = ast_from_files(&[
        "module a; fun helper() {}",
        "module b; fun f() { helper(); }",
    ]);
    let (_, diags) = with_diags(|| resolve(crate::testing::session(), &ast));
    assert!(
        non_lang_item_diags(&diags)
            .iter()
            .any(|d| d.message.contains("cannot find `helper`")),
        "expected `helper` to be unresolvable from an unrelated module: {diags:?}"
    );
}

#[test]
fn a_name_falls_back_through_three_levels_of_ancestry() {
    let ast = ast_from_files(&[
        "module app; public fun shared() {}",
        "module app::mid; fun unused() {}",
        "module app::mid::deep; fun f() { shared(); }",
    ]);
    let (_, diags) = with_diags(|| resolve(crate::testing::session(), &ast));
    assert!(
        non_lang_item_diags(&diags).is_empty(),
        "unexpected diagnostics: {diags:?}"
    );
}

#[test]
fn a_function_used_as_a_path_qualifier_does_not_resolve() {
    let ast = ast_from_files(&["module app; fun helper() {} fun f() { helper::thing(); }"]);
    let (_, diags) = with_diags(|| resolve(crate::testing::session(), &ast));
    assert!(
        non_lang_item_diags(&diags)
            .iter()
            .any(|d| d.message.contains("cannot find")),
        "expected the qualified path to fail: {diags:?}"
    );
}

#[test]
fn an_existing_modules_missing_member_reports_once() {
    let ast = ast_from_files(&[
        "module math; public fun dot() {}",
        "module app; fun f() { math::cross(); }",
    ]);
    let (_, diags) = with_diags(|| resolve(crate::testing::session(), &ast));
    let diags = non_lang_item_diags(&diags);
    assert_eq!(diags.len(), 1, "{diags:?}");
    assert!(diags[0].message.contains("cannot find `cross`"));
}

#[test]
fn an_aliased_import_binds_under_the_alias_only() {
    let ast = ast_from_files(&[
        "module math; public fun dot() {}",
        "module app; import math::dot as scalar_product;",
    ]);
    let table = SymbolTable::new(crate::testing::session(), &ast);
    let app = module_by_path(&ast, &table, &[crate::testing::intern("app")]).unwrap();
    assert!(
        table
            .lookup_function(app, crate::testing::intern("scalar_product"))
            .is_some()
    );
    assert!(
        table
            .lookup_function(app, crate::testing::intern("dot"))
            .is_none()
    );
}

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

#[test]
fn an_imported_struct_is_usable_in_a_type_position() {
    let ast = ast_from_files(&[
        "module shapes; public struct Circle { r: i32 }",
        "module app; import shapes::Circle; fun f(c: Circle) {}",
    ]);
    let (_, diags) = with_diags(|| resolve(crate::testing::session(), &ast));
    assert!(
        non_lang_item_diags(&diags).is_empty(),
        "unexpected diagnostics: {diags:?}"
    );
}

#[test]
fn an_imported_trait_is_usable_as_a_bound() {
    let ast = ast_from_files(&[
        "module traits; public trait Show { fun show(&self); }",
        "module app; import traits::Show; fun f<T: Show>(x: T) {}",
    ]);
    let (_, diags) = with_diags(|| resolve(crate::testing::session(), &ast));
    assert!(
        non_lang_item_diags(&diags).is_empty(),
        "unexpected diagnostics: {diags:?}"
    );
}

#[test]
fn a_with_lends_binding_outlives_its_own_written_block() {
    let ast = ast_from_files(&["module app; fun f() { with x = 1 { } let y = x; }"]);
    let (_, diags) = with_diags(|| resolve(crate::testing::session(), &ast));
    assert!(
        non_lang_item_diags(&diags).is_empty(),
        "expected `x` to still be in scope after the `with`'s block: {diags:?}"
    );
}

#[test]
fn a_for_loops_pattern_binding_does_not_outlive_the_loop() {
    let ast = ast_from_files(&["module app; fun f(xs: i32) { for x in xs { } let y = x; }"]);
    let (_, diags) = with_diags(|| resolve(crate::testing::session(), &ast));
    assert!(
        non_lang_item_diags(&diags)
            .iter()
            .any(|d| d.message.contains("cannot find `x`")),
        "expected `x` to have dropped out of scope: {diags:?}"
    );
}

#[test]
fn a_while_lets_pattern_binding_does_not_outlive_the_loop() {
    let ast =
        ast_from_files(&["module app; fun f(opt: bool) { while let x = opt { } let y = x; }"]);
    let (_, diags) = with_diags(|| resolve(crate::testing::session(), &ast));
    assert!(
        non_lang_item_diags(&diags)
            .iter()
            .any(|d| d.message.contains("cannot find `x`")),
        "expected `x` to have dropped out of scope: {diags:?}"
    );
}

#[test]
fn a_closure_parameter_shadows_an_outer_local_of_the_same_name() {
    let ast =
        ast_from_files(&["module app; fun f() { let x = 1; let g = |x: i32| { x }; let y = x; }"]);
    let (_, diags) = with_diags(|| resolve(crate::testing::session(), &ast));
    assert!(
        non_lang_item_diags(&diags).is_empty(),
        "unexpected diagnostics: {diags:?}"
    );
}

#[test]
fn two_generic_parameters_of_the_same_name_conflict() {
    let ast = ast_from_files(&["module app; fun f<T, T>(x: T) {}"]);
    let (_, diags) = with_diags(|| resolve(crate::testing::session(), &ast));
    assert!(
        non_lang_item_diags(&diags)
            .iter()
            .any(|d| d.message.contains("is defined multiple times")),
        "expected a duplicate generic parameter name to be reported: {diags:?}"
    );
}

#[test]
fn a_function_may_call_itself_recursively() {
    let ast = ast_from_files(&["module app; fun fact(n: i32) -> i32 { return fact(n); }"]);
    let (_, diags) = with_diags(|| resolve(crate::testing::session(), &ast));
    assert!(
        non_lang_item_diags(&diags).is_empty(),
        "unexpected diagnostics: {diags:?}"
    );
}

#[test]
fn two_functions_may_call_each_other_regardless_of_declaration_order() {
    let ast = ast_from_files(&["module app; fun a() { b(); } fun b() { a(); }"]);
    let (_, diags) = with_diags(|| resolve(crate::testing::session(), &ast));
    assert!(
        non_lang_item_diags(&diags).is_empty(),
        "unexpected diagnostics: {diags:?}"
    );
}

#[test]
fn a_struct_may_reference_itself_through_a_field_type() {
    let ast = ast_from_files(&["module app; struct Node { next: &Node }"]);
    let (_, diags) = with_diags(|| resolve(crate::testing::session(), &ast));
    assert!(
        non_lang_item_diags(&diags).is_empty(),
        "unexpected diagnostics: {diags:?}"
    );
}

#[test]
fn two_structs_may_reference_each_other_regardless_of_declaration_order() {
    let ast = ast_from_files(&["module app; struct A { b: &B } struct B { a: &A }"]);
    let (_, diags) = with_diags(|| resolve(crate::testing::session(), &ast));
    assert!(
        non_lang_item_diags(&diags).is_empty(),
        "unexpected diagnostics: {diags:?}"
    );
}

#[test]
fn an_enum_variant_may_reference_its_own_enum_through_a_reference() {
    let ast = ast_from_files(&["module app; enum List { cons: &List, nil }"]);
    let (_, diags) = with_diags(|| resolve(crate::testing::session(), &ast));
    assert!(
        non_lang_item_diags(&diags).is_empty(),
        "unexpected diagnostics: {diags:?}"
    );
}
