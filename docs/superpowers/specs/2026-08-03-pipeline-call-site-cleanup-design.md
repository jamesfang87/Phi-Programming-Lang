# Pipeline call-site cleanup

Date: 2026-08-03
Status: implemented, with one correction noted under section 3

## Problem

The compiler's passes were written one at a time, and each settled on its own conventions.
Passes that do structurally identical work — walk the tree, record a fact per node — do not look
alike, so knowledge of one does not transfer to the next. Concretely:

- Name resolution addresses a node as `(DefId, LocalId)`; type checking addresses the same node
  as a `HirId`. HIR nodes themselves reference their children by `LocalId`, so a pass holding a
  node must rebuild the full address of every child it wants to visit. The result is 52
  `HirId { owner, local_id }` struct literals across 14 files.
- Both passes produce a `HirId -> X` side table, but the two tables live in differently-named
  files, are named on different schemes, and expose different accessor vocabularies.
- One concept carries several names across passes: a generic type parameter is `Res::TyParam`,
  `TyKind::Generic`, and `hir::Generic`. `SelfParam` names two unrelated things.
- `Compiler::build` takes four positional `bool`s, so no call site says which flag is which.
- Test setup is copy-pasted across four modules, with a comment admitting it
  (`src/typeck.rs:656`).

Two further problems are structural rather than cosmetic:

- **The type table and the unifier can drift.** `check_expr` returns a `Ty` and it is the
  caller's job to record it; nothing enforces that, and arms already exist that compute a type
  and drop it. Separately, a recorded type may be an inference variable that later unifies with
  something concrete — the table still holds the variable, so every reader must remember to route
  through the unifier or report `{integer}` where it means `i32`.
- **Blocks and expressions are ambiguously nested.** An expression may contain a block
  (`ExprKind::Block`) and a block may contain an expression (`StmtKind::Expr`), and different
  constructs pick different sides for no principled reason. `If`'s two arms are different node
  kinds: `then_branch` is a `Block`, `else_branch` is an `Expr`. `body` means a `Block` in
  `Function`, `Loop`, and `With`, but an `Expr` in `Arm` and `Closure`. `else_branch` means an
  `Expr` in `If` but a `Block` in `LetStmt`.

## Goals

Make similar work look similar, so that a reader who has understood one pass can predict the
next. Make the two known drift hazards structurally impossible rather than merely documented.

## Non-goals

- Moving both passes under a `src/sema/` parent. The pairing is real but the churn to every use
  path in the crate is not worth it.
- A `Stage` trait over lex/parse/lower/resolve/check. Five stages do not justify the machinery.
- Changing `emit_debug`'s formatting. Its structural dump is deliberately distinct from the
  user-facing one and stays that way.
- Renaming `typeck/lower_ty.rs`. "Lowering" a written annotation to a semantic type is the
  standard term for what it does.
- Touching `symbol_table`'s `lookup_*` family, which is already internally consistent.
- Hunting for bugs. This is a structure and naming change; behaviour is preserved except where
  section 5 explicitly makes a stale read impossible.

## Section 1 — Block uniformity and AST/HIR parity

**Rule.** Every construct that owns executable code owns a `Block`, never a bare `Expr`. Where
the AST and the HIR represent the same thing with no desugaring between them, they use the same
names.

### Structural changes to the HIR

| Node | Today | After |
| --- | --- | --- |
| `Arm` (`src/hir/pat.rs:56`) | `{ pat, body }` where `body -> Node::Expr` | `{ pat, block }` where `block -> Node::Block` |
| `Closure` (`src/hir/items.rs:135`) | `{ params, ret, body }` where `body -> Node::Expr` | `{ params, ret, block }` where `block -> Node::Block` |
| `ExprKind::If` (`src/hir/expr.rs:88`) | `{ cond, then_branch -> Block, else_branch -> Expr }` | `{ cond, then_block -> Block, else_block -> Block }` |

Lowering synthesizes the wrapping block for an expression-bodied arm or closure, using the
existing `OwnerBuilder::synth_block` (`src/hir/lower/owner.rs:101`). An `else if` chain lowers to
`else { if .. }` — a block whose tail expression is the nested `if` — so a chain of any length is
uniform instead of `else_branch` being sometimes an `If` and sometimes a `Block`.

Typeck consequently loses its separate arm and closure body paths; both call `check_block` like
every other construct.

### Renames by node kind

A field pointing at a `Node::Block` is named `block` (or `<role>_block`). Applies to:

- `hir::Function.body` -> `block`
- `hir::ExprKind::Loop.body` -> `block`
- `hir::StmtKind::With.body` -> `block`
- `hir::LetStmt.else_branch` -> `else_block`
- `hir::ExprKind::Spawn(..)` and `Concurrent(..)` — document the payload as `block`

The same renames apply to the AST's corresponding fields (`ast::Function.body`,
`ast::StmtKind::While/WhileLet/For.body`, `ast::StmtKind::With.body`,
`ast::DeclStmt.else_branch`), since none of them involve desugaring.

### AST/HIR parity

| AST today | HIR today | After (both) |
| --- | --- | --- |
| `ExprKind::DeclRef(Path)` | `ExprKind::Path(Path)` | `ExprKind::Path(Path)` |
| `ExprKind::FunCall { callee, args }` | `ExprKind::Call { callee, args }` | `ExprKind::Call { callee, args }` |
| `CtorPayload { name, expr, span }` and `PayloadField<T> { name, value, span }` | `(Ident, LocalId)` in `Ctor.payload`, `Payload::Record`, `AccessArgs::Record` | AST keeps only `PayloadField<T> { name, value, span }`; HIR gains a named `FieldExpr { name, expr }` replacing the three bare tuples |

`hir::Block { stmts, expr }` keeps its split-out tail while `ast::Block { stmts }` keeps the tail
as a `StmtKind::Expr { expr, semi: false }`. Splitting the tail is a genuine desugaring, so the
rule permits the difference, and typeck reads a block's value off one field instead of inspecting
the last statement's `semi` flag. `ast::Payload<T>`'s generic parameter versus `hir::Payload`'s
absence of one stays for the same reason: a HIR id erases the Expr/Pat distinction that the AST
still needs.

### Misnomers and stutters

- `ast::StmtKind::Return { ret: Expr }` -> `Return(Expr)`
- `ast::StmtKind::Defer { defer: Expr }` -> `Defer(Expr)`
- `ast::StmtKind::For { name: Pat, .. }` -> `For { pat, .. }` (it holds a pattern, not a name)
- `ast::DeclStmt.name: Pat` -> `pat`, and `ast::WithStmtLend.name: Pat` -> `pat`, for the same
  reason
- `ast::TyKind::Base { base: Path, args }` -> `Path { path, args }`

## Section 2 — Naming in name resolution and type checking

### Result types

| Today | After |
| --- | --- |
| `NameResolverResults` in `src/nameres/resolve_results.rs` | `NameResolutions` in `src/nameres/results.rs` |
| `TypeckResults` in `src/typeck/tyres.rs` | `TypeResolutions` in `src/typeck/results.rs` |

`resolve_results.rs` sat in the same `resolve_*` namespace as the walk modules without being one;
`tyres` is opaque. After the move the two passes mirror each other, file and type.

Note the deliberate tension: "resolution" now names both what a path refers to and what a node's
type is. That is accepted for the symmetry, and paid for by never using the word for a third
thing — see `Unifier::root` below.

### Accessor vocabulary

Writers are `record_*`, readers are named for what they return, iterators are `iter_*`.

```
NameResolutions::record(id, res)          TypeResolutions::record(id, ty)
               ::res(id) -> Option<Res>                  ::record_def(def, ty)
               ::record_self_ty / ::self_ty              ::ty(id) -> Option<Ty>
               ::record_generics / ::generic             ::iter()
               ::iter() / ::iter_self_tys / ::iter_generics
```

This replaces `add` / `get` / `self_tys_iter` / `generics_iter` / `add_def`. A bare `get` says
nothing about what comes back, and `iter` alongside `*_iter` was inconsistent within one type.

### Concept names

| Today | After | Reason |
| --- | --- | --- |
| `Res::TyParam` | `Res::Generic` | `hir::Generic` and `TyKind::Generic` already use this word |
| `TyKind::SelfParam` | `TyKind::SelfTy` | `SelfParam` currently names both a method receiver and the `Self` type; this frees it for the receiver, and matches `Res::SelfTy` |
| `Unify` (struct), field `unify` | `Unifier`, field `unifier` | Stops call sites reading `self.unify.unify(..)` |
| `Unify::find` | `Unifier::root` | Drops union-find jargon. Deliberately not `resolve`, so that word keeps one meaning per pass |
| `lower_ty::expect_no_args(.., what: &str)` | `.., kind: &str` | — |

### Parameter conventions

Across both passes: `id: HirId` for the node being visited, `def: DefId` for a definition,
`owner: DefId` only where it is specifically the arena owner. No `_id` suffix when the type
already says it (`def: DefId`, not `def_id: DefId`).

### Documentation

`src/typeck.rs:5`, `src/typeck.rs:41`, and `src/typeck/tyres.rs:14` link to `[`collect`]` and
`crate::typeck::collect`, which does not exist — the entry point is `check`, and `collect` is the
name of its first stage's methods. Repoint them at `typeck::check` and `Typeck::collect_module`,
and state plainly that the pass runs `collect_*` over every signature before `check_*` over any
body.

## Section 3 — Node addressing

**HIR nodes reference their children by `HirId`, not `LocalId`.** Every node already stores its
own `hir_id` — 20 such fields across the node structs — so a pass that holds a node holds fully
formed addresses for all of its children, and constructs nothing.

```rust
// before
pub struct Block {
    pub hir_id: HirId,
    pub stmts: Vec<LocalId>,   // -> Node::Stmt
    pub expr: Option<LocalId>, // -> Node::Expr
    pub span: SrcSpan,
}

// after
pub struct Block {
    pub hir_id: HirId,
    pub stmts: Vec<HirId>,   // -> Node::Stmt
    pub expr: Option<HirId>, // -> Node::Expr
    pub span: SrcSpan,
}
```

The same substitution applies to every `LocalId` field in `src/hir/expr.rs`, `block.rs`, `pat.rs`,
`types.rs`, and `items.rs`. `ExprKind::Closure(DefId)` is unaffected: it names an owner, not a
node.

At the call site this removes the construction step entirely:

```rust
// before
let stmt = HirId { owner: block_id.owner, local_id: stmt };
self.check_stmt(stmt);

// after
self.check_stmt(stmt);
```

`LocalId` survives as the arena's internal index — `Arena` is a `Vec<Node>` and `Hir::node` still
resolves a `HirId` by indexing `arena(id.owner)` with `id.local_id`. `LocalId::OWNER` also stays,
since `TypeResolutions::record_def` uses it to key a definition's own type. What changes is that
`LocalId` no longer appears in any node field or any pass's signature.

### Building

`ArenaBuilder` and `OwnerLowerer` already know their `DefId`, so they mint finished addresses:

```rust
// before                          // after
fn reserve(&mut self) -> LocalId   fn reserve(&mut self) -> HirId
fn fill(&mut self, id: LocalId, node: impl Into<Node>)
                                   fn fill(&mut self, id: HirId, node: impl Into<Node>)
```

`OwnerLowerer::hir_id(LocalId) -> HirId` disappears — there is nothing left to convert.

### Name resolution's signatures

```rust
// before
pub fn resolve_expr(&mut self, owner_id: DefId, expr_id: LocalId)
pub fn resolve_ty(&mut self, owner_id: DefId, ty_id: LocalId)
pub fn bind_pat(&mut self, owner_id: DefId, pat_id: LocalId)

// after
pub fn resolve_expr(&mut self, id: HirId)
pub fn resolve_ty(&mut self, id: HirId)
pub fn bind_pat(&mut self, id: HirId)
```

Internal uses of `owner_id` become `id.owner`. `resolve_value_path` and `resolve_ty_path` keep
taking a `DefId`: they resolve a path *within* an owner rather than visiting a node, so `DefId` is
the honest parameter.

### Same-arena checking

`LocalId` fields made "a child lives in its parent's arena" true by type. `HirId` fields do not, so
that invariant is restored by two assertions, both cheap:

```rust
// src/hir/builder.rs — nothing foreign gets stored
pub fn fill(&mut self, id: HirId, node: impl Into<Node>) {
    debug_assert_eq!(id.owner, self.def_id(), "node filled into another owner's arena");
    ...
}

// src/hir.rs — nothing foreign gets followed
pub fn node(&self, id: HirId) -> &Node {
    let node = self.arena(id.owner).get(id.local_id);
    debug_assert_eq!(node.hir_id(), id, "HirId does not address the node it names");
    node
}
```

The second is the load-bearing one, and it is what makes this affordable: because every node
stores its own `hir_id`, a wrong-owner child reference is caught the first time anyone follows it,
without a line of per-node-kind code. It needs one new accessor, `Node::hir_id(&self) -> HirId` —
a single match over the `Node` enum — which the debug dump can use as well.

**Correction, found while implementing.** The claim above is wrong. `Hir::node` selects the arena
*from* `id.owner`, so following a child id that names a foreign arena lands on a real node in that
arena, whose stored `hir_id` is exactly the id that was followed. Nothing disagrees and the
assertion passes. A `HirId` is fully self-describing, so it cannot be checked against itself.

Both assertions are kept, for what they actually do. `fill`'s is the real one: it is the single
point where a child id enters a node, so it is where a foreign owner can be rejected. `Hir::node`'s
checks that an arena agrees with itself — that the node at a slot is the one whose own id names
that slot — which catches a node filled in under an id other than the one it was built with.
Genuine cross-arena child detection would need parent context at the store site, i.e. the
per-node-kind `children()` walk ruled out below.

Both are `debug_assert!`, so release builds pay nothing. Out of scope, deliberately: an exhaustive
debug-only validation pass over every stored id at the end of lowering. That would catch an id
that is stored but never followed, at the cost of a per-node-kind `children()` walk over every
`ExprKind`, `StmtKind`, and `PatKind` variant — real complexity for a case that, by definition,
nothing depends on. If a bad id ever does matter, it gets followed, and the assert in `Hir::node`
fires.

## Section 4 — Test scaffolding

New `#[cfg(test)] mod testing` at `src/testing.rs`, declared from `src/main.rs`:

```rust
pub fn lex_src(src: &str) -> (Vec<Token>, u32);
pub fn parse_src(src: &str) -> ParsedSrcFile;
pub fn lower_src(src: &str) -> Hir;
pub fn resolve_src(src: &str) -> (Hir, NameResolutions);
```

Each clears `DiagCtx` and `Interner`, registers the source under `<test>` via `SrcMap::add_file`,
runs the pipeline to that point, and asserts no diagnostics were raised. Each builds on the one
before it rather than repeating the earlier stages.

Alongside them, the HIR-digging helpers that both test modules have grown copies of:
`first_function`, `find_return`, `first_struct`, `first_trait`, `first_extend_method`,
`find_value`.

This removes the duplicated `build()` at `src/typeck.rs:657`, `lower_src` at
`src/hir/lower/tests.rs:22`, and the three near-identical preludes in `src/parser.rs` (lines 217,
238, 254). Tests that deliberately assert on diagnostics keep driving the pipeline by hand, since
the helpers assert the absence of diagnostics.

## Section 5 — Linking the type table and the unifier

`TypeResolutions` stays a map and `Unifier` stays a union-find. Neither learns about the other.
`Typeck` owns the pairing, through a single accessor that is the only place a type is ever
recorded:

```rust
/// The type of `id`, computed on first use and remembered afterwards.
///
/// This is the only place a type enters `TypeResolutions`, and the only way one is read, so a
/// node cannot be checked without its type being recorded, and a recorded type cannot be read
/// while it is still an unresolved inference variable.
fn ty_of(&mut self, id: HirId) -> Ty {
    if let Some(ty) = self.types.ty(id) {
        return self.unifier.root(ty);
    }
    let ty = self.check_expr(id);
    self.types.record(id, ty);
    ty
}
```

`check_expr` becomes private and `#[must_use]`; it computes a type and records nothing. Every
existing `self.check_expr(..)` call site becomes `self.ty_of(..)`.

After each body is checked, `writeback(owner: DefId)` walks that owner's entries and replaces each
with its deeply-resolved form, so the finished table contains no unresolved variables reachable
from a pinned-down one. Downstream consumers — `emit_debug::print_typeck` included — then read the
table without needing the `Unifier` at all.

`ty_of` closes the "forgot to record" hazard by construction. `root`-on-read closes the stale-read
hazard within the pass. `writeback` closes it for everything after the pass.

## Section 6 — Diagnostic formatting

`src/typeck/display.rs` currently threads `(hir, tcx)` through every level of a recursive
`write_ty`. Replace with a context plus a trait:

```rust
pub struct DisplayCx<'a> { hir: &'a Hir, tcx: &'a TyCtx }

pub trait Pretty {
    fn pretty(&self, f: &mut fmt::Formatter<'_>, cx: &DisplayCx<'_>) -> fmt::Result;
}

impl<'a> DisplayCx<'a> {
    pub fn show<T: Pretty>(&self, value: T) -> impl fmt::Display + '_;
}
```

`Ty` and `UnifyError` both implement `Pretty`. Implementing it for `UnifyError` moves each
variant's wording next to the variant and deletes `Typeck::message` (`src/typeck.rs:546`).
`Typeck::display_ty` becomes `Typeck::cx() -> DisplayCx<'_>`; the roughly 25 display assertions in
`src/typeck.rs` become `cx.show(ty).to_string()`.

`emit_debug::fmt_ty` is untouched. It dumps a type's internal structure for `--debug` and is
deliberately a separate path from the surface syntax users see in diagnostics.

## Section 7 — Driver surface

```rust
pub struct BuildOptions {
    pub dumps: Dumps,        // ast | hir | nameres | typeck
    pub exclude_core: bool,
}

impl BuildOptions {
    pub fn from_args(args: &[String]) -> Result<Self, String>;
}

impl Compiler {
    pub fn build(&mut self, root: &Path, options: &BuildOptions) -> io::Result<bool>;
}
```

`--debug` sets every dump flag at parse time, so `build` stops re-deriving `print_ast || debug` at
each stage. `BuildOptions::from_args` also owns the unknown-argument check that is currently
inline in `main.rs:70`.

`typeck::check` returns `TypeckOutput { tcx, types }` instead of a bare tuple — a `Ty` is
meaningless without the `TyCtx` that interned it, and a named struct says so.

`Compiler::lex` and `Compiler::parse` take `&mut self` without using it; they become associated
functions.

## Implementation order

1. Section 1 — structural HIR changes and AST/HIR parity. First, because it changes which fields
   exist, so section 3's substitution runs over the final field set instead of converting fields
   this section is about to delete.
2. Section 2 — naming. Before the mechanical passes, so those land on final names and nothing is
   renamed twice.
3. Section 3 — node addressing. Sweeping but mechanical, and the compiler finds every site.
4. Section 4 — shared test helpers, once the churn from 1–3 has settled.
5. Section 5 — the typeck choke point, which depends on the final names from 2 and addressing
   from 3.
6. Section 6 — diagnostic formatting.
7. Section 7 — driver surface.

## Risks

Section 1 is the only part that changes program structure rather than surface. It will churn
`src/hir/lower/tests.rs` and the golden snapshots under `tests/`. Expect the `--hir` and `--debug`
dumps to change shape for arms, closures, and `if`/`else` chains; the snapshots need regenerating
and the regenerated output needs reading, not just accepting.

Wrapping arm and closure bodies in synthetic blocks adds one HIR node per arm and per closure.
That is a deliberate trade: uniform traversal in exchange for a slightly larger tree.

Section 3 gives up a type-level guarantee. With `LocalId` fields, a child could not name a node in
another arena; with `HirId` fields it can, and the two `debug_assert!`s catch it at build or on
first lookup rather than at compile time. It also doubles a child reference from four bytes to
eight. Both are accepted for the removal of every construction site.

Section 3 touches essentially every file that reads the HIR. It is mechanical and type-directed —
the compiler points at each site — but the diff is large and should land on its own so it stays
reviewable.

## Verification

Each section is complete when `cargo build`, `cargo test`, and `cargo clippy` are clean and the
golden tests under `tests/` pass — with any snapshot change reviewed by hand rather than blanket
re-accepted. Sections 1, 3, and 5 additionally need new tests:

- an expression-bodied arm and closure lower to a block whose tail is that expression, and an
  `else if` chain lowers to nested blocks (section 1);
- a `HirId` built against the wrong owner trips the `Hir::node` assertion, confirming the check
  actually fires rather than being dead code (section 3);
- a node's type read back after a later unification reflects the unified type rather than the
  inference variable originally recorded, and the table after `writeback` holds no unresolved
  variables (section 5).
