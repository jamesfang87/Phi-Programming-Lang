use super::*;
use crate::ast::interner::Interner;
use crate::ast::{BinaryOp, Ident, Mutability, Path, UnaryOp, Visibility};
use crate::diag::DiagCtx;
use crate::driver::src_map::SrcMap;
use crate::hir::ids::{DefId, LocalId};
use crate::hir::{
    AccessArgs, Arm, Block, Closure, Enum, Expr, ExprKind, Extend, Field, Function, LoopSource,
    Module, Node, OwnerNode, Param, Pat, PatKind, Payload, Stmt, StmtKind, Struct, Trait, Ty,
    TyKind, VariantPayload,
};
use crate::lexer::Lexer;
use crate::parser::Parser;

// -----------------------------------------------------------------
// Driving the pipeline
// -----------------------------------------------------------------

/// Lexes, parses, and lowers `src` into an `Hir`, asserting no diagnostics were raised along
/// the way.
fn lower_src(src: &str) -> Hir {
    DiagCtx::clear();
    Interner::clear();
    let chars: Vec<char> = src.chars().collect();
    let offset = SrcMap::add_file("<test>".to_string(), chars.clone());
    let tokens = Lexer::new(&chars, offset).tokenize();
    let unit = Parser::new(tokens, offset).parse();
    let diagnostics = DiagCtx::diagnostics();
    assert!(
        diagnostics.is_empty(),
        "unexpected diagnostics for {src:?}: {diagnostics:?}"
    );
    lower_unit(&[unit])
}

fn text(ident: Ident) -> String {
    Interner::resolve(ident.text)
}

// -----------------------------------------------------------------
// Typed node lookup helpers
// -----------------------------------------------------------------

fn node_in(hir: &Hir, owner: DefId, id: LocalId) -> &Node {
    hir.arena(owner).get(id)
}

fn find_value(hir: &Hir, m: &Module, name: &str) -> DefId {
    m.items
        .iter()
        .copied()
        .find(|&id| {
            matches!(hir.owner(id), OwnerNode::Function(f)
                if text(f.name) == name)
        })
        .unwrap_or_else(|| panic!("no {name:?} in module's items"))
}

fn find_type(hir: &Hir, m: &Module, name: &str) -> DefId {
    m.items
        .iter()
        .copied()
        .find(|&id| match hir.owner(id) {
            OwnerNode::Struct(s) => text(s.name) == name,
            OwnerNode::Enum(e) => text(e.name) == name,
            OwnerNode::Trait(t) => text(t.name) == name,
            _ => false,
        })
        .unwrap_or_else(|| panic!("no {name:?} in module's items"))
}

fn as_function(hir: &Hir, id: DefId) -> &Function {
    match hir.owner(id) {
        OwnerNode::Function(f) => f,
        other => panic!("expected a function owner, got {other:?}"),
    }
}

fn as_struct(hir: &Hir, id: DefId) -> &Struct {
    match hir.owner(id) {
        OwnerNode::Struct(s) => s,
        other => panic!("expected a struct owner, got {other:?}"),
    }
}

fn as_enum(hir: &Hir, id: DefId) -> &Enum {
    match hir.owner(id) {
        OwnerNode::Enum(e) => e,
        other => panic!("expected an enum owner, got {other:?}"),
    }
}

fn as_trait(hir: &Hir, id: DefId) -> &Trait {
    match hir.owner(id) {
        OwnerNode::Trait(t) => t,
        other => panic!("expected a trait owner, got {other:?}"),
    }
}

fn as_extend(hir: &Hir, id: DefId) -> &Extend {
    match hir.owner(id) {
        OwnerNode::Extend(e) => e,
        other => panic!("expected an extend owner, got {other:?}"),
    }
}

fn as_closure(hir: &Hir, id: DefId) -> &Closure {
    match hir.owner(id) {
        OwnerNode::Closure(c) => c,
        other => panic!("expected a closure owner, got {other:?}"),
    }
}

fn block<'h>(hir: &'h Hir, owner: DefId, id: LocalId) -> &'h Block {
    match node_in(hir, owner, id) {
        Node::Block(b) => b,
        other => panic!("expected a block node, got {other:?}"),
    }
}

fn stmt<'h>(hir: &'h Hir, owner: DefId, id: LocalId) -> &'h Stmt {
    match node_in(hir, owner, id) {
        Node::Stmt(s) => s,
        other => panic!("expected a stmt node, got {other:?}"),
    }
}

fn expr<'h>(hir: &'h Hir, owner: DefId, id: LocalId) -> &'h Expr {
    match node_in(hir, owner, id) {
        Node::Expr(e) => e,
        other => panic!("expected an expr node, got {other:?}"),
    }
}

fn pat<'h>(hir: &'h Hir, owner: DefId, id: LocalId) -> &'h Pat {
    match node_in(hir, owner, id) {
        Node::Pat(p) => p,
        other => panic!("expected a pat node, got {other:?}"),
    }
}

fn ty<'h>(hir: &'h Hir, owner: DefId, id: LocalId) -> &'h Ty {
    match node_in(hir, owner, id) {
        Node::Ty(t) => t,
        other => panic!("expected a ty node, got {other:?}"),
    }
}

fn arm<'h>(hir: &'h Hir, owner: DefId, id: LocalId) -> &'h Arm {
    match node_in(hir, owner, id) {
        Node::Arm(a) => a,
        other => panic!("expected an arm node, got {other:?}"),
    }
}

fn field<'h>(hir: &'h Hir, owner: DefId, id: LocalId) -> &'h Field {
    match node_in(hir, owner, id) {
        Node::Field(f) => f,
        other => panic!("expected a field node, got {other:?}"),
    }
}

fn param<'h>(hir: &'h Hir, owner: DefId, id: LocalId) -> &'h Param {
    match node_in(hir, owner, id) {
        Node::Param(p) => p,
        other => panic!("expected a param node, got {other:?}"),
    }
}

/// The sole top-level function in a single-item source, together with its `DefId`.
fn only_function(hir: &Hir) -> (DefId, &Function) {
    let m = hir.root();
    assert_eq!(m.items.len(), 1);
    let id = m.items[0];
    (id, as_function(hir, id))
}

// -----------------------------------------------------------------
// Items
// -----------------------------------------------------------------

#[test]
fn function_is_declared_in_the_module() {
    let hir = lower_src("fun main() {}");
    let m = hir.root();
    let id = find_value(&hir, m, "main");
    let f = as_function(&hir, id);
    assert_eq!(text(f.name), "main");
    assert!(matches!(f.visibility, Visibility::Private));
    assert!(f.self_param.is_none());
    assert!(f.params.is_empty());
    assert!(f.ret.is_none());
    let body = block(&hir, id, f.body.expect("expected a body"));
    assert!(body.stmts.is_empty());
    assert!(body.expr.is_none());
}

#[test]
fn function_params_and_return_type_are_lowered() {
    let hir = lower_src("fun add(x: i32, y: i32) -> i32 { x + y }");
    let (id, f) = only_function(&hir);
    assert_eq!(f.params.len(), 2);
    let x = param(&hir, id, f.params[0]);
    assert_eq!(text(x.name), "x");
    match &ty(&hir, id, x.ty).kind {
        TyKind::Base { path, args } => {
            assert_eq!(text(path.segments[0]), "i32");
            assert!(args.is_empty());
        }
        other => panic!("expected a base type, got {other:?}"),
    }
    let ret_ty = ty(&hir, id, f.ret.expect("expected a return type"));
    assert!(matches!(&ret_ty.kind, TyKind::Base { .. }));

    let body = block(&hir, id, f.body.expect("expected a body"));
    assert!(body.stmts.is_empty());
    let tail = expr(&hir, id, body.expr.expect("expected a tail expression"));
    assert!(matches!(
        tail.kind,
        ExprKind::Binary {
            op: BinaryOp::Add,
            ..
        }
    ));
}

#[test]
fn struct_has_its_fields_lowered() {
    let hir = lower_src("struct Point { x: i32, y: i32 }");
    let m = hir.root();
    let id = find_type(&hir, m, "Point");
    let s = as_struct(&hir, id);
    assert_eq!(text(s.name), "Point");
    assert_eq!(s.fields.len(), 2);
    let x = field(&hir, id, s.fields[0]);
    assert_eq!(text(x.name), "x");
}

#[test]
fn enum_variants_are_lowered() {
    let hir = lower_src("enum Shape { Circle: f64, Rectangle: { w: f64, h: f64 }, Point }");
    let m = hir.root();
    let id = find_type(&hir, m, "Shape");
    let e = as_enum(&hir, id);
    assert_eq!(e.variants.len(), 3);

    let circle = match node_in(&hir, id, e.variants[0]) {
        Node::Variant(v) => v,
        other => panic!("expected a variant node, got {other:?}"),
    };
    assert_eq!(text(circle.name), "Circle");
    assert!(matches!(circle.payload, VariantPayload::Type(_)));

    let rect = match node_in(&hir, id, e.variants[1]) {
        Node::Variant(v) => v,
        other => panic!("expected a variant node, got {other:?}"),
    };
    match &rect.payload {
        VariantPayload::Record(fields) => assert_eq!(fields.len(), 2),
        other => panic!("expected a record payload, got {other:?}"),
    }

    let point = match node_in(&hir, id, e.variants[2]) {
        Node::Variant(v) => v,
        other => panic!("expected a variant node, got {other:?}"),
    };
    assert!(matches!(point.payload, VariantPayload::Unit));
}

#[test]
fn trait_functions_are_lowered_as_independent_owners() {
    let hir = lower_src("trait Shape { fun area(&self) -> f64; }");
    let m = hir.root();
    let id = find_type(&hir, m, "Shape");
    let t = as_trait(&hir, id);
    assert_eq!(t.functions.len(), 1);
    let method_id = t.functions[0];
    assert_ne!(method_id, id);
    let f = as_function(&hir, method_id);
    assert_eq!(text(f.name), "area");
    assert!(f.self_param.is_some());
    // The trait's own function declares no body.
    assert!(f.body.is_none());
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
    let e = as_extend(&hir, id);
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
    let method = as_function(&hir, e.methods[0]);
    assert_eq!(text(method.name), "get");
}

#[test]
fn generic_params_carry_their_bounds() {
    let hir = lower_src("struct Wrapper<T: Clone> { value: T }");
    let m = hir.root();
    let id = find_type(&hir, m, "Wrapper");
    let s = as_struct(&hir, id);
    assert_eq!(s.generics.len(), 1);
    let g = match node_in(&hir, id, s.generics[0]) {
        Node::Generic(g) => g,
        other => panic!("expected a generic node, got {other:?}"),
    };
    assert_eq!(text(g.name), "T");
    assert_eq!(g.bounds.len(), 1);
    assert_eq!(text(g.bounds[0].segments[0]), "Clone");
}

#[test]
fn import_glob_and_alias_are_lowered_into_the_module() {
    let hir = lower_src("import math::vector as mv; import math::*;");
    let m = hir.root();
    assert_eq!(m.imports.len(), 2);
    let aliased = match node_in(&hir, hir.root_id(), m.imports[0]) {
        Node::Import(i) => i,
        other => panic!("expected an import node, got {other:?}"),
    };
    assert!(!aliased.glob);
    assert_eq!(text(aliased.alias.expect("expected an alias")), "mv");

    let glob = match node_in(&hir, hir.root_id(), m.imports[1]) {
        Node::Import(i) => i,
        other => panic!("expected an import node, got {other:?}"),
    };
    assert!(glob.glob);
    assert!(glob.alias.is_none());
}

#[test]
fn nested_module_declaration_synthesizes_ancestor_modules() {
    // The parser doesn't currently wire a file's `module` header into `SrcUnit::module`, so
    // this exercises `LoweringCtx::module_for_path` directly by attaching the decl by hand.
    let mut unit = {
        DiagCtx::clear();
        Interner::clear();
        let src = "fun helper() {}";
        let chars: Vec<char> = src.chars().collect();
        let offset = SrcMap::add_file("<test>".to_string(), chars.clone());
        let tokens = Lexer::new(&chars, offset).tokenize();
        Parser::new(tokens, offset).parse()
    };
    let path_span = unit.span;
    unit.module = Some(ast::ModuleDecl {
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

    let hir = lower_unit(&[unit]);
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
                    && matches!(hir.owner(id), OwnerNode::Module(m)
                        if text(*m.path.segments.last().unwrap()) == last_segment)
            })
            .unwrap_or_else(|| panic!("no synthesized module ending in {last_segment:?}"))
    };

    let math_id = find_module(&hir, "math");
    let math_module = match hir.owner(math_id) {
        OwnerNode::Module(m) => m,
        other => panic!("expected a module owner, got {other:?}"),
    };
    assert_eq!(root.items[0], math_id);
    assert_eq!(math_module.items.len(), 1);

    let vector_id = find_module(&hir, "vector");
    let vector_module = match hir.owner(vector_id) {
        OwnerNode::Module(m) => m,
        other => panic!("expected a module owner, got {other:?}"),
    };
    assert_eq!(vector_module.items.len(), 1);
    let helper_id = vector_module.items[0];
    assert_eq!(text(as_function(&hir, helper_id).name), "helper");

    // Synthesized ancestors are parented just like declared ones, so a def in the innermost
    // module can still be walked all the way back to the root.
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
    let extend_id = root
        .items
        .iter()
        .copied()
        .find(|&id| matches!(hir.owner(id), OwnerNode::Extend(_)))
        .expect("no extend block in the module's items");

    let t_id = as_trait(&hir, trait_id).functions[0];
    let m_id = as_extend(&hir, extend_id).methods[0];

    // A method hangs off the item that declares it, not off the module the item sits in --
    // which is what lets `Self` be recovered from the method's id alone.
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
    let body = block(&hir, f_id, f.body.unwrap());
    let StmtKind::Let(let_stmt) = &stmt(&hir, f_id, body.stmts[0]).kind else {
        panic!("expected a let statement")
    };
    let ExprKind::Closure(closure_id) = &expr(&hir, f_id, let_stmt.init).kind else {
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
    let (id, f) = only_function(&hir);
    assert_eq!(f.params.len(), 5);

    let a = ty(&hir, id, param(&hir, id, f.params[0]).ty);
    match &a.kind {
        TyKind::Ref { mutability, .. } => assert_eq!(*mutability, Mutability::Mutable),
        other => panic!("expected a ref type, got {other:?}"),
    }

    let b = ty(&hir, id, param(&hir, id, f.params[1]).ty);
    assert!(matches!(&b.kind, TyKind::Any(_)));

    let c = ty(&hir, id, param(&hir, id, f.params[2]).ty);
    match &c.kind {
        TyKind::Tuple(elems) => assert_eq!(elems.len(), 2),
        other => panic!("expected a tuple type, got {other:?}"),
    }

    let d = ty(&hir, id, param(&hir, id, f.params[3]).ty);
    match &d.kind {
        TyKind::Array { len, .. } => assert!(len.is_some()),
        other => panic!("expected an array type, got {other:?}"),
    }

    let e = ty(&hir, id, param(&hir, id, f.params[4]).ty);
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
    let (id, f) = only_function(&hir);
    let body = block(&hir, id, f.body.unwrap());
    assert_eq!(body.stmts.len(), 3);

    let StmtKind::Let(p_let) = &stmt(&hir, id, body.stmts[0]).kind else {
        panic!("expected a let statement")
    };
    match &expr(&hir, id, p_let.init).kind {
        ExprKind::Ctor { path, payload } => {
            let path = path.as_ref().expect("`Point { .. }` names its type");
            assert_eq!(text(path.segments[0]), "Point");
            assert_eq!(payload.len(), 2);
            assert_eq!(text(payload[0].0), "x");
        }
        other => panic!("expected a ctor expr, got {other:?}"),
    }

    let StmtKind::Let(t_let) = &stmt(&hir, id, body.stmts[1]).kind else {
        panic!("expected a let statement")
    };
    match &expr(&hir, id, t_let.init).kind {
        ExprKind::Tuple(elems) => assert_eq!(elems.len(), 3),
        other => panic!("expected a tuple expr, got {other:?}"),
    }

    let StmtKind::Let(r_let) = &stmt(&hir, id, body.stmts[2]).kind else {
        panic!("expected a let statement")
    };
    match &expr(&hir, id, r_let.init).kind {
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
    let (id, f) = only_function(&hir);
    let body = block(&hir, id, f.body.unwrap());
    let tail = expr(&hir, id, body.expr.unwrap());
    match &tail.kind {
        ExprKind::Index { base, .. } => match &expr(&hir, id, *base).kind {
            ExprKind::Access { base, member, args } => {
                assert_eq!(text(*member), "c");
                assert!(matches!(args, AccessArgs::Call(args) if args.len() == 1));
                assert!(matches!(
                    &expr(&hir, id, *base).kind,
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
    let (id, f) = only_function(&hir);
    let body = block(&hir, id, f.body.unwrap());
    assert_eq!(body.stmts.len(), 1);

    let StmtKind::Expr(if_id) = stmt(&hir, id, body.stmts[0]).kind else {
        panic!("expected an expr statement")
    };
    match &expr(&hir, id, if_id).kind {
        ExprKind::If { else_branch, .. } => assert!(else_branch.is_some()),
        other => panic!("expected an if expr, got {other:?}"),
    }

    let tail = expr(&hir, id, body.expr.unwrap());
    match &tail.kind {
        ExprKind::Match { arms, .. } => {
            assert_eq!(arms.len(), 2);
            let first = arm(&hir, id, arms[0]);
            match &pat(&hir, id, first.pat).kind {
                PatKind::Variant { variant, payload } => {
                    assert_eq!(text(*variant), "circle");
                    let Payload::Single(inner) = payload else {
                        panic!("expected a single payload, got {payload:?}")
                    };
                    assert!(matches!(
                        pat(&hir, id, *inner).kind,
                        PatKind::Binding { .. }
                    ));
                }
                other => panic!("expected a variant pattern, got {other:?}"),
            }
            let second = arm(&hir, id, arms[1]);
            assert!(matches!(pat(&hir, id, second.pat).kind, PatKind::Wildcard));
        }
        other => panic!("expected a match expr, got {other:?}"),
    }
}

// -----------------------------------------------------------------
// Variants
// -----------------------------------------------------------------

/// The `let` initializer of the sole statement in `src`'s only function.
fn only_init(hir: &Hir) -> (DefId, LocalId) {
    let (id, f) = only_function(hir);
    let body = block(hir, id, f.body.unwrap());
    let StmtKind::Let(let_stmt) = &stmt(hir, id, body.stmts[0]).kind else {
        panic!("expected a let statement")
    };
    (id, let_stmt.init)
}

#[test]
fn payload_less_variant_lowers_with_no_payload() {
    let hir = lower_src("fun f() { let x = .none; }");
    let (id, init) = only_init(&hir);
    match &expr(&hir, id, init).kind {
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
    let (id, init) = only_init(&hir);
    match &expr(&hir, id, init).kind {
        ExprKind::Variant { variant, payload } => {
            assert_eq!(text(*variant), "parallelogram");
            let Payload::Single(inner) = payload else {
                panic!("expected a single payload, got {payload:?}")
            };
            match &expr(&hir, id, *inner).kind {
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
    let (id, init) = only_init(&hir);
    match &expr(&hir, id, init).kind {
        ExprKind::Variant { variant, payload } => {
            assert_eq!(text(*variant), "square");
            let Payload::Record(fields) = payload else {
                panic!("expected a record payload, got {payload:?}")
            };
            assert_eq!(fields.len(), 1);
            assert_eq!(text(fields[0].0), "l");
            assert!(matches!(
                expr(&hir, id, fields[0].1).kind,
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
    let (id, init) = only_init(&hir);
    match &expr(&hir, id, init).kind {
        ExprKind::Variant { payload, .. } => {
            let Payload::Record(fields) = payload else {
                panic!("expected a record payload, got {payload:?}")
            };
            assert_eq!(text(fields[0].0), "l");
            match &expr(&hir, id, fields[0].1).kind {
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
    let (id, f) = only_function(&hir);
    let body = block(&hir, id, f.body.unwrap());
    let ExprKind::Match { arms, .. } = &expr(&hir, id, body.expr.unwrap()).kind else {
        panic!("expected a match expr")
    };
    match &pat(&hir, id, arm(&hir, id, arms[0]).pat).kind {
        PatKind::Variant { variant, payload } => {
            assert_eq!(text(*variant), "square");
            let Payload::Record(fields) = payload else {
                panic!("expected a record payload, got {payload:?}")
            };
            assert_eq!(text(fields[0].0), "l");
            match &pat(&hir, id, fields[0].1).kind {
                PatKind::Binding { name, .. } => assert_eq!(text(*name), "l"),
                other => panic!("expected the shorthand to become a binding, got {other:?}"),
            }
        }
        other => panic!("expected a variant pattern, got {other:?}"),
    }
}

#[test]
fn elided_struct_literal_names_no_type() {
    let hir = lower_src("fun f() { let x = .{ l: 4.0, w: 6.0 }; }");
    let (id, init) = only_init(&hir);
    match &expr(&hir, id, init).kind {
        ExprKind::Ctor { path, payload } => {
            assert!(path.is_none(), "`.{{ .. }}` names no type");
            assert_eq!(payload.len(), 2);
            assert_eq!(text(payload[0].0), "l");
        }
        other => panic!("expected a ctor expr, got {other:?}"),
    }
}

#[test]
fn closure_is_lowered_as_its_own_owner() {
    let hir = lower_src("fun f() { let add = |x: i32, y: i32| -> i32 { x + y }; }");
    let (id, f) = only_function(&hir);
    let body = block(&hir, id, f.body.unwrap());
    let StmtKind::Let(let_stmt) = &stmt(&hir, id, body.stmts[0]).kind else {
        panic!("expected a let statement")
    };
    let closure_id = match &expr(&hir, id, let_stmt.init).kind {
        ExprKind::Closure(closure_id) => *closure_id,
        other => panic!("expected a closure expr, got {other:?}"),
    };
    assert_ne!(closure_id, id);
    let c = as_closure(&hir, closure_id);
    assert_eq!(c.params.len(), 2);
    assert!(c.ret.is_some());
    // The closure's block body `{ x + y }` lowers to its own `ExprKind::Block` wrapping the
    // block whose tail is the addition.
    let closure_body_block = match &expr(&hir, closure_id, c.body).kind {
        ExprKind::Block(b) => block(&hir, closure_id, *b),
        other => panic!("expected the closure body to be a block expr, got {other:?}"),
    };
    assert!(matches!(
        expr(&hir, closure_id, closure_body_block.expr.unwrap()).kind,
        ExprKind::Binary { .. }
    ));
}

#[test]
fn block_tail_expression_is_not_a_statement() {
    let hir = lower_src("fun f() { let x = 1; x }");
    let (id, f) = only_function(&hir);
    let body = block(&hir, id, f.body.unwrap());
    assert_eq!(body.stmts.len(), 1);
    assert!(matches!(
        stmt(&hir, id, body.stmts[0]).kind,
        StmtKind::Let(_)
    ));
    assert!(matches!(
        expr(&hir, id, body.expr.unwrap()).kind,
        ExprKind::Path(_)
    ));
}

/// The trailing `;` is what discards a block's value, so the same final expression is the
/// block's tail without one and an ordinary statement with one.
#[test]
fn a_trailing_semicolon_discards_the_block_value() {
    let hir = lower_src("fun f() { g() }");
    let (id, f) = only_function(&hir);
    let body = block(&hir, id, f.body.unwrap());
    assert!(body.stmts.is_empty());
    assert!(body.expr.is_some());

    let hir = lower_src("fun f() { g(); }");
    let (id, f) = only_function(&hir);
    let body = block(&hir, id, f.body.unwrap());
    assert_eq!(body.stmts.len(), 1);
    assert!(body.expr.is_none());
}

/// The same holds for a block-bodied expression, which may drop its `;` but doesn't have to --
/// so both spellings stay distinguishable in the last position.
#[test]
fn a_block_bodied_expression_is_a_tail_only_without_a_semicolon() {
    let hir = lower_src("fun f() { if c { 1 } else { 2 } }");
    let (id, f) = only_function(&hir);
    let body = block(&hir, id, f.body.unwrap());
    assert!(body.stmts.is_empty());
    assert!(matches!(
        expr(&hir, id, body.expr.unwrap()).kind,
        ExprKind::If { .. }
    ));

    let hir = lower_src("fun f() { if c { 1 } else { 2 }; }");
    let (id, f) = only_function(&hir);
    let body = block(&hir, id, f.body.unwrap());
    assert_eq!(body.stmts.len(), 1);
    assert!(body.expr.is_none());
}

// -----------------------------------------------------------------
// Statements
// -----------------------------------------------------------------

#[test]
fn lowers_break_continue_return_defer_stmts() {
    let hir = lower_src("fun f() { while true { break; continue; } return 1; defer cleanup(); }");
    let (id, f) = only_function(&hir);
    let body = block(&hir, id, f.body.unwrap());
    assert_eq!(body.stmts.len(), 3);
    assert!(matches!(
        stmt(&hir, id, body.stmts[1]).kind,
        StmtKind::Return(Some(_))
    ));
    assert!(matches!(
        stmt(&hir, id, body.stmts[2]).kind,
        StmtKind::Defer(_)
    ));
}

#[test]
fn lowers_with_stmt_lends() {
    let hir = lower_src("fun f() { with x = &a, y = &mut b { foo(); } }");
    let (id, f) = only_function(&hir);
    let body = block(&hir, id, f.body.unwrap());
    match &stmt(&hir, id, body.stmts[0]).kind {
        StmtKind::With { lends, body } => {
            assert_eq!(lends.len(), 2);
            assert!(matches!(
                expr(&hir, id, lends[0].init).kind,
                ExprKind::Borrow {
                    mutability: Mutability::Immutable,
                    ..
                }
            ));
            assert!(matches!(
                expr(&hir, id, lends[1].init).kind,
                ExprKind::Borrow {
                    mutability: Mutability::Mutable,
                    ..
                }
            ));
            // `foo();` was written with a `;`, so it stays a statement and the block has no
            // value (see `lower_block`).
            let with_body = block(&hir, id, *body);
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
    let (id, f) = only_function(&hir);
    let body = block(&hir, id, f.body.unwrap());
    let StmtKind::Expr(loop_expr_id) = stmt(&hir, id, body.stmts[0]).kind else {
        panic!("expected an expr statement wrapping the loop")
    };
    let (source, loop_body_id) = match &expr(&hir, id, loop_expr_id).kind {
        ExprKind::Loop { source, body } => (source, *body),
        other => panic!("expected a loop expr, got {other:?}"),
    };
    assert!(matches!(source, LoopSource::While));

    let loop_body = block(&hir, id, loop_body_id);
    // Guard statement, plus the original body's one statement.
    assert_eq!(loop_body.stmts.len(), 2);

    let StmtKind::Expr(guard_id) = stmt(&hir, id, loop_body.stmts[0]).kind else {
        panic!("expected the guard to be an expr statement")
    };
    match &expr(&hir, id, guard_id).kind {
        ExprKind::If {
            cond, then_branch, ..
        } => {
            assert!(matches!(
                expr(&hir, id, *cond).kind,
                ExprKind::Unary {
                    op: UnaryOp::Not,
                    ..
                }
            ));
            let then_block = block(&hir, id, *then_branch);
            assert_eq!(then_block.stmts.len(), 1);
            assert!(matches!(
                stmt(&hir, id, then_block.stmts[0]).kind,
                StmtKind::Break
            ));
        }
        other => panic!("expected the guard to be an if expr, got {other:?}"),
    }

    assert!(matches!(
        stmt(&hir, id, loop_body.stmts[1]).kind,
        StmtKind::Expr(_)
    ));
}

#[test]
fn if_let_desugars_to_a_match() {
    // `if let pat = e { a } else { b }` -> `match e { pat => { a }, _ => { b } }`.
    let hir = lower_src("fun f() -> i32 { if let .some(x) = o { x } else { 0 } }");
    let (id, f) = only_function(&hir);
    let body = block(&hir, id, f.body.unwrap());
    match &expr(&hir, id, body.expr.unwrap()).kind {
        ExprKind::Match { arms, .. } => {
            assert_eq!(arms.len(), 2);
            let first = arm(&hir, id, arms[0]);
            assert!(matches!(
                pat(&hir, id, first.pat).kind,
                PatKind::Variant { .. }
            ));
            let second = arm(&hir, id, arms[1]);
            assert!(matches!(pat(&hir, id, second.pat).kind, PatKind::Wildcard));
        }
        other => panic!("expected a match expr, got {other:?}"),
    }
}

/// A `match` has to be exhaustive even when the source `if let` had no `else`, so the wildcard
/// arm is always there -- yielding an empty block, the same value an `else`-less `if` produces.
#[test]
fn if_let_without_else_still_gets_a_wildcard_arm() {
    let hir = lower_src("fun f() { if let .some(x) = o { foo(x); } }");
    let (id, f) = only_function(&hir);
    let body = block(&hir, id, f.body.unwrap());
    match &expr(&hir, id, body.expr.unwrap()).kind {
        ExprKind::Match { arms, .. } => {
            assert_eq!(arms.len(), 2);
            let second = arm(&hir, id, arms[1]);
            assert!(matches!(pat(&hir, id, second.pat).kind, PatKind::Wildcard));
            match &expr(&hir, id, second.body).kind {
                ExprKind::Block(b) => {
                    let b = block(&hir, id, *b);
                    assert!(b.stmts.is_empty() && b.expr.is_none());
                }
                other => panic!("expected an empty block, got {other:?}"),
            }
        }
        other => panic!("expected a match expr, got {other:?}"),
    }
}

#[test]
fn while_let_desugars_to_a_loop_around_a_match() {
    // `while let pat = e { body }` -> `loop { match e { pat => { body }, _ => break } }`.
    let hir = lower_src("fun f() { while let .some(x) = next() { foo(x); } }");
    let (id, f) = only_function(&hir);
    let body = block(&hir, id, f.body.unwrap());
    let StmtKind::Expr(loop_expr_id) = stmt(&hir, id, body.stmts[0]).kind else {
        panic!("expected an expr statement wrapping the loop")
    };
    let (source, loop_body_id) = match &expr(&hir, id, loop_expr_id).kind {
        ExprKind::Loop { source, body } => (source, *body),
        other => panic!("expected a loop expr, got {other:?}"),
    };
    assert!(matches!(source, LoopSource::While));

    // Unlike `while`, the body can't be spliced into the loop -- it only runs on a match -- so
    // the loop holds exactly the one match statement.
    let loop_body = block(&hir, id, loop_body_id);
    assert_eq!(loop_body.stmts.len(), 1);
    let StmtKind::Expr(match_id) = stmt(&hir, id, loop_body.stmts[0]).kind else {
        panic!("expected the match to be an expr statement")
    };
    match &expr(&hir, id, match_id).kind {
        ExprKind::Match { arms, .. } => {
            assert_eq!(arms.len(), 2);
            assert!(matches!(
                pat(&hir, id, arm(&hir, id, arms[0]).pat).kind,
                PatKind::Variant { .. }
            ));
            let break_arm = arm(&hir, id, arms[1]);
            assert!(matches!(
                pat(&hir, id, break_arm.pat).kind,
                PatKind::Wildcard
            ));
            match &expr(&hir, id, break_arm.body).kind {
                ExprKind::Block(b) => {
                    let b = block(&hir, id, *b);
                    assert!(matches!(stmt(&hir, id, b.stmts[0]).kind, StmtKind::Break));
                }
                other => panic!("expected a block holding `break`, got {other:?}"),
            }
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
    let (id, f) = only_function(&hir);
    let body = block(&hir, id, f.body.unwrap());
    assert!(body.expr.is_none());
    assert_eq!(body.stmts.len(), 1);
    let StmtKind::Expr(outer_id) = stmt(&hir, id, body.stmts[0]).kind else {
        panic!("expected the desugared for-loop to be an expr statement")
    };
    let outer = expr(&hir, id, outer_id);
    let inner_block = match &outer.kind {
        ExprKind::Block(b) => block(&hir, id, *b),
        other => panic!("expected the desugared for-loop to be a block, got {other:?}"),
    };
    assert_eq!(inner_block.stmts.len(), 2);

    let StmtKind::Let(iter_let) = &stmt(&hir, id, inner_block.stmts[0]).kind else {
        panic!("expected the first statement to bind __iter")
    };
    assert_eq!(iter_let.mutability, Mutability::Mutable);
    match &pat(&hir, id, iter_let.pat).kind {
        PatKind::Binding { name, .. } => assert_eq!(text(*name), "__iter"),
        other => panic!("expected a binding pattern, got {other:?}"),
    }

    let StmtKind::Expr(loop_expr_id) = stmt(&hir, id, inner_block.stmts[1]).kind else {
        panic!("expected the second statement to be the loop")
    };
    let (source, loop_body_id) = match &expr(&hir, id, loop_expr_id).kind {
        ExprKind::Loop { source, body } => (source, *body),
        other => panic!("expected a loop expr, got {other:?}"),
    };
    assert!(matches!(source, LoopSource::For));

    let loop_body = block(&hir, id, loop_body_id);
    assert_eq!(loop_body.stmts.len(), 1);
    let StmtKind::Expr(match_id) = stmt(&hir, id, loop_body.stmts[0]).kind else {
        panic!("expected the loop body to hold a match statement")
    };
    match &expr(&hir, id, match_id).kind {
        ExprKind::Match { scrutinee, arms } => {
            match &expr(&hir, id, *scrutinee).kind {
                ExprKind::Access { member, args, .. } => {
                    assert_eq!(text(*member), "next");
                    assert!(matches!(args, AccessArgs::Call(args) if args.is_empty()));
                }
                other => panic!("expected a `.next()` call, got {other:?}"),
            }
            assert_eq!(arms.len(), 2);
            let some_arm = arm(&hir, id, arms[0]);
            match &pat(&hir, id, some_arm.pat).kind {
                PatKind::Variant { variant, payload } => {
                    assert_eq!(text(*variant), "some");
                    assert!(matches!(payload, Payload::Single(_)));
                }
                other => panic!("expected a `.some(..)` pattern, got {other:?}"),
            }
            let none_arm = arm(&hir, id, arms[1]);
            assert!(matches!(
                pat(&hir, id, none_arm.pat).kind,
                PatKind::Variant {
                    payload: Payload::None,
                    ..
                }
            ));
        }
        other => panic!("expected a match expr, got {other:?}"),
    }
}
