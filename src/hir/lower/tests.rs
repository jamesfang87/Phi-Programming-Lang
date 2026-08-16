use super::ctx::LoweringCtx;
use super::*;
use crate::ast::interner::Interner;
use crate::ast::{Ast, BinaryOp, Ident, ModuleDecl, Mutability, NodeId, Path, UnaryOp, Visibility};
use crate::hir::ids::{DefId, HirId};
use crate::hir::{
    AccessArgs, ExprKind, Function, Local, LoopSource, Module, OwnerNode, PatKind, Payload, Res,
    StmtKind, TyKind, VariantPayload,
};
use crate::testing::{lower_src, parse_src};

// -----------------------------------------------------------------
// Running the lowering pipeline
// -----------------------------------------------------------------

fn text(ident: Ident) -> &'static str {
    Interner::resolve(ident.text)
}

// -----------------------------------------------------------------
// Typed node lookup helpers
// -----------------------------------------------------------------

fn find_value(hir: &Hir, m: &Module, name: &str) -> DefId {
    m.items
        .iter()
        .copied()
        .find(|&id| {
            matches!(hir.def(id), OwnerNode::Function(f)
                if text(f.name) == name)
        })
        .unwrap_or_else(|| panic!("no {name:?} in module's items"))
}

fn find_type(hir: &Hir, m: &Module, name: &str) -> DefId {
    m.items
        .iter()
        .copied()
        .find(|&id| match hir.def(id) {
            OwnerNode::Struct(s) => text(s.name) == name,
            OwnerNode::Enum(e) => text(e.name) == name,
            OwnerNode::Trait(t) => text(t.name) == name,
            _ => false,
        })
        .unwrap_or_else(|| panic!("no {name:?} in module's items"))
}

/// The sole top-level function in a single-item source, together with its `DefId`.
fn only_function(hir: &Hir) -> (DefId, &Function) {
    let m = hir.root();
    assert_eq!(m.items.len(), 1);
    let id = m.items[0];
    (id, hir.function(id))
}

// -----------------------------------------------------------------
// DefId pre-allocation
// -----------------------------------------------------------------

/// Every top-level definition -- struct, function, enum, trait, `extend` block -- gets a `DefId`,
/// plus the root module: six in total.
#[test]
fn every_definition_has_a_def_id_before_any_body_is_lowered() {
    let hir = lower_src("struct A {} fun f() {} enum E { x } trait T {} extend A {}");
    assert_eq!(hir.def_ids().count(), 6);
}

/// `Foo` is declared after the function that names it in a parameter position. Name resolution
/// already resolved that reference before lowering runs; what this test exercises is that
/// `Foo`'s `DefId` exists by the time lowering reaches for it to build the `hir::Path`, even
/// though `Foo`'s own item is pre-allocated after `f`'s.
#[test]
fn a_forward_reference_resolves_to_an_already_allocated_def_id() {
    let hir = lower_src("fun f(x: Foo) {} struct Foo {}");
    assert_eq!(hir.def_ids().count(), 3);
}

/// This test exercises pre-allocation directly rather than just its end result:
/// it runs the pass only as far as `prealloc_item` and checks every item already has a `DefId`
/// in `cx.def_ids`, before `lower_module` -- which builds any arena -- has run at all. Before
/// `def_ids`/`prealloc_item` existed, every one of `lower_item`, `lower_function`,
/// `lower_struct`, ... allocated its own `DefId` lazily instead, so this assertion had nothing to
/// check and could not have failed the way it now would if pre-allocation regressed.
#[test]
fn every_item_gets_a_def_id_before_lower_module_runs() {
    let unit = parse_src("fun f(x: Foo) {} struct Foo {} trait T { fun m(self) {} }");
    let ast = Ast::new(vec![unit]);
    let surface_results = crate::nameres::resolve(&ast);
    let mut cx = LoweringCtx::new(&surface_results);

    for mod_id in ast.mod_ids() {
        let parent_def = ast.parent(mod_id).map(|id| cx.def_ids[&id]);
        let def_id = cx.items.alloc(parent_def);
        cx.def_ids.insert(mod_id, def_id);
    }
    for mod_id in ast.mod_ids() {
        let module_def = cx.def_ids[&mod_id];
        for item in &ast.module(mod_id).items {
            cx.prealloc_item(module_def, item);
        }
    }

    // No arena exists yet -- `lower_module` was never called -- but every item's `NodeId`
    // already maps to a `DefId`, including the trait's method, which needed the trait's own id
    // to be allocated first.
    assert!(cx.owners.is_empty());
    let root = ast.module(ast.root_id());
    assert_eq!(root.items.len(), 3);
    for item in &root.items {
        assert!(
            cx.def_ids.contains_key(&item.id),
            "item {:?} has no pre-allocated DefId",
            item.kind
        );
    }
    let trait_item = root
        .items
        .iter()
        .find(|item| matches!(item.kind, crate::ast::ItemKind::Trait(_)))
        .expect("fixture declares a trait");
    assert_eq!(
        cx.method_defs
            .get(&trait_item.id)
            .expect("trait's methods were pre-allocated")
            .len(),
        1
    );
    // Root module + fun f + struct Foo + trait T.
    assert_eq!(cx.def_ids.len(), 4);
}

// -----------------------------------------------------------------
// Items
// -----------------------------------------------------------------

#[test]
fn function_is_declared_in_the_module() {
    let hir = lower_src("fun main() {}");
    let m = hir.root();
    let id = find_value(&hir, m, "main");
    let f = hir.function(id);
    assert_eq!(text(f.name), "main");
    assert!(matches!(f.visibility, Visibility::Private));
    assert!(f.self_param.is_none());
    assert!(f.params.is_empty());
    assert!(f.ret.is_none());
    let body = hir.block(f.block.expect("expected a body"));
    assert!(body.stmts.is_empty());
    assert!(body.expr.is_none());
}

#[test]
fn function_params_and_return_type_are_lowered() {
    let hir = lower_src("fun add(x: i32, y: i32) -> i32 { x + y }");
    let (_, f) = only_function(&hir);
    assert_eq!(f.params.len(), 2);
    let x = hir.param(f.params[0]);
    assert_eq!(text(x.name), "x");
    match &hir.ty(x.ty).kind {
        TyKind::Path { path, args } => {
            assert_eq!(text(path.segments[0]), "i32");
            assert!(args.is_empty());
        }
        other => panic!("expected a base type, got {other:?}"),
    }
    let ret_ty = hir.ty(f.ret.expect("expected a return type"));
    assert!(matches!(&ret_ty.kind, TyKind::Path { .. }));

    let body = hir.block(f.block.expect("expected a body"));
    assert!(body.stmts.is_empty());
    let tail = hir.expr(body.expr.expect("expected a tail expression"));
    assert!(matches!(
        tail.kind,
        ExprKind::Binary {
            op: BinaryOp::Add,
            ..
        }
    ));
}

/// `expr as ty` lowers its operand as an ordinary expression and its target as an ordinary type
/// annotation, exactly like a `let`'s or a parameter's.
#[test]
fn a_cast_expr_lowers_its_operand_and_target_type() {
    let hir = lower_src("fun f(x: i32) -> i64 { x as i64 }");
    let (_, f) = only_function(&hir);
    let body = hir.block(f.block.expect("expected a body"));
    let tail = hir.expr(body.expr.expect("expected a tail expression"));

    let ExprKind::Cast {
        expr: operand,
        ty: cast_ty,
    } = tail.kind
    else {
        panic!("expected a cast expr, got {:?}", tail.kind);
    };
    assert!(matches!(hir.expr(operand).kind, ExprKind::Path(_)));
    match &hir.ty(cast_ty).kind {
        TyKind::Path { path, args } => {
            assert_eq!(text(path.segments[0]), "i64");
            assert!(args.is_empty());
        }
        other => panic!("expected a base type, got {other:?}"),
    }
}

#[test]
fn struct_has_its_fields_lowered() {
    let hir = lower_src("struct Point { x: i32, y: i32 }");
    let m = hir.root();
    let id = find_type(&hir, m, "Point");
    let s = hir.struct_(id);
    assert_eq!(text(s.name), "Point");
    assert_eq!(s.fields.len(), 2);
    let x = hir.field(s.fields[0]);
    assert_eq!(text(x.name), "x");
}

#[test]
fn enum_variants_are_lowered() {
    let hir = lower_src("enum Shape { Circle: f64, Rectangle: { w: f64, h: f64 }, Point }");
    let m = hir.root();
    let id = find_type(&hir, m, "Shape");
    let e = hir.enum_(id);
    assert_eq!(e.variants.len(), 3);

    let circle = hir.variant(e.variants[0]);
    assert_eq!(text(circle.name), "Circle");
    assert!(matches!(circle.payload, VariantPayload::Type(_)));

    let rect = hir.variant(e.variants[1]);
    match &rect.payload {
        VariantPayload::Record(fields) => assert_eq!(fields.len(), 2),
        other => panic!("expected a record payload, got {other:?}"),
    }

    let point = hir.variant(e.variants[2]);
    assert!(matches!(point.payload, VariantPayload::Unit));
}

#[test]
fn trait_functions_are_lowered_as_independent_owners() {
    let hir = lower_src("trait Shape { fun area(&self) -> f64; }");
    let m = hir.root();
    let id = find_type(&hir, m, "Shape");
    let t = hir.trait_(id);
    assert_eq!(t.functions.len(), 1);
    let method_id = t.functions[0];
    assert_ne!(method_id, id);
    let f = hir.function(method_id);
    assert_eq!(text(f.name), "area");
    assert!(f.self_param.is_some());
    // The trait's own function declares no body.
    assert!(f.block.is_none());
    // A trait method isn't itself a top-level item of the module.
    assert!(!m.items.contains(&method_id));
}

#[test]
fn extend_methods_and_generics_are_lowered() {
    let hir = lower_src("extend<T> Box<T> with Container<T> { fun get(&self) {} }");
    let m = hir.root();
    // `extend` blocks aren't named, so they're only reachable through the module's item list.
    assert_eq!(m.items.len(), 1);
    let id = m.items[0];
    let e = hir.extend(id);
    assert_eq!(e.extend_generics.len(), 1);
    assert_eq!(e.adt_generics.len(), 1);
    assert_eq!(e.trait_generics.len(), 1);
    assert_eq!(text(e.adt_path.segments[0]), "Box");
    assert_eq!(
        text(
            e.trait_path
                .as_ref()
                .expect("expected a trait path")
                .segments[0]
        ),
        "Container"
    );
    assert_eq!(e.methods.len(), 1);
    let method = hir.function(e.methods[0]);
    assert_eq!(text(method.name), "get");
}

/// Regression test: a trait's generics must be lowered before its functions, since a function's
/// signature or body can name them (`fun get(self) -> T` inside `trait C<T>`), and path lowering
/// needs the generic's `HirId` to already exist when it resolves such a reference.
///
/// This can't observe *when* the generic node was built from outside -- both the old, buggy
/// order and the fixed one produce the same final `Trait`/`Function` shape, since a function is
/// lowered into its own separate arena from the trait's. The actual regression guard is the
/// `debug_assert!` in `LoweringCtx::lower_trait`, right before each `self.lower_function(id, f)`
/// call, checking that the trait's own `DefId` is already in `generics_ready`. That assertion was
/// confirmed non-vacuous by hand: temporarily reverting `lower_trait` to the pre-fix order (lower
/// every function, *then* lower the trait's own generics) while keeping the assertion in place
/// made it panic, failing this test, `trait_functions_are_lowered_as_independent_owners`, and
/// `a_methods_parent_is_its_trait_or_extend_block` -- the three tests in this file that exercise a
/// trait's functions. There is no standing test that re-exercises the pre-fix order on every
/// run (a debug-assert regression test would need a seam into `lower_trait`'s ordering that has
/// no other reason to exist); the `debug_assert!` itself is the permanent guard. This test is the
/// weaker, black-box check the task brief asks for regardless: that both the generics and the
/// functions came out right.
#[test]
fn a_traits_generics_are_lowered_before_its_functions() {
    let hir = lower_src("trait C<T> { fun get(self) -> T; }");
    let m = hir.root();
    let id = find_type(&hir, m, "C");
    let t = hir.trait_(id);
    assert_eq!(t.generics.len(), 1);
    hir.generic(t.generics[0]);
    assert_eq!(t.functions.len(), 1);
    let f = hir.function(t.functions[0]);
    assert_eq!(text(f.name), "get");
}

/// Same regression, for an `extend` block's own (`extend<T>`) generics against its methods.
#[test]
fn an_extend_blocks_generics_are_lowered_before_its_methods() {
    let hir = lower_src("struct S {} extend<T> S { fun get(self) -> T {} }");
    let extend_id = crate::testing::first_extend(&hir);
    let e = hir.extend(extend_id);
    assert_eq!(e.extend_generics.len(), 1);
    hir.generic(e.extend_generics[0]);
    assert_eq!(e.methods.len(), 1);
    let method = hir.function(e.methods[0]);
    assert_eq!(text(method.name), "get");
}

#[test]
fn generic_params_carry_their_bounds() {
    let hir = lower_src("struct Wrapper<T: Clone> { value: T }");
    let m = hir.root();
    let id = find_type(&hir, m, "Wrapper");
    let s = hir.struct_(id);
    assert_eq!(s.generics.len(), 1);
    let g = hir.generic(s.generics[0]);
    assert_eq!(text(g.name), "T");
    assert_eq!(g.bounds.len(), 1);
    assert_eq!(text(g.bounds[0].segments[0]), "Clone");
}

#[test]
fn import_glob_and_alias_are_lowered_into_the_module() {
    let hir = lower_src("import math::vector as mv; import math::*;");
    let m = hir.root();
    assert_eq!(m.imports.len(), 2);
    let aliased = hir.import(m.imports[0]);
    assert!(!aliased.glob);
    assert_eq!(text(aliased.alias.expect("expected an alias")), "mv");

    let glob = hir.import(m.imports[1]);
    assert!(glob.glob);
    assert!(glob.alias.is_none());
}

#[test]
fn nested_module_declaration_synthesizes_ancestor_modules() {
    // The parser does not wire a file's `module` header into `ParsedSrcFile::module`,
    // so this attaches the decl by hand to reach the module tree `Ast::new` builds from it.
    let mut unit = parse_src("fun helper() {}");
    let path_span = unit.span;
    unit.module = Some(ModuleDecl {
        id: NodeId::next(),
        path: Path {
            segments: vec![
                Ident {
                    text: Interner::intern("math"),
                    span: path_span,
                },
                Ident {
                    text: Interner::intern("vector"),
                    span: path_span,
                },
            ],
            span: path_span,
        },
        span: path_span,
    });

    let ast = Ast::new(vec![unit]);
    let surface_results = crate::nameres::resolve(&ast);
    let hir = lower_program(&ast, &surface_results);
    let root = hir.root();
    // The root's only item is the synthesized `math`, which in turn holds `math::vector`.
    assert_eq!(root.items.len(), 1);

    // `lower.rs`'s test module is a descendant of `crate::hir`, so `Hir`'s otherwise-private
    // fields (see `LoweringCtx::finish`'s comment) are visible here: scan every allocated owner
    // directly for the module whose path ends in the given last segment.
    let find_module = |hir: &Hir, last_segment: &str| -> DefId {
        (0..hir.arenas.len())
            .map(DefId::from_usize)
            .find(|&id| {
                id != hir.root_module
                    && matches!(hir.def(id), OwnerNode::Module(m)
                        if text(*m.path.segments.last().unwrap()) == last_segment)
            })
            .unwrap_or_else(|| panic!("no synthesized module ending in {last_segment:?}"))
    };

    let math_id = find_module(&hir, "math");
    let math_module = match hir.def(math_id) {
        OwnerNode::Module(m) => m,
        other => panic!("expected a module owner, got {other:?}"),
    };
    assert_eq!(root.items[0], math_id);
    assert_eq!(math_module.items.len(), 1);

    let vector_id = find_module(&hir, "vector");
    let vector_module = match hir.def(vector_id) {
        OwnerNode::Module(m) => m,
        other => panic!("expected a module owner, got {other:?}"),
    };
    assert_eq!(vector_module.items.len(), 1);
    let helper_id = vector_module.items[0];
    assert_eq!(text(hir.function(helper_id).name), "helper");

    // Synthesized ancestors are parented like declared ones, so a def in the innermost
    // module is walkable all the way back to the root.
    assert_eq!(hir.parent(helper_id), Some(vector_id));
    assert_eq!(hir.parent(vector_id), Some(math_id));
    assert_eq!(hir.parent(math_id), Some(hir.root_id()));
    assert_eq!(hir.parent(hir.root_id()), None);
    assert_eq!(hir.module_of(helper_id), vector_id);
    assert_eq!(hir.module_of(vector_id), vector_id);
}

// -----------------------------------------------------------------
// Parents
// -----------------------------------------------------------------

#[test]
fn a_free_items_parent_is_its_module() {
    let hir = lower_src("fun f() {} struct S { x: i32 }");
    let root = hir.root();
    let f_id = find_value(&hir, root, "f");
    let s_id = find_type(&hir, root, "S");

    assert_eq!(hir.parent(f_id), Some(hir.root_id()));
    assert_eq!(hir.parent(s_id), Some(hir.root_id()));
    assert_eq!(hir.module_of(f_id), hir.root_id());
}

#[test]
fn a_methods_parent_is_its_trait_or_extend_block() {
    let hir = lower_src(
        "trait T { fun t(self) {} }
         struct S {}
         extend S with T { fun m(self) {} }",
    );
    let root = hir.root();
    let trait_id = find_type(&hir, root, "T");
    let extend_id = crate::testing::first_extend(&hir);

    let t_id = hir.trait_(trait_id).functions[0];
    let m_id = hir.extend(extend_id).methods[0];

    // A method is parented to the item that declares it, not to the module -- this allows
    // `Self` to be inferred from the method's id alone.
    assert_eq!(hir.parent(t_id), Some(trait_id));
    assert_eq!(hir.parent(m_id), Some(extend_id));
    // ...while `module_of` still skips past it to the enclosing module.
    assert_eq!(hir.module_of(t_id), hir.root_id());
    assert_eq!(hir.module_of(m_id), hir.root_id());
}

#[test]
fn a_closures_parent_is_the_owner_it_appears_in() {
    let hir = lower_src("fun f() { let g = || 1; }");
    let (f_id, f) = only_function(&hir);
    let body = hir.block(f.block.unwrap());
    let StmtKind::Let { init, .. } = &hir.stmt(body.stmts[0]).kind else {
        panic!("expected a let statement")
    };
    let ExprKind::Closure(closure_id) = &hir.expr(*init).kind else {
        panic!("expected a closure expr")
    };

    assert_eq!(hir.parent(*closure_id), Some(f_id));
    assert_eq!(hir.module_of(*closure_id), hir.root_id());
}

// -----------------------------------------------------------------
// Types
// -----------------------------------------------------------------

#[test]
fn lowers_compound_types() {
    let hir = lower_src(
        "fun f(a: &mut i32, b: any Draw, c: (i32, bool), d: [i32; 3], e: fun(i32) -> i32) {}",
    );
    let (_, f) = only_function(&hir);
    assert_eq!(f.params.len(), 5);

    let a = hir.ty(hir.param(f.params[0]).ty);
    match &a.kind {
        TyKind::Ref { mutability, .. } => assert_eq!(*mutability, Mutability::Mutable),
        other => panic!("expected a ref type, got {other:?}"),
    }

    let b = hir.ty(hir.param(f.params[1]).ty);
    assert!(matches!(&b.kind, TyKind::Any(_)));

    let c = hir.ty(hir.param(f.params[2]).ty);
    match &c.kind {
        TyKind::Tuple(elems) => assert_eq!(elems.len(), 2),
        other => panic!("expected a tuple type, got {other:?}"),
    }

    let d = hir.ty(hir.param(f.params[3]).ty);
    match &d.kind {
        TyKind::Array { len, .. } => assert!(len.is_some()),
        other => panic!("expected an array type, got {other:?}"),
    }

    let e = hir.ty(hir.param(f.params[4]).ty);
    match &e.kind {
        TyKind::Function { params, ret } => {
            assert_eq!(params.len(), 1);
            assert!(ret.is_some());
        }
        other => panic!("expected a function type, got {other:?}"),
    }
}

// -----------------------------------------------------------------
// Expressions
// -----------------------------------------------------------------

#[test]
fn lowers_ctor_tuple_and_range_exprs() {
    let hir =
        lower_src("fun f() { let p = Point { x: 1, y: 2 }; let t = (1, 2, 3); let r = 0..5; }");
    let (_, f) = only_function(&hir);
    let body = hir.block(f.block.unwrap());
    assert_eq!(body.stmts.len(), 3);

    let StmtKind::Let { init: p_init, .. } = &hir.stmt(body.stmts[0]).kind else {
        panic!("expected a let statement")
    };
    match &hir.expr(*p_init).kind {
        ExprKind::Ctor { path, payload } => {
            let path = path.as_ref().expect("`Point { .. }` specifies its type");
            assert_eq!(text(path.segments[0]), "Point");
            assert_eq!(payload.len(), 2);
            assert_eq!(text(payload[0].name), "x");
        }
        other => panic!("expected a ctor expr, got {other:?}"),
    }

    let StmtKind::Let { init: t_init, .. } = &hir.stmt(body.stmts[1]).kind else {
        panic!("expected a let statement")
    };
    match &hir.expr(*t_init).kind {
        ExprKind::Tuple(elems) => assert_eq!(elems.len(), 3),
        other => panic!("expected a tuple expr, got {other:?}"),
    }

    let StmtKind::Let { init: r_init, .. } = &hir.stmt(body.stmts[2]).kind else {
        panic!("expected a let statement")
    };
    match &hir.expr(*r_init).kind {
        ExprKind::Range {
            lo, hi, inclusive, ..
        } => {
            assert!(lo.is_some());
            assert!(hi.is_some());
            assert!(!inclusive);
        }
        other => panic!("expected a range expr, got {other:?}"),
    }
}

#[test]
fn lowers_access_and_index_exprs() {
    // `a.b` and `.c(1)` are the same node kind -- which is a field and which is a method call
    // isn't known until typeck -- so they differ only in their `AccessArgs`.
    let hir = lower_src("fun f() { a.b.c(1)[0] }");
    let (_, f) = only_function(&hir);
    let body = hir.block(f.block.unwrap());
    let tail = hir.expr(body.expr.unwrap());
    match &tail.kind {
        ExprKind::Index { base, .. } => match &hir.expr(*base).kind {
            ExprKind::Access { base, member, args } => {
                assert_eq!(text(*member), "c");
                assert!(matches!(args, AccessArgs::Call(args) if args.len() == 1));
                assert!(matches!(
                    &hir.expr(*base).kind,
                    ExprKind::Access {
                        args: AccessArgs::None,
                        ..
                    }
                ));
            }
            other => panic!("expected an access expr, got {other:?}"),
        },
        other => panic!("expected an index expr, got {other:?}"),
    }
}

#[test]
fn lowers_if_and_match_exprs() {
    let hir = lower_src("fun f() { if x { 1 } else { 2 }; match x { .circle(r) => 1, _ => 0 } }");
    let (_, f) = only_function(&hir);
    let body = hir.block(f.block.unwrap());
    assert_eq!(body.stmts.len(), 1);

    let StmtKind::Expr(if_id) = hir.stmt(body.stmts[0]).kind else {
        panic!("expected an expr statement")
    };
    match &hir.expr(if_id).kind {
        ExprKind::If { else_block, .. } => assert!(else_block.is_some()),
        other => panic!("expected an if expr, got {other:?}"),
    }

    let tail = hir.expr(body.expr.unwrap());
    match &tail.kind {
        ExprKind::Match { arms, .. } => {
            assert_eq!(arms.len(), 2);
            let first = hir.arm(arms[0]);
            match &hir.pat(first.pat).kind {
                PatKind::Variant { variant, payload } => {
                    assert_eq!(text(*variant), "circle");
                    let Payload::Single(inner) = payload else {
                        panic!("expected a single payload, got {payload:?}")
                    };
                    assert!(matches!(hir.pat(*inner).kind, PatKind::Binding { .. }));
                }
                other => panic!("expected a variant pattern, got {other:?}"),
            }
            let second = hir.arm(arms[1]);
            assert!(matches!(hir.pat(second.pat).kind, PatKind::Wildcard));
        }
        other => panic!("expected a match expr, got {other:?}"),
    }
}

// -----------------------------------------------------------------
// Variants
// -----------------------------------------------------------------

/// The `let` initializer of the sole statement in `src`'s only function.
fn only_init(hir: &Hir) -> HirId {
    let (_, f) = only_function(hir);
    let body = hir.block(f.block.unwrap());
    let StmtKind::Let { init, .. } = &hir.stmt(body.stmts[0]).kind else {
        panic!("expected a let statement")
    };
    *init
}

#[test]
fn payload_less_variant_lowers_with_no_payload() {
    let hir = lower_src("fun f() { let x = .none; }");
    let init = only_init(&hir);
    match &hir.expr(init).kind {
        ExprKind::Variant { variant, payload } => {
            assert_eq!(text(*variant), "none");
            assert!(matches!(payload, Payload::None));
        }
        other => panic!("expected a variant expr, got {other:?}"),
    }
}

/// A payload is always exactly one value, so a tuple payload lowers to a single
/// `ExprKind::Tuple` in the payload slot rather than to several arguments.
#[test]
fn tuple_payload_lowers_as_one_value() {
    let hir = lower_src("fun f() { let x = .parallelogram((1.0, 2.0)); }");
    let init = only_init(&hir);
    match &hir.expr(init).kind {
        ExprKind::Variant { variant, payload } => {
            assert_eq!(text(*variant), "parallelogram");
            let Payload::Single(inner) = payload else {
                panic!("expected a single payload, got {payload:?}")
            };
            match &hir.expr(*inner).kind {
                ExprKind::Tuple(elems) => assert_eq!(elems.len(), 2),
                other => panic!("expected a tuple expr, got {other:?}"),
            }
        }
        other => panic!("expected a variant expr, got {other:?}"),
    }
}

#[test]
fn record_payload_keeps_its_field_names() {
    let hir = lower_src("fun f() { let x = .square { l: 4.0 }; }");
    let init = only_init(&hir);
    match &hir.expr(init).kind {
        ExprKind::Variant { variant, payload } => {
            assert_eq!(text(*variant), "square");
            let Payload::Record(fields) = payload else {
                panic!("expected a record payload, got {payload:?}")
            };
            assert_eq!(fields.len(), 1);
            assert_eq!(text(fields[0].name), "l");
            assert!(matches!(
                hir.expr(fields[0].value).kind,
                ExprKind::Literal(_)
            ));
        }
        other => panic!("expected a variant expr, got {other:?}"),
    }
}

/// `{ l }` is shorthand for `{ l: l }`, so lowering must leave a real expression behind it.
#[test]
fn record_payload_field_shorthand_is_desugared() {
    let hir = lower_src("fun f() { let x = .square { l }; }");
    let init = only_init(&hir);
    match &hir.expr(init).kind {
        ExprKind::Variant { payload, .. } => {
            let Payload::Record(fields) = payload else {
                panic!("expected a record payload, got {payload:?}")
            };
            assert_eq!(text(fields[0].name), "l");
            match &hir.expr(fields[0].value).kind {
                ExprKind::Path(path) => assert_eq!(text(path.segments[0]), "l"),
                other => panic!("expected the shorthand to become a path, got {other:?}"),
            }
        }
        other => panic!("expected a variant expr, got {other:?}"),
    }
}

/// The same shorthand on the pattern side becomes a real binding pattern.
#[test]
fn record_pattern_field_shorthand_is_desugared() {
    let hir = lower_src("fun f() { match x { .square { l } => l, _ => 0 } }");
    let (_, f) = only_function(&hir);
    let body = hir.block(f.block.unwrap());
    let ExprKind::Match { arms, .. } = &hir.expr(body.expr.unwrap()).kind else {
        panic!("expected a match expr")
    };
    match &hir.pat(hir.arm(arms[0]).pat).kind {
        PatKind::Variant { variant, payload } => {
            assert_eq!(text(*variant), "square");
            let Payload::Record(fields) = payload else {
                panic!("expected a record payload, got {payload:?}")
            };
            assert_eq!(text(fields[0].name), "l");
            match &hir.pat(fields[0].value).kind {
                PatKind::Binding { name, .. } => assert_eq!(text(*name), "l"),
                other => panic!("expected the shorthand to become a binding, got {other:?}"),
            }
        }
        other => panic!("expected a variant pattern, got {other:?}"),
    }
}

/// The pattern-side shorthand's synthesized binding has no `ast::Pat` of its own, so AST-level
/// resolution keys it under the `PayloadField`'s `NodeId` instead (see
/// `Resolver::visit_record_pat_fields` in `src/nameres/resolver.rs`). This checks the
/// two sides agree end to end: a name shorthand-bound in the pattern actually resolves, inside
/// the arm's own body, to the binding the shorthand introduced -- not `Res::Err`, and not a
/// lowering panic.
#[test]
fn record_pattern_shorthand_binds_reachable_in_the_arm_body() {
    let hir = lower_src("fun f() { match x { .rect { w, h } => w, _ => 0 } }");
    let (_, f) = only_function(&hir);
    let body = hir.block(f.block.unwrap());
    let ExprKind::Match { arms, .. } = &hir.expr(body.expr.unwrap()).kind else {
        panic!("expected a match expr")
    };
    let matched_arm = hir.arm(arms[0]);

    let PatKind::Variant { payload, .. } = &hir.pat(matched_arm.pat).kind else {
        panic!("expected a variant pattern")
    };
    let Payload::Record(fields) = payload else {
        panic!("expected a record payload, got {payload:?}")
    };
    let w_binding = fields
        .iter()
        .find(|f| text(f.name) == "w")
        .expect("fixture shorthand-binds `w`")
        .value;

    let arm_body = hir.block(matched_arm.block);
    let tail = arm_body.expr.expect("arm body has a tail expression");
    match &hir.expr(tail).kind {
        ExprKind::Path(path) => {
            assert_eq!(text(path.segments[0]), "w");
            assert_eq!(
                path.res,
                Res::Local(Local::Variable(w_binding)),
                "the arm body's `w` should resolve to the shorthand's own binding, not Res::Err"
            );
        }
        other => panic!("expected a path expr, got {other:?}"),
    }
}

/// Symmetric to the pattern-side test above: a record *expression* shorthand field's implicit
/// value is keyed the same way (`PayloadField::id`, not an `Expr`'s), so this checks a name in
/// scope resolves through it rather than landing on `Res::Err`.
#[test]
fn record_expr_shorthand_resolves_the_name_it_names() {
    let hir = lower_src("fun f(w: i32) { let x = .square { w }; }");
    let (_, f) = only_function(&hir);
    let w_param = f.params[0];

    let body = hir.block(f.block.unwrap());
    let StmtKind::Let { init, .. } = &hir.stmt(body.stmts[0]).kind else {
        panic!("expected a let statement")
    };
    match &hir.expr(*init).kind {
        ExprKind::Variant { payload, .. } => {
            let Payload::Record(fields) = payload else {
                panic!("expected a record payload, got {payload:?}")
            };
            match &hir.expr(fields[0].value).kind {
                ExprKind::Path(path) => {
                    assert_eq!(text(path.segments[0]), "w");
                    assert_eq!(
                        path.res,
                        Res::Local(Local::Param(w_param)),
                        "the shorthand's implicit value should resolve to the `w` parameter, \
                         not Res::Err"
                    );
                }
                other => panic!("expected the shorthand to become a path, got {other:?}"),
            }
        }
        other => panic!("expected a variant expr, got {other:?}"),
    }
}

#[test]
fn elided_struct_literal_names_no_type() {
    let hir = lower_src("fun f() { let x = .{ l: 4.0, w: 6.0 }; }");
    let init = only_init(&hir);
    match &hir.expr(init).kind {
        ExprKind::Ctor { path, payload } => {
            assert!(path.is_none(), "`.{{ .. }}` names no type");
            assert_eq!(payload.len(), 2);
            assert_eq!(text(payload[0].name), "l");
        }
        other => panic!("expected a ctor expr, got {other:?}"),
    }
}

#[test]
fn closure_is_lowered_as_its_own_owner() {
    let hir = lower_src("fun f() { let add = |x: i32, y: i32| -> i32 { x + y }; }");
    let (id, f) = only_function(&hir);
    let body = hir.block(f.block.unwrap());
    let StmtKind::Let { init, .. } = &hir.stmt(body.stmts[0]).kind else {
        panic!("expected a let statement")
    };
    let closure_id = match &hir.expr(*init).kind {
        ExprKind::Closure(closure_id) => *closure_id,
        other => panic!("expected a closure expr, got {other:?}"),
    };
    assert_ne!(closure_id, id);
    let c = hir.closure(closure_id);
    assert_eq!(c.params.len(), 2);
    assert!(c.ret.is_some());
    // A closure owns a block directly. The body here was already written as `{ x + y }`, so it
    // lowers to that block without acquiring a redundant wrapper, and the addition is its tail.
    let closure_block = hir.block(c.block);
    assert!(matches!(
        hir.expr(closure_block.expr.unwrap()).kind,
        ExprKind::Binary { .. }
    ));
}

#[test]
fn block_tail_expression_is_not_a_statement() {
    let hir = lower_src("fun f() { let x = 1; x }");
    let (_, f) = only_function(&hir);
    let body = hir.block(f.block.unwrap());
    assert_eq!(body.stmts.len(), 1);
    assert!(matches!(hir.stmt(body.stmts[0]).kind, StmtKind::Let { .. }));
    assert!(matches!(
        hir.expr(body.expr.unwrap()).kind,
        ExprKind::Path(_)
    ));
}

/// The trailing `;` is what discards a block's value, so the same final expression is the
/// block's tail without one and an ordinary statement with one.
#[test]
fn a_trailing_semicolon_discards_the_block_value() {
    let hir = lower_src("fun f() { g() }");
    let (_, f) = only_function(&hir);
    let body = hir.block(f.block.unwrap());
    assert!(body.stmts.is_empty());
    assert!(body.expr.is_some());

    let hir = lower_src("fun f() { g(); }");
    let (_, f) = only_function(&hir);
    let body = hir.block(f.block.unwrap());
    assert_eq!(body.stmts.len(), 1);
    assert!(body.expr.is_none());
}

/// The same holds for a block-bodied expression, which may omit its `;` but doesn't have to --
/// so both spellings remain distinguishable in the last position.
#[test]
fn a_block_bodied_expression_is_a_tail_only_without_a_semicolon() {
    let hir = lower_src("fun f() { if c { 1 } else { 2 } }");
    let (_, f) = only_function(&hir);
    let body = hir.block(f.block.unwrap());
    assert!(body.stmts.is_empty());
    assert!(matches!(
        hir.expr(body.expr.unwrap()).kind,
        ExprKind::If { .. }
    ));

    let hir = lower_src("fun f() { if c { 1 } else { 2 }; }");
    let (_, f) = only_function(&hir);
    let body = hir.block(f.block.unwrap());
    assert_eq!(body.stmts.len(), 1);
    assert!(body.expr.is_none());
}

// -----------------------------------------------------------------
// Statements
// -----------------------------------------------------------------

#[test]
fn lowers_break_continue_return_defer_stmts() {
    let hir = lower_src("fun f() { while true { break; continue; } return 1; defer cleanup(); }");
    let (_, f) = only_function(&hir);
    let body = hir.block(f.block.unwrap());
    assert_eq!(body.stmts.len(), 3);
    assert!(matches!(
        hir.stmt(body.stmts[1]).kind,
        StmtKind::Return(Some(_))
    ));
    assert!(matches!(hir.stmt(body.stmts[2]).kind, StmtKind::Defer(_)));
}

#[test]
fn lowers_with_stmt_lends() {
    let hir = lower_src("fun f() { with x = &a, y = &mut b { foo(); } }");
    let (_, f) = only_function(&hir);
    let body = hir.block(f.block.unwrap());
    match &hir.stmt(body.stmts[0]).kind {
        StmtKind::With {
            lends,
            block: with_block,
        } => {
            assert_eq!(lends.len(), 2);
            assert!(matches!(
                hir.expr(lends[0].init).kind,
                ExprKind::Borrow {
                    mutability: Mutability::Immutable,
                    ..
                }
            ));
            assert!(matches!(
                hir.expr(lends[1].init).kind,
                ExprKind::Borrow {
                    mutability: Mutability::Mutable,
                    ..
                }
            ));
            // `foo();` was written with a `;`, so it stays a statement and the block has no
            // value (see `lower_block`).
            let with_body = hir.block(*with_block);
            assert_eq!(with_body.stmts.len(), 1);
            assert!(with_body.expr.is_none());
        }
        other => panic!("expected a with statement, got {other:?}"),
    }
}

// -----------------------------------------------------------------
// Desugaring
// -----------------------------------------------------------------

#[test]
fn while_loop_desugars_to_loop_with_negated_guard() {
    // `while cond { body }` -> `loop { if !cond { break; } body... }` (see `lower_while`).
    let hir = lower_src("fun f() { while x < 5 { foo(); } }");
    let (_, f) = only_function(&hir);
    let body = hir.block(f.block.unwrap());
    let StmtKind::Expr(loop_expr_id) = hir.stmt(body.stmts[0]).kind else {
        panic!("expected an expr statement wrapping the loop")
    };
    let (source, loop_body_id) = match &hir.expr(loop_expr_id).kind {
        ExprKind::Loop { source, block } => (source, *block),
        other => panic!("expected a loop expr, got {other:?}"),
    };
    assert!(matches!(source, LoopSource::While));

    let loop_body = hir.block(loop_body_id);
    // Guard statement, plus the original body's one statement.
    assert_eq!(loop_body.stmts.len(), 2);

    let StmtKind::Expr(guard_id) = hir.stmt(loop_body.stmts[0]).kind else {
        panic!("expected the guard to be an expr statement")
    };
    match &hir.expr(guard_id).kind {
        ExprKind::If {
            cond, then_block, ..
        } => {
            assert!(matches!(
                hir.expr(*cond).kind,
                ExprKind::Unary {
                    op: UnaryOp::Not,
                    ..
                }
            ));
            let then_block = hir.block(*then_block);
            assert_eq!(then_block.stmts.len(), 1);
            assert!(matches!(
                hir.stmt(then_block.stmts[0]).kind,
                StmtKind::Break
            ));
        }
        other => panic!("expected the guard to be an if expr, got {other:?}"),
    }

    assert!(matches!(
        hir.stmt(loop_body.stmts[1]).kind,
        StmtKind::Expr(_)
    ));
}

#[test]
fn if_let_desugars_to_a_match() {
    // `if let pat = e { a } else { b }` -> `match e { pat => { a }, _ => { b } }`.
    let hir = lower_src("fun f() -> i32 { if let .some(x) = o { x } else { 0 } }");
    let (_, f) = only_function(&hir);
    let body = hir.block(f.block.unwrap());
    match &hir.expr(body.expr.unwrap()).kind {
        ExprKind::Match { arms, .. } => {
            assert_eq!(arms.len(), 2);
            let first = hir.arm(arms[0]);
            assert!(matches!(hir.pat(first.pat).kind, PatKind::Variant { .. }));
            let second = hir.arm(arms[1]);
            assert!(matches!(hir.pat(second.pat).kind, PatKind::Wildcard));
        }
        other => panic!("expected a match expr, got {other:?}"),
    }
}

/// A `match` has to be exhaustive even when the source `if let` had no `else`, so the wildcard
/// Every HIR construct that owns executable code owns a `Block`, so a match arm written with a
/// bare expression gets a block whose tail value is that expression.
#[test]
fn an_expression_bodied_arm_is_wrapped_in_a_block() {
    let hir = lower_src("fun f() { match o { .some(x) => 1, _ => 2 } }");
    let (_, f) = only_function(&hir);
    let body = hir.block(f.block.unwrap());
    let ExprKind::Match { arms, .. } = &hir.expr(body.expr.unwrap()).kind else {
        panic!("expected a match expr")
    };

    let first = hir.arm(arms[0]);
    let arm_block = hir.block(first.block);
    assert!(
        arm_block.stmts.is_empty(),
        "a wrapped expression body has no statements"
    );
    assert!(matches!(
        hir.expr(arm_block.expr.expect("the expression is the tail"))
            .kind,
        ExprKind::Literal(_)
    ));
}

/// The same wrapping applies to a closure written with a bare expression body.
#[test]
fn an_expression_bodied_closure_is_wrapped_in_a_block() {
    let hir = lower_src("fun f() { let g = |x: i32| -> i32 x; }");
    let (_, f) = only_function(&hir);
    let body = hir.block(f.block.unwrap());
    let StmtKind::Let { init, .. } = &hir.stmt(body.stmts[0]).kind else {
        panic!("expected a let statement")
    };
    let ExprKind::Closure(closure_id) = &hir.expr(*init).kind else {
        panic!("expected a closure expr")
    };

    let c = hir.closure(*closure_id);
    let closure_block = hir.block(c.block);
    assert!(closure_block.stmts.is_empty());
    assert!(matches!(
        hir.expr(closure_block.expr.expect("the expression is the tail"))
            .kind,
        ExprKind::Path(_)
    ));
}

/// An `else if` lowers to `else { if .. }`, so both of an `If`'s branches are blocks no matter
/// how long the chain is, instead of the `else` alternating between an `If` and a `Block`.
#[test]
fn else_if_lowers_to_a_block_holding_the_nested_if() {
    let hir = lower_src("fun f() { if a { 1 } else if b { 2 } else { 3 } }");
    let (_, f) = only_function(&hir);
    let body = hir.block(f.block.unwrap());

    let ExprKind::If { else_block, .. } = &hir.expr(body.expr.unwrap()).kind else {
        panic!("expected an if expr")
    };
    let outer_else = hir.block(else_block.expect("the chain has an else"));

    // The `else if` is the wrapping block's tail value, not a statement.
    assert!(outer_else.stmts.is_empty());
    let ExprKind::If { else_block, .. } = &hir
        .expr(outer_else.expr.expect("the nested if is the tail"))
        .kind
    else {
        panic!("expected the else to hold a nested if expr")
    };

    // The final `else { 3 }` was already a block, so it is not wrapped a second time.
    let inner_else = hir.block(else_block.expect("the nested if has an else"));
    assert!(matches!(
        hir.expr(inner_else.expr.expect("`3` is the tail")).kind,
        ExprKind::Literal(_)
    ));
}

/// arm is always there -- yielding an empty block, the same value an `else`-less `if` produces.
#[test]
fn if_let_without_else_still_gets_a_wildcard_arm() {
    let hir = lower_src("fun f() { if let .some(x) = o { foo(x); } }");
    let (_, f) = only_function(&hir);
    let body = hir.block(f.block.unwrap());
    match &hir.expr(body.expr.unwrap()).kind {
        ExprKind::Match { arms, .. } => {
            assert_eq!(arms.len(), 2);
            let second = hir.arm(arms[1]);
            assert!(matches!(hir.pat(second.pat).kind, PatKind::Wildcard));
            let b = hir.block(second.block);
            assert!(b.stmts.is_empty() && b.expr.is_none());
        }
        other => panic!("expected a match expr, got {other:?}"),
    }
}

#[test]
fn while_let_desugars_to_a_loop_around_a_match() {
    // `while let pat = e { body }` -> `loop { match e { pat => { body }, _ => break } }`.
    let hir = lower_src("fun f() { while let .some(x) = next() { foo(x); } }");
    let (_, f) = only_function(&hir);
    let body = hir.block(f.block.unwrap());
    let StmtKind::Expr(loop_expr_id) = hir.stmt(body.stmts[0]).kind else {
        panic!("expected an expr statement wrapping the loop")
    };
    let (source, loop_body_id) = match &hir.expr(loop_expr_id).kind {
        ExprKind::Loop { source, block } => (source, *block),
        other => panic!("expected a loop expr, got {other:?}"),
    };
    assert!(matches!(source, LoopSource::While));

    // Unlike `while`, the body can't be spliced into the loop -- it only runs on a match -- so
    // the loop holds exactly the one match statement.
    let loop_body = hir.block(loop_body_id);
    assert_eq!(loop_body.stmts.len(), 1);
    let StmtKind::Expr(match_id) = hir.stmt(loop_body.stmts[0]).kind else {
        panic!("expected the match to be an expr statement")
    };
    match &hir.expr(match_id).kind {
        ExprKind::Match { arms, .. } => {
            assert_eq!(arms.len(), 2);
            assert!(matches!(
                hir.pat(hir.arm(arms[0]).pat).kind,
                PatKind::Variant { .. }
            ));
            let break_arm = hir.arm(arms[1]);
            assert!(matches!(hir.pat(break_arm.pat).kind, PatKind::Wildcard));
            let b = hir.block(break_arm.block);
            assert!(matches!(hir.stmt(b.stmts[0]).kind, StmtKind::Break));
        }
        other => panic!("expected a match expr, got {other:?}"),
    }
}

#[test]
fn for_loop_desugars_to_iterator_protocol() {
    // `for pat in iter { body }` ->
    // `{ let mut __iter = iter; loop { match __iter.next() { Some(pat) => body, None => break } } }`
    // (see `lower_for`).
    let hir = lower_src("fun f() { for x in xs { foo(x); } }");
    let (_, f) = only_function(&hir);
    let body = hir.block(f.block.unwrap());
    assert!(body.expr.is_none());
    assert_eq!(body.stmts.len(), 1);
    let StmtKind::Expr(outer_id) = hir.stmt(body.stmts[0]).kind else {
        panic!("expected the desugared for-loop to be an expr statement")
    };
    let outer = hir.expr(outer_id);
    let inner_block = match &outer.kind {
        ExprKind::Block(b) => hir.block(*b),
        other => panic!("expected the desugared for-loop to be a block, got {other:?}"),
    };
    assert_eq!(inner_block.stmts.len(), 2);

    let StmtKind::Let {
        mutability,
        pat: iter_pat,
        ..
    } = &hir.stmt(inner_block.stmts[0]).kind
    else {
        panic!("expected the first statement to bind __iter")
    };
    assert_eq!(*mutability, Mutability::Mutable);
    match &hir.pat(*iter_pat).kind {
        PatKind::Binding { name, .. } => assert_eq!(text(*name), "__iter"),
        other => panic!("expected a binding pattern, got {other:?}"),
    }

    let StmtKind::Expr(loop_expr_id) = hir.stmt(inner_block.stmts[1]).kind else {
        panic!("expected the second statement to be the loop")
    };
    let (source, loop_body_id) = match &hir.expr(loop_expr_id).kind {
        ExprKind::Loop { source, block } => (source, *block),
        other => panic!("expected a loop expr, got {other:?}"),
    };
    assert!(matches!(source, LoopSource::For));

    let loop_body = hir.block(loop_body_id);
    assert_eq!(loop_body.stmts.len(), 1);
    let StmtKind::Expr(match_id) = hir.stmt(loop_body.stmts[0]).kind else {
        panic!("expected the loop body to hold a match statement")
    };
    match &hir.expr(match_id).kind {
        ExprKind::Match { scrutinee, arms } => {
            match &hir.expr(*scrutinee).kind {
                ExprKind::Access { member, args, .. } => {
                    assert_eq!(text(*member), "next");
                    assert!(matches!(args, AccessArgs::Call(args) if args.is_empty()));
                }
                other => panic!("expected a `.next()` call, got {other:?}"),
            }
            assert_eq!(arms.len(), 2);
            let some_arm = hir.arm(arms[0]);
            match &hir.pat(some_arm.pat).kind {
                PatKind::Variant { variant, payload } => {
                    assert_eq!(text(*variant), "some");
                    assert!(matches!(payload, Payload::Single(_)));
                }
                other => panic!("expected a `.some(..)` pattern, got {other:?}"),
            }
            let none_arm = hir.arm(arms[1]);
            assert!(matches!(
                hir.pat(none_arm.pat).kind,
                PatKind::Variant {
                    payload: Payload::None,
                    ..
                }
            ));
        }
        other => panic!("expected a match expr, got {other:?}"),
    }
}
