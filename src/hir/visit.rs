use crate::hir::{
    AccessArgs, DefId, ExprKind, Hir, HirId, OwnerNode, PatKind, Path, Payload, StmtKind, TyKind,
    VariantPayload,
};

/// A traversal over the HIR. See the [module docs](self) for how overriding works.
pub trait Visitor<'hir>: Sized {
    fn hir(&self) -> &'hir Hir;

    fn visit_nested_owner(&mut self, _def_id: DefId) {}

    fn visit_path(&mut self, _path: &'hir Path) {}

    fn visit_module(&mut self, def_id: DefId) {
        walk_module(self, def_id);
    }
    fn visit_item(&mut self, def_id: DefId) {
        walk_item(self, def_id);
    }
    fn visit_function(&mut self, def_id: DefId) {
        walk_function(self, def_id);
    }
    fn visit_struct(&mut self, def_id: DefId) {
        walk_struct(self, def_id);
    }
    fn visit_enum(&mut self, def_id: DefId) {
        walk_enum(self, def_id);
    }
    fn visit_trait(&mut self, def_id: DefId) {
        walk_trait(self, def_id);
    }
    fn visit_extend(&mut self, def_id: DefId) {
        walk_extend(self, def_id);
    }
    fn visit_closure(&mut self, def_id: DefId) {
        walk_closure(self, def_id);
    }

    fn visit_generic(&mut self, id: HirId) {
        walk_generic(self, id);
    }
    fn visit_self_param(&mut self, id: HirId) {
        let _ = id;
    }
    fn visit_import(&mut self, id: HirId) {
        let _ = id;
    }
    fn visit_param(&mut self, id: HirId) {
        walk_param(self, id);
    }
    fn visit_closure_param(&mut self, id: HirId) {
        walk_closure_param(self, id);
    }
    fn visit_field(&mut self, id: HirId) {
        walk_field(self, id);
    }
    fn visit_variant(&mut self, id: HirId) {
        walk_variant(self, id);
    }

    fn visit_block(&mut self, id: HirId) {
        walk_block(self, id);
    }
    fn visit_stmt(&mut self, id: HirId) {
        walk_stmt(self, id);
    }
    fn visit_arm(&mut self, id: HirId) {
        walk_arm(self, id);
    }
    fn visit_expr(&mut self, id: HirId) {
        walk_expr(self, id);
    }
    fn visit_pat(&mut self, id: HirId) {
        walk_pat(self, id);
    }
    fn visit_ty(&mut self, id: HirId) {
        walk_ty(self, id);
    }
}

// -----------------------------------------------------------------
// Owners
// -----------------------------------------------------------------

pub fn walk_module<'hir, V: Visitor<'hir>>(v: &mut V, def_id: DefId) {
    let OwnerNode::Module(module) = v.hir().def(def_id) else {
        unreachable!("{def_id:?} does not name a module");
    };

    for &id in &module.imports {
        v.visit_import(id);
    }
    for &item in &module.items {
        v.visit_item(item);
    }
}

/// Dispatches on what kind of definition `def_id` names.
pub fn walk_item<'hir, V: Visitor<'hir>>(v: &mut V, def_id: DefId) {
    match v.hir().def(def_id) {
        OwnerNode::Module(_) => v.visit_module(def_id),
        OwnerNode::Function(_) => v.visit_function(def_id),
        OwnerNode::Struct(_) => v.visit_struct(def_id),
        OwnerNode::Enum(_) => v.visit_enum(def_id),
        OwnerNode::Trait(_) => v.visit_trait(def_id),
        OwnerNode::Extend(_) => v.visit_extend(def_id),
        OwnerNode::Closure(_) => v.visit_closure(def_id),
    }
}

pub fn walk_function<'hir, V: Visitor<'hir>>(v: &mut V, def_id: DefId) {
    let OwnerNode::Function(function) = v.hir().def(def_id) else {
        unreachable!("root of a Function owner is always OwnerNode::Function");
    };

    for &id in &function.generics {
        v.visit_generic(id);
    }
    if let Some(id) = function.self_param {
        v.visit_self_param(id);
    }
    for &id in &function.params {
        v.visit_param(id);
    }
    if let Some(id) = function.ret {
        v.visit_ty(id);
    }
    if let Some(id) = function.block {
        v.visit_block(id);
    }
}

pub fn walk_struct<'hir, V: Visitor<'hir>>(v: &mut V, def_id: DefId) {
    let OwnerNode::Struct(struct_) = v.hir().def(def_id) else {
        unreachable!("root of a Struct owner is always OwnerNode::Struct");
    };

    for &id in &struct_.generics {
        v.visit_generic(id);
    }
    for &id in &struct_.fields {
        v.visit_field(id);
    }
}

pub fn walk_enum<'hir, V: Visitor<'hir>>(v: &mut V, def_id: DefId) {
    let OwnerNode::Enum(enum_) = v.hir().def(def_id) else {
        unreachable!("root of an Enum owner is always OwnerNode::Enum");
    };

    for &id in &enum_.generics {
        v.visit_generic(id);
    }
    for &id in &enum_.variants {
        v.visit_variant(id);
    }
}

pub fn walk_trait<'hir, V: Visitor<'hir>>(v: &mut V, def_id: DefId) {
    let OwnerNode::Trait(trait_) = v.hir().def(def_id) else {
        unreachable!("root of a Trait owner is always OwnerNode::Trait");
    };

    for &id in &trait_.generics {
        v.visit_generic(id);
    }
    for &method in &trait_.functions {
        v.visit_nested_owner(method);
    }
}

pub fn walk_extend<'hir, V: Visitor<'hir>>(v: &mut V, def_id: DefId) {
    let OwnerNode::Extend(extend) = v.hir().def(def_id) else {
        unreachable!("root of an Extend owner is always OwnerNode::Extend");
    };

    // The first group declares parameters; the other two apply arguments.
    for &id in &extend.extend_generics {
        v.visit_generic(id);
    }
    for &id in extend.adt_generics.iter().chain(&extend.trait_generics) {
        v.visit_ty(id);
    }
    v.visit_path(&extend.adt_path);
    if let Some(path) = &extend.trait_path {
        v.visit_path(path);
    }
    for &method in &extend.methods {
        v.visit_nested_owner(method);
    }
}

pub fn walk_closure<'hir, V: Visitor<'hir>>(v: &mut V, def_id: DefId) {
    let OwnerNode::Closure(closure) = v.hir().def(def_id) else {
        unreachable!("root of a Closure owner is always OwnerNode::Closure");
    };

    for &id in &closure.params {
        v.visit_closure_param(id);
    }
    if let Some(id) = closure.ret {
        v.visit_ty(id);
    }
    v.visit_block(closure.block);
}

// -----------------------------------------------------------------
// Declarations nested in an owner
// -----------------------------------------------------------------

pub fn walk_generic<'hir, V: Visitor<'hir>>(v: &mut V, id: HirId) {
    for bound in &v.hir().generic(id).bounds {
        v.visit_path(bound);
    }
}

pub fn walk_param<'hir, V: Visitor<'hir>>(v: &mut V, id: HirId) {
    v.visit_ty(v.hir().param(id).ty);
}

pub fn walk_closure_param<'hir, V: Visitor<'hir>>(v: &mut V, id: HirId) {
    if let Some(ty) = v.hir().closure_param(id).ty {
        v.visit_ty(ty);
    }
}

pub fn walk_field<'hir, V: Visitor<'hir>>(v: &mut V, id: HirId) {
    v.visit_ty(v.hir().field(id).ty);
}

pub fn walk_variant<'hir, V: Visitor<'hir>>(v: &mut V, id: HirId) {
    match &v.hir().variant(id).payload {
        VariantPayload::Unit => {}
        VariantPayload::Type(ty) => v.visit_ty(*ty),
        VariantPayload::Record(fields) => {
            for &id in fields {
                v.visit_field(id);
            }
        }
    }
}

// -----------------------------------------------------------------
// Blocks and statements
// -----------------------------------------------------------------

pub fn walk_block<'hir, V: Visitor<'hir>>(v: &mut V, id: HirId) {
    let block = v.hir().block(id);

    for &id in &block.stmts {
        v.visit_stmt(id);
    }
    if let Some(id) = block.expr {
        v.visit_expr(id);
    }
}

pub fn walk_stmt<'hir, V: Visitor<'hir>>(v: &mut V, id: HirId) {
    match &v.hir().stmt(id).kind {
        StmtKind::Let {
            pat,
            ty,
            init,
            else_block,
            ..
        } => {
            // The initializer is visited before the pattern binds, so that `let x = x;` reads the
            // outer `x` rather than the one being declared.
            v.visit_expr(*init);
            if let Some(ty) = *ty {
                v.visit_ty(ty);
            }
            v.visit_pat(*pat);
            if let Some(block) = *else_block {
                v.visit_block(block);
            }
        }
        StmtKind::With { lends, block } => {
            for lend in lends {
                v.visit_expr(lend.init);
                if let Some(ty) = lend.ty {
                    v.visit_ty(ty);
                }
                v.visit_pat(lend.pat);
            }
            v.visit_block(*block);
        }
        StmtKind::Return(expr) => {
            if let Some(expr) = *expr {
                v.visit_expr(expr);
            }
        }
        StmtKind::Defer(expr) | StmtKind::Expr(expr) => v.visit_expr(*expr),
        StmtKind::Break | StmtKind::Continue | StmtKind::Error => {}
    }
}

pub fn walk_arm<'hir, V: Visitor<'hir>>(v: &mut V, id: HirId) {
    let arm = v.hir().arm(id);

    v.visit_pat(arm.pat);
    if let Some(guard) = arm.guard {
        v.visit_expr(guard);
    }
    v.visit_block(arm.block);
}

// -----------------------------------------------------------------
// Expressions, patterns, types
// -----------------------------------------------------------------

pub fn walk_expr<'hir, V: Visitor<'hir>>(v: &mut V, id: HirId) {
    match &v.hir().expr(id).kind {
        ExprKind::Path(path) => v.visit_path(path),
        ExprKind::Unary { operand, .. }
        | ExprKind::Borrow { operand, .. }
        | ExprKind::Try(operand) => v.visit_expr(*operand),
        ExprKind::Binary { lhs, rhs, .. }
        | ExprKind::Assign { lhs, rhs }
        | ExprKind::AssignOp { lhs, rhs, .. } => {
            v.visit_expr(*lhs);
            v.visit_expr(*rhs);
        }
        ExprKind::Call { callee, args } => {
            v.visit_expr(*callee);
            for &arg in args {
                v.visit_expr(arg);
            }
        }
        ExprKind::Access { base, args, .. } => {
            v.visit_expr(*base);
            match args {
                AccessArgs::None => {}
                AccessArgs::Call(args) => {
                    for &arg in args {
                        v.visit_expr(arg);
                    }
                }
                AccessArgs::Record(fields) => {
                    for field in fields {
                        v.visit_expr(field.value);
                    }
                }
            }
        }
        ExprKind::Index { base, index } => {
            v.visit_expr(*base);
            v.visit_expr(*index);
        }
        ExprKind::Ctor { path, payload } => {
            // `None` for the elided `.{ .. }` form, whose type typeck infers from context.
            if let Some(path) = path {
                v.visit_path(path);
            }
            for field in payload {
                v.visit_expr(field.value);
            }
        }
        ExprKind::Variant { payload, .. } => {
            for value in payload_values(payload) {
                v.visit_expr(value);
            }
        }
        ExprKind::Tuple(elems) => {
            for &elem in elems {
                v.visit_expr(elem);
            }
        }
        ExprKind::Range { lo, hi, .. } => {
            for bound in [*lo, *hi].into_iter().flatten() {
                v.visit_expr(bound);
            }
        }
        ExprKind::If {
            cond,
            then_block,
            else_block,
        } => {
            v.visit_expr(*cond);
            v.visit_block(*then_block);
            if let Some(else_block) = *else_block {
                v.visit_block(else_block);
            }
        }
        ExprKind::Match { scrutinee, arms } => {
            v.visit_expr(*scrutinee);
            for &arm in arms {
                v.visit_arm(arm);
            }
        }
        ExprKind::Loop { block, .. }
        | ExprKind::Spawn(block)
        | ExprKind::Concurrent(block)
        | ExprKind::Block(block) => v.visit_block(*block),
        ExprKind::Closure(def_id) => v.visit_nested_owner(*def_id),
        ExprKind::Cast { expr, ty } => {
            v.visit_expr(*expr);
            v.visit_ty(*ty);
        }
        ExprKind::Literal(_) | ExprKind::Error => {}
    }
}

pub fn walk_pat<'hir, V: Visitor<'hir>>(v: &mut V, id: HirId) {
    match &v.hir().pat(id).kind {
        PatKind::Variant { payload, .. } => {
            for value in payload_values(payload) {
                v.visit_pat(value);
            }
        }
        PatKind::Tuple(elems) => {
            for &elem in elems {
                v.visit_pat(elem);
            }
        }
        PatKind::Wildcard | PatKind::Binding { .. } | PatKind::Literal(_) | PatKind::Error => {}
    }
}

pub fn walk_ty<'hir, V: Visitor<'hir>>(v: &mut V, id: HirId) {
    match &v.hir().ty(id).kind {
        // `Self` is an ordinary single-segment path here; what sets it apart is `path.res`.
        TyKind::Path { path, args } | TyKind::Dyn { path, args } => {
            v.visit_path(path);
            for &arg in args {
                v.visit_ty(arg);
            }
        }
        TyKind::Ref { base, .. } | TyKind::Any(base) | TyKind::Iso(base) => v.visit_ty(*base),
        TyKind::Tuple(elems) => {
            for &elem in elems {
                v.visit_ty(elem);
            }
        }
        TyKind::Array { elem, len } => {
            v.visit_ty(*elem);
            // An array's length is a constant *expression*, not a type.
            if let Some(len) = *len {
                v.visit_expr(len);
            }
        }
        TyKind::Function { params, ret } => {
            for &param in params {
                v.visit_ty(param);
            }
            if let Some(ret) = *ret {
                v.visit_ty(ret);
            }
        }
        TyKind::Error => {}
    }
}

fn payload_values(payload: &Payload) -> Vec<HirId> {
    match payload {
        Payload::None => Vec::new(),
        Payload::Single(value) => vec![*value],
        Payload::Record(fields) => fields.iter().map(|f| f.value).collect(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::hir::{Node, Res};
    use crate::testing::lower_src;

    /// Records the `HirId` of every node the walk reaches, every `DefId` it enters, and every
    /// `Path` it passes to `visit_path`. Overrides `visit_nested_owner` to traverse nested owners,
    /// so that the per-arena comparison below covers all the arenas rather than only the root's.
    struct Recorder<'hir> {
        hir: &'hir Hir,
        visited: Vec<HirId>,
        paths: Vec<&'hir Path>,
        owners: Vec<DefId>,
    }

    impl<'hir> Recorder<'hir> {
        fn walk(hir: &'hir Hir) -> Self {
            let mut r = Recorder {
                hir,
                visited: Vec::new(),
                paths: Vec::new(),
                owners: Vec::new(),
            };
            r.visit_module(hir.root_id());
            r
        }

        /// Every node in every arena except slot zero of each, which contains the owner. An owner
        /// is accessed by `DefId` through an owner hook, not by `HirId` through a child list, so
        /// it never appears in `visited`.
        fn every_child_node(hir: &Hir) -> Vec<HirId> {
            hir.def_ids()
                .flat_map(|def| hir.arena(def).nodes.iter().skip(1).map(Node::hir_id))
                .collect()
        }
    }

    impl<'hir> Visitor<'hir> for Recorder<'hir> {
        fn hir(&self) -> &'hir Hir {
            self.hir
        }

        /// Enters every nested owner, since the exhaustiveness check runs over all the arenas
        /// and each one must be visited from some entry point. `walk_item` dispatches to the owner
        /// hook below, which records it.
        fn visit_nested_owner(&mut self, def_id: DefId) {
            walk_item(self, def_id);
        }

        fn visit_path(&mut self, path: &'hir Path) {
            self.paths.push(path);
        }

        fn visit_module(&mut self, def_id: DefId) {
            self.owners.push(def_id);
            walk_module(self, def_id);
        }
        fn visit_function(&mut self, def_id: DefId) {
            self.owners.push(def_id);
            walk_function(self, def_id);
        }
        fn visit_struct(&mut self, def_id: DefId) {
            self.owners.push(def_id);
            walk_struct(self, def_id);
        }
        fn visit_enum(&mut self, def_id: DefId) {
            self.owners.push(def_id);
            walk_enum(self, def_id);
        }
        fn visit_trait(&mut self, def_id: DefId) {
            self.owners.push(def_id);
            walk_trait(self, def_id);
        }
        fn visit_extend(&mut self, def_id: DefId) {
            self.owners.push(def_id);
            walk_extend(self, def_id);
        }
        fn visit_closure(&mut self, def_id: DefId) {
            self.owners.push(def_id);
            walk_closure(self, def_id);
        }

        fn visit_generic(&mut self, id: HirId) {
            self.visited.push(id);
            walk_generic(self, id);
        }
        fn visit_self_param(&mut self, id: HirId) {
            self.visited.push(id);
        }
        fn visit_import(&mut self, id: HirId) {
            self.visited.push(id);
        }
        fn visit_param(&mut self, id: HirId) {
            self.visited.push(id);
            walk_param(self, id);
        }
        fn visit_closure_param(&mut self, id: HirId) {
            self.visited.push(id);
            walk_closure_param(self, id);
        }
        fn visit_field(&mut self, id: HirId) {
            self.visited.push(id);
            walk_field(self, id);
        }
        fn visit_variant(&mut self, id: HirId) {
            self.visited.push(id);
            walk_variant(self, id);
        }
        fn visit_block(&mut self, id: HirId) {
            self.visited.push(id);
            walk_block(self, id);
        }
        fn visit_stmt(&mut self, id: HirId) {
            self.visited.push(id);
            walk_stmt(self, id);
        }
        fn visit_arm(&mut self, id: HirId) {
            self.visited.push(id);
            walk_arm(self, id);
        }
        fn visit_expr(&mut self, id: HirId) {
            self.visited.push(id);
            walk_expr(self, id);
        }
        fn visit_pat(&mut self, id: HirId) {
            self.visited.push(id);
            walk_pat(self, id);
        }
        fn visit_ty(&mut self, id: HirId) {
            self.visited.push(id);
            walk_ty(self, id);
        }
    }

    /// Covers every `Node` variant and every child field the walk reads: both `if` branches, a
    /// `let` with an annotation, a `with` lend, a `match`, a closure, a `defer`, an array length
    /// expression, all three `Payload` shapes, and all three `VariantPayload` shapes.
    const EVERYTHING: &str = r#"
        struct Pair<T> { fst: T, snd: i32 }

        enum Shape { unit, circle: f64, square: { l: f64 } }

        trait Draw { fun draw(&self) -> i32; }

        extend<T> Pair<T> with Draw {
            fun draw(&self) -> i32 { return self.snd; }
        }

        fun everything<T: Draw>(p: Pair<i32>, n: i32, arr: [i32; 4], f: fun(i32) -> i32) -> i32 {
            let mut total: i32 = n + 1;
            let q = Pair { fst: 1, snd: 2 };
            let s = Shape.circle(1.0);
            let t = Shape.square { l: 2.0 };
            let u = .{ fst: 3, snd: 4 };
            let c = &total;
            let d = |x: i32| { x + 1 };
            let e = arr[0];
            let g = 1..2;
            let h = f(n)?;
            with lent = &total { total = total + 1; }
            match s {
                .circle(r) => 1,
                .square { l } => 2,
                _ => 3,
            }
            if total > 0 { total = total + 1; } else { total = total - 1; }
            while total > 0 { break; }
            for i in arr { continue; }
            spawn { total = 1; }
            concurrent { total = 2; }
            defer total = 0;
            return p.draw();
        }
    "#;

    /// Every `HirId` allocated in an arena is reached by the walk. A child field omitted from a
    /// `walk_*` function results in test failure, catching a subtree that one pass would traverse
    /// and another would skip.
    #[test]
    fn the_walk_reaches_every_node_in_every_arena() {
        let hir = lower_src(EVERYTHING);
        let recorder = Recorder::walk(&hir);

        let mut visited = recorder.visited.clone();
        visited.sort_by_key(|id| (id.owner.index(), id.local_id.index()));
        let mut expected = Recorder::every_child_node(&hir);
        expected.sort_by_key(|id| (id.owner.index(), id.local_id.index()));

        let missed: Vec<_> = expected
            .iter()
            .filter(|id| !visited.contains(id))
            .map(|&id| (id, hir.node(id).kind_name()))
            .collect();
        assert!(missed.is_empty(), "walk never reached: {missed:?}");
    }

    /// Every `DefId` is entered. Without this, the check above would pass vacuously for an arena
    /// the walk never entered: `visited` would hold none of its nodes, but neither would the
    /// comparison, since `Recorder` collects both from what it reached.
    #[test]
    fn the_walk_enters_every_owner() {
        let hir = lower_src(EVERYTHING);
        let recorder = Recorder::walk(&hir);

        let missed: Vec<_> = hir
            .def_ids()
            .filter(|def| !recorder.owners.contains(def))
            .map(|def| (def, hir.def(def).kind_name()))
            .collect();
        assert!(missed.is_empty(), "walk never entered: {missed:?}");
    }

    /// No `HirId` is visited twice. A node listed in two child fields would be checked twice by
    /// `Typeck` and lowered twice by MIR lowering.
    #[test]
    fn the_walk_reaches_each_node_exactly_once() {
        let hir = lower_src(EVERYTHING);
        let recorder = Recorder::walk(&hir);

        let mut seen = recorder.visited.clone();
        seen.sort_by_key(|id| (id.owner.index(), id.local_id.index()));
        let duplicates: Vec<_> = seen.windows(2).filter(|w| w[0] == w[1]).collect();
        assert!(duplicates.is_empty(), "visited twice: {duplicates:?}");
    }

    /// `visit_path` fires for the path positions unreachable through `visit_expr`: a generic
    /// parameter's bound and an `extend` header's `adt_path` and `trait_path`.
    #[test]
    fn visit_path_fires_for_paths_outside_expressions() {
        let hir = lower_src(EVERYTHING);
        let recorder = Recorder::walk(&hir);

        let named: Vec<String> = recorder
            .paths
            .iter()
            .map(|p| {
                p.segments
                    .iter()
                    .map(|s| crate::ast::interner::Interner::resolve(s.text))
                    .collect::<Vec<_>>()
                    .join("::")
            })
            .collect();

        // `Draw` is written twice as something other than an expression: once as the bound on
        // `T`, once as the trait of the `extend` header. Neither is reachable through `visit_expr`.
        assert!(
            named.iter().filter(|n| *n == "Draw").count() >= 2,
            "expected the generic bound and the extend header's trait path, got {named:?}"
        );
        // The `extend` header's self type, likewise reachable only through the header.
        assert!(named.iter().any(|n| n == "Pair"), "got {named:?}");
    }

    /// A `Visitor` that does not override `visit_nested_owner` sees the closure's `DefId`
    /// but none of the nodes in the closure's arena.
    #[test]
    fn the_walk_stops_at_a_nested_owner_unless_the_pass_follows_it() {
        struct Shallow<'hir> {
            hir: &'hir Hir,
            owners: Vec<DefId>,
            blocks: usize,
        }
        impl<'hir> Visitor<'hir> for Shallow<'hir> {
            fn hir(&self) -> &'hir Hir {
                self.hir
            }
            fn visit_nested_owner(&mut self, def_id: DefId) {
                self.owners.push(def_id);
            }
            fn visit_block(&mut self, id: HirId) {
                self.blocks += 1;
                walk_block(self, id);
            }
        }

        let hir = lower_src("fun f() { let g = |x: i32| { x + 1 }; }");
        let function = crate::testing::first_function(&hir);
        let mut v = Shallow {
            hir: &hir,
            owners: Vec::new(),
            blocks: 0,
        };
        v.visit_function(function);

        // The function's own body, and not the closure's.
        assert_eq!(v.blocks, 1);
        assert_eq!(v.owners.len(), 1, "the closure is offered, not entered");
        assert!(matches!(hir.def(v.owners[0]), OwnerNode::Closure(_)));
    }

    /// `ExprKind::If::else_block` is a `Node::Block`, so `walk_expr` must reach it with
    /// `visit_block`. Calling `visit_expr` on it panics in `Hir::expr`, which is what the
    /// pre-visitor name resolution did on every `if`/`else`.
    #[test]
    fn an_else_branch_is_walked_as_a_block() {
        let hir = lower_src("fun f(c: bool) { if c { let a = 1; } else { let b = 2; } }");
        let recorder = Recorder::walk(&hir);

        let blocks = recorder
            .visited
            .iter()
            .filter(|&&id| matches!(hir.node(id), Node::Block(_)))
            .count();
        // The function body, the `then` block, and the `else` block.
        assert_eq!(blocks, 3);
    }

    /// `StmtKind::Let`'s `ty` and `else_block` are both optional fields that the pre-visitor
    /// name resolution omitted from its walk entirely.
    #[test]
    fn a_let_annotation_and_else_block_are_walked() {
        let hir = lower_src("fun f() { let x: i32 = 1 else { let y = 2; }; }");
        let recorder = Recorder::walk(&hir);

        assert!(
            recorder
                .visited
                .iter()
                .any(|&id| matches!(hir.node(id), Node::Ty(_))),
            "the `: i32` annotation was never walked"
        );
        let blocks = recorder
            .visited
            .iter()
            .filter(|&&id| matches!(hir.node(id), Node::Block(_)))
            .count();
        // The function body and the `else` block.
        assert_eq!(blocks, 2, "the let-else block was never walked");
    }

    /// `TyKind::Array::len` addresses a `Node::Expr`, not a `Node::Ty` -- the only edge from the
    /// type walk back into the expression walk.
    #[test]
    fn an_array_length_is_walked_as_an_expression() {
        let hir = lower_src("fun f(a: [i32; 4]) {}");
        let recorder = Recorder::walk(&hir);

        assert!(
            recorder
                .visited
                .iter()
                .any(|&id| matches!(hir.node(id), Node::Expr(_))),
            "the array length expression was never walked"
        );
    }

    /// The `&'hir Path` handed to `visit_path` is the one stored in the arena, with `res`
    /// intact, rather than a copy rebuilt from `segments`.
    #[test]
    fn visit_path_carries_the_resolution_lowering_attached() {
        let hir = lower_src("fun f(n: i32) -> i32 { return n; }");
        let recorder = Recorder::walk(&hir);

        assert!(
            recorder
                .paths
                .iter()
                .any(|p| matches!(p.res, Res::Local(_))),
            "no path resolved to the parameter `n`: {:?}",
            recorder.paths.iter().map(|p| p.res).collect::<Vec<_>>()
        );
    }
}
