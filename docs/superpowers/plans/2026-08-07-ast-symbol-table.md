# AST-Level Symbol Table Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Build the AST-level `SymbolTable` and `NameResolutions` specified in `docs/superpowers/specs/2026-08-07-ast-symbol-table-design.md`, so name resolution can run on the AST before HIR lowering.

**Architecture:** A new `src/nameres/surface/` module holds the AST-level resolver: `Res`/`Type`/`TyDef`/`Local` (resolution results), `SymbolTable<'ast>` (three scope stacks + per-module namespaces), and `NameResolutions` (a `NodeId -> [(Path, Res)]` side table). Two AST changes land first against live code: `TyKind::SelfType` is deleted so `Self` parses as an ordinary path, and `Path` gains segment-only equality. The surface resolver is then built alongside the existing HIR-based resolver in `src/nameres/`, which keeps running the pipeline unchanged.

**Tech Stack:** Rust 2024, `chumsky` 0.13 (parser), `ariadne` 0.6 (diagnostics), `smallvec` (added in Task 3).

## Global Constraints

- **The existing HIR-based resolver stays live and passing through Task 11.** `src/nameres/{symbol_table,results,resolve_*}.rs` and `src/driver/pipeline.rs` are not rewired until Task 12. Only Tasks 1, 2, and 9 touch or extend live-code modules before then; Tasks 3-8, 10, and 11 add new code alongside it. **Tasks 12-15 then perform the migration and delete the old resolver.** Do not delete or re-point any part of the HIR-side resolver before Task 15 — including `SelfTyRes`, which is load-bearing until then (see Task 1).
- **No throwaway scaffolding.** If a task seems to need a temporary structure that Tasks 12-15 would delete, that is a signal the work belongs in Tasks 12-15 instead. Stop and report rather than building it.
- **New code lives under `src/nameres/surface/`.** Inside that module, types are named exactly as the spec names them (`SymbolTable`, `Res`, `NameResolutions`). Module qualification (`surface::NameResolutions` vs `results::NameResolutions`) is what keeps the two apart while both exist. Do not rename the spec's types.
- **`NodeId` values must never appear in any debug/golden output, and dumps must be ordered by source span, not by `NodeId`.** `NodeId` comes from a global atomic counter (`src/ast/node_id.rs:14`) and is only deterministic while parsing is sequential.
- **`Res::Err` is recorded, never omitted.** Absence in `NameResolutions` must mean "never reached". Every failed resolution records `Res::Err` and emits exactly one diagnostic.
- **Diagnostic message text is fixed** and must match these strings exactly:
  - `cannot find \`{name}\` in this scope`
  - `the name \`{name}\` is defined multiple times`
  - `ambiguous import: \`{name}\` refers to more than one item`
  - `\`{name}\` shadows a built-in type`
  - `\`dyn\` requires a trait`
  - `\`Self\` is not available here`
- **All diagnostics go through `DiagCtx::emit`** with an `ariadne`-backed `Diagnostic`, matching the helpers at `src/nameres/symbol_table.rs:425-469`.
- **Every task ends green:** `cargo build`, `cargo test`, and `cargo fmt --check` all pass before commit.
- **Clippy must not regress.** `master` already fails `cargo clippy -- -D warnings` with ~23 pre-existing errors, so "clippy passes" is not achievable and is not the bar. The bar is that your change introduces **no new** clippy findings: capture the baseline (`git stash` your work, run clippy, restore) and diff. Report the comparison. Cleaning up pre-existing findings is out of scope for every task in this plan.
- **Doc comments match house style:** the codebase writes substantial `///` docs explaining *why* a design choice was made, not just what the code does. Match the density of `src/nameres/symbol_table.rs` and `src/hir.rs`.

---

## File Structure

**Modified (live code, Tasks 1-2 only):**
- `src/ast.rs` — delete `TyKind::SelfType`; add `PartialEq`/`Eq`/`Hash` to `Path`
- `src/parser/type_parser.rs` — `Self` parses as `TyKind::Path`
- `src/hir/types.rs`, `src/hir/lower/ty.rs`, `src/hir/visit.rs` — delete the HIR `SelfType` variant and its arms
- `src/nameres/resolve_ty.rs` — resolve the `Self` symbol against `SelfTyRes`
- `src/typeck/lower_ty.rs` — delete its `SelfType` arm

**Created (new code, Tasks 3-11):**
- `src/ast/visit.rs` — the shared AST visitor (Task 9)
- `src/nameres/surface.rs` — module root, re-exports, and the `resolve(&Ast)` entry point
- `src/nameres/surface/res.rs` — `Res`, `Type`, `TyDef`, `Local`
- `src/nameres/surface/results.rs` — `NameResolutions`
- `src/nameres/surface/symbol_table.rs` — `SymbolTable<'ast>`, `ModuleScope`, construction, lookup
- `src/nameres/surface/diagnostics.rs` — the six reporters
- `src/nameres/surface/resolver.rs` — the AST walk that populates `NameResolutions`
- `src/nameres/surface/tests.rs` — unit tests

---

## Task 1: Remove `TyKind::SelfType` from the AST

`Self` becomes an ordinary **AST** `TyKind::Path` whose single segment is the symbol `Self`, so the new AST resolver can treat it like any other name. HIR lowering recognizes that path and maps it back to `HirTyKind::SelfType`.

**`HirTyKind::SelfType` stays.** So do `SelfTyRes`, `src/nameres/resolve_ty.rs`, and every consumer in `typeck`. The spec's "this deletes the `self_tys` side table outright" describes the end state *after* the follow-up rewiring, which is out of scope here — this plan's Global Constraints require the HIR resolver to stay live and passing.

Deleting `HirTyKind::SelfType` in this task would break two things that `src/typeck/lower_ty.rs:100-125` does and a plain `TypeRes::Def(adt)` cannot:

1. `lower_base`'s `TypeRes::Def` arm calls `report_trait_as_ty` for `OwnerNode::Trait(_)`, so every `Self` written inside a trait body would become a diagnostic.
2. Its arg-count check rejects `args.len() != declared`, so `Self` inside `struct Foo<T>` would report "expected 1 argument, got 0".

`self_ty` does real work — applying a struct's own parameters, an `extend` block's target generics, a trait's `SelfTy` placeholder — that an ordinary path lookup does not reproduce. Leave it alone.

**Files:**
- Modify: `src/ast.rs:281` (delete `SelfType` from the AST's `TyKind`)
- Modify: `src/parser/type_parser.rs:95` (produce a `Path`), `:433`, `:581` (fix the two assertions)
- Modify: `src/hir/lower/ty.rs:33` (recognize the `Self` path)

**Do NOT modify:** `src/hir/types.rs`, `src/hir/visit.rs`, `src/nameres/resolve_ty.rs`, `src/nameres/results.rs`, `src/nameres/resolve_item.rs`, or anything under `src/typeck/`. If you believe one of them needs changing, stop and report rather than changing it.

**Interfaces:**
- Consumes: nothing.
- Produces: in the **AST**, `Self` is representable only as `TyKind::Path { path, args }` where `path.segments` is one `Ident` whose `text` is `Interner::intern("Self")`. Task 8 relies on this. The HIR is unchanged.

- [ ] **Step 1: Write the failing parser test**

Replace the assertion at `src/parser/type_parser.rs:433`. Find the existing test that asserts `matches!(ty.kind, TyKind::SelfType)` and rewrite it:

```rust
#[test]
fn self_type_parses_as_a_single_segment_path() {
    let ty = parse_ty("Self");
    let TyKind::Path { path, args } = &ty.kind else {
        panic!("expected `Self` to parse as a path, got {:?}", ty.kind);
    };
    assert_eq!(path.segments.len(), 1);
    assert_eq!(Interner::resolve(path.segments[0].text), "Self");
    assert!(args.is_empty());
}
```

Use whatever the file's existing test helper for parsing a type is (read the surrounding tests — do not invent a `parse_ty` if the file names it something else).

- [ ] **Step 2: Run it and confirm it fails**

Run: `cargo test --lib parser::type_parser`
Expected: FAIL — the parser still produces `TyKind::SelfType`.

- [ ] **Step 3: Make the parser produce a path**

At `src/parser/type_parser.rs:95`, replace the `TyKind::SelfType` construction with a `TyKind::Path`. The parser already has the `Self` token's span; build:

```rust
kind: TyKind::Path {
    path: Path {
        segments: vec![Ident {
            text: Interner::intern("Self"),
            span,
        }],
        span,
    },
    args: Vec::new(),
},
```

where `span` is the span the surrounding code already computed for the `Self` token. Add `use crate::ast::{Ident, Path};` and the `Interner` import if they are not already present.

- [ ] **Step 4: Delete the AST variant**

Delete line `src/ast.rs:281` (`SelfType,`). Fix the assertion at `src/parser/type_parser.rs:581` — `TyKind::Any(inner) => assert!(matches!(inner.kind, TyKind::SelfType))` becomes a check that `inner.kind` is a single-segment `Self` path, same shape as Step 1.

- [ ] **Step 5: Map the `Self` path back to `HirTyKind::SelfType` in lowering**

`src/hir/lower/ty.rs`'s `ast::TyKind::Path` arm now receives `Self`. Recognize it *before* the ordinary path lowering and produce `HirTyKind::SelfType`, leaving the HIR exactly as it was:

```rust
/// Whether `path` is the `Self` the parser produces for the `Self` keyword.
///
/// Matching on the segment's text is safe because `Self` is a reserved token: the lexer
/// always emits `UpperSelfKw` for that spelling and never `Identifier`, and `path_parser` is
/// built solely from `ident_parser`, which consumes only `Identifier`. So no user-written
/// path segment can ever carry this text.
fn is_self_path(path: &ast::Path) -> bool {
    path.segments.len() == 1 && Interner::resolve(path.segments[0].text) == "Self"
}
```

Nothing else in the HIR moves. `HirTyKind::SelfType`, `src/hir/visit.rs:502`, `src/nameres/*`, and `src/typeck/lower_ty.rs:65` all stay exactly as they are — see this task's "Do NOT modify" list and the reasoning above it. `SelfTyRes` is deleted in Task 15, not here.

- [ ] **Step 6: Verify the whole suite is green**

Run: `cargo build && cargo test && cargo fmt --check`
Expected: all pass, with no test needing its expectations changed — the HIR is unchanged, so nothing downstream should observe a difference. A golden file needing an update means the mapping in Step 5 is wrong; investigate rather than re-blessing it.

For clippy, follow the Global Constraint: capture a baseline and diff. **Do not run `git stash` after committing** — with a clean tree it is a no-op and the "baseline" run measures the same commit twice, proving nothing. Use a detached worktree at the base commit instead:

```bash
git worktree add --detach /tmp/phi-clippy-base b5a510e
(cd /tmp/phi-clippy-base && cargo clippy -- -D warnings 2>&1 | grep '^error' | sort) > /tmp/base.txt
cargo clippy -- -D warnings 2>&1 | grep '^error' | sort > /tmp/new.txt
diff /tmp/base.txt /tmp/new.txt && echo "no new clippy findings"
git worktree remove /tmp/phi-clippy-base
```

- [ ] **Step 7: Commit**

```bash
git add -A
git commit -m "refactor: parse Self as an ordinary path, delete TyKind::SelfType"
```

---

## Task 2: `Path` equality over segments only

`Path` gains `PartialEq`, `Eq`, and `Hash` defined over `segments[..].text`, ignoring every span. Entry lookup in `NameResolutions::get` depends on this (spec, "Required AST changes" #3).

**Files:**
- Modify: `src/ast.rs:65` (`Path`)
- Test: `src/ast.rs` (a `#[cfg(test)] mod` at the end of the file, or the existing one if present)

**Interfaces:**
- Consumes: nothing.
- Produces: `impl PartialEq + Eq + Hash for ast::Path`, comparing segment symbols only. Tasks 3 and 7 rely on this.

- [ ] **Step 1: Write the failing test**

```rust
#[cfg(test)]
mod path_eq_tests {
    use super::*;
    use std::collections::hash_map::DefaultHasher;
    use std::hash::{Hash, Hasher};

    fn path(segments: &[&str], start: usize) -> Path {
        let span = SrcSpan::new(start, start + 1);
        Path {
            segments: segments
                .iter()
                .map(|s| Ident { text: Interner::intern(s), span })
                .collect(),
            span,
        }
    }

    fn hash_of(p: &Path) -> u64 {
        let mut h = DefaultHasher::new();
        p.hash(&mut h);
        h.finish()
    }

    #[test]
    fn paths_with_the_same_segments_but_different_spans_are_equal() {
        let a = path(&["math", "vector"], 0);
        let b = path(&["math", "vector"], 500);
        assert_eq!(a, b);
        assert_eq!(hash_of(&a), hash_of(&b));
    }

    #[test]
    fn paths_with_different_segments_are_not_equal() {
        assert_ne!(path(&["math"], 0), path(&["vector"], 0));
    }

    #[test]
    fn paths_of_different_lengths_are_not_equal() {
        assert_ne!(path(&["math"], 0), path(&["math", "vector"], 0));
    }
}
```

Check `SrcSpan`'s real constructor before writing this — use whatever `src/diag.rs` (or wherever `SrcSpan` lives) actually exposes.

- [ ] **Step 2: Run it and confirm it fails**

Run: `cargo test --lib path_eq_tests`
Expected: FAIL — `Path` does not implement `PartialEq`.

- [ ] **Step 3: Implement the three traits by hand**

Do not add them to the `derive` list — the derive would include spans. At `src/ast.rs:65`:

```rust
/// `Path` compares and hashes over its segment symbols alone, ignoring every span: a `Path`
/// identifies a name, not a source location, and two writings of `math::vector` are the same
/// path. `NameResolutions::get` matches a node's recorded paths this way, which is what lets
/// an `extend` block's two entries be told apart by what they name rather than by position.
impl PartialEq for Path {
    fn eq(&self, other: &Self) -> bool {
        self.segments.len() == other.segments.len()
            && self
                .segments
                .iter()
                .zip(&other.segments)
                .all(|(a, b)| a.text == b.text)
    }
}

impl Eq for Path {}

impl std::hash::Hash for Path {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        // Must agree with `PartialEq` above: hash exactly what `eq` compares, and nothing
        // else. Hashing the length too keeps `[a, b]` and `[ab]` apart.
        self.segments.len().hash(state);
        for segment in &self.segments {
            segment.text.hash(state);
        }
    }
}
```

- [ ] **Step 4: Run the tests**

Run: `cargo test --lib path_eq_tests`
Expected: PASS (3/3).

- [ ] **Step 5: Verify nothing else broke**

Run: `cargo build && cargo test && cargo fmt --check && cargo clippy -- -D warnings`
Expected: all pass.

- [ ] **Step 6: Commit**

```bash
git add -A
git commit -m "feat: Path equality and hashing over segments only"
```

---

## Task 3: The `surface` module skeleton, `Res`, and `NameResolutions`

Creates the new module and the two data types that everything else in the plan produces or consumes.

**Files:**
- Modify: `Cargo.toml` (add `smallvec`)
- Modify: `src/nameres.rs` (add `pub mod surface;`)
- Create: `src/nameres/surface.rs`
- Create: `src/nameres/surface/res.rs`
- Create: `src/nameres/surface/results.rs`
- Create: `src/nameres/surface/tests.rs`

**Interfaces:**
- Consumes: `ast::Path`'s segment-only `PartialEq`/`Hash` from Task 2.
- Produces:
  - `surface::res::{Res, Type, TyDef, Local}` — exactly as written in Step 2 below.
  - `surface::results::NameResolutions` with `new()`, `record(&mut self, owner: NodeId, path: Path, res: Res)`, `get(&self, owner: NodeId, path: &Path) -> Option<Res>`, `entries(&self, owner: NodeId) -> &[(Path, Res)]`, `record_lang_items(&mut self, items: LangItems)`, `lang_items(&self) -> &LangItems`.
  - Tasks 4-10 all import from these two modules.

- [ ] **Step 1: Add the dependency**

```bash
cargo add smallvec
```

Confirm `Cargo.toml` gained `smallvec = "1"` (or whatever major version resolves) under `[dependencies]`.

- [ ] **Step 2: Create `src/nameres/surface/res.rs`**

These definitions are copied verbatim from the spec. Do not add variants, and do not flatten `Type` into `Res`.

```rust
//! What a path written in the AST resolved to.

use crate::ast::NodeId;
use crate::nameres::results::PrimTy;

/// What one written path named.
///
/// `Err` is *recorded*, never left absent: absence in `NameResolutions` has to mean "never
/// reached", and conflating it with "resolved, unsuccessfully" leaves every consumer telling
/// the two apart from context it does not have.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Res {
    Type(Type),
    Local(Local),
    /// The `Item` wrapping a `fun`. `Function`, like `Struct`/`Enum`/`Trait`/`Extend`, has no
    /// `NodeId` of its own -- it sits inside `Item`, which does (`src/ast.rs:85`).
    Function(NodeId),
    Module(NodeId),
    Err,
}

/// What a path in *type* position named.
///
/// This nests inside `Res` rather than being flattened into it so that a type-position lookup
/// has exactly one return type and consumers narrow once.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Type {
    /// A built-in such as `i32` or `bool`, which never gets a `NodeId`.
    Prim(PrimTy),
    /// A generic type parameter, addressed by the `ast::Generic` that declares it.
    Generic(NodeId),
    Def(TyDef),
}

/// A nominal item. Struct, enum, and trait are combined because all three share one namespace,
/// so a consumer that needs only "a nominal item, give me its `NodeId`" matches a single arm.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TyDef {
    Struct(NodeId),
    Enum(NodeId),
    Trait(NodeId),
}

impl TyDef {
    /// The `NodeId` of the `Item` this names, whichever kind it is.
    pub fn node_id(self) -> NodeId {
        match self {
            TyDef::Struct(id) | TyDef::Enum(id) | TyDef::Trait(id) => id,
        }
    }
}

/// A binding in value position.
///
/// `SelfParam` is kept apart from `Variable` because `self` is not an ordinary local: it
/// carries a `SelfMode` rather than a declared type, and its type is the enclosing item's
/// `Self`. Every consumer handles it specially anyway; a distinct variant forces that to be
/// exhaustive.
///
/// There is deliberately no `Variant` arm. A `.variant` names no enum of its own -- the enum
/// comes from the expected type, so typeck resolves it once it knows that type.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Local {
    Param(NodeId),
    SelfParam(NodeId),
    /// The binding `ast::Pat`.
    Variable(NodeId),
}
```

- [ ] **Step 3: Write the failing `NameResolutions` tests**

In `src/nameres/surface/tests.rs`:

```rust
use crate::ast::{Ident, NodeId, Path};
use crate::ast::interner::Interner;
use crate::diag::SrcSpan;
use crate::nameres::surface::res::{Local, Res};
use crate::nameres::surface::results::NameResolutions;

fn path(segments: &[&str]) -> Path {
    let span = SrcSpan::new(0, 1);
    Path {
        segments: segments
            .iter()
            .map(|s| Ident { text: Interner::intern(s), span })
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

    assert_eq!(r.get(owner, &path(&["Show"])), Some(Res::Local(Local::Param(target))));
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
```

Check `NodeId`'s allocator name at `src/ast/node_id.rs` — the memory of this codebase says it is `NodeId::next()`, but confirm before writing.

- [ ] **Step 4: Run and confirm failure**

Run: `cargo test --lib nameres::surface`
Expected: FAIL to compile — `results` module does not exist.

- [ ] **Step 5: Create `src/nameres/surface/results.rs`**

```rust
//! The output of AST-level name resolution.

use std::collections::HashMap;

use smallvec::SmallVec;

use crate::ast::{NodeId, Path};
use crate::langitems::LangItems;
use crate::nameres::surface::res::Res;

/// Every path written in the program, grouped by the node that owns it, plus the lang items.
///
/// One table serves what the HIR-side resolver split into four. An `ast::Generic` owns its
/// bound paths, so bounds are simply further entries in that node's list, in source order.
/// `Self` is an ordinary path since `TyKind::SelfType` was removed. Generics are
/// resolver-internal scope-stack state rather than output.
///
/// `lang_items` stays pass output: resolving it is name resolution's job and can only be done
/// while the symbol table exists, but every consumer of it is a later pass.
#[derive(Debug, Default)]
pub struct NameResolutions {
    /// `SmallVec<[_; 2]>` is inline capacity, not a cap. Two is the common maximum --
    /// `extend Vec<T> with Show` puts `adt_path` and `trait_path` on one `Item` node. A
    /// generic with three bounds spills to the heap and stays correct.
    paths: HashMap<NodeId, SmallVec<[(Path, Res); 2]>>,
    lang_items: LangItems,
}

impl NameResolutions {
    pub fn new() -> Self {
        Self::default()
    }

    /// Records that `path`, written on `owner`, named `res`.
    ///
    /// **Invariant: within one node, no two entries may have equal paths.** A node owning two
    /// textually identical paths (`extend Foo with Foo`, or a duplicate bound `T: Show + Show`)
    /// is rejected by the check that owns it, before it reaches here. That is what leaves
    /// `get` unambiguous by construction rather than by a first-match tiebreak.
    pub fn record(&mut self, owner: NodeId, path: Path, res: Res) {
        let entries = self.paths.entry(owner).or_default();
        debug_assert!(
            !entries.iter().any(|(p, _)| p == &path),
            "a node may not own two equal paths; the owning check should have rejected this"
        );
        entries.push((path, res));
    }

    /// What `path`, as written on `owner`, named. Matches on segment symbols -- see
    /// `impl PartialEq for Path`. For `extend Vec<T> with Show`, the caller holding
    /// `adt_path` matches `[Vec]` and the caller holding `trait_path` matches `[Show]`, with
    /// no positional contract and no role discriminant for a later edit to invalidate.
    pub fn get(&self, owner: NodeId, path: &Path) -> Option<Res> {
        self.paths
            .get(&owner)?
            .iter()
            .find(|(p, _)| p == path)
            .map(|(_, res)| *res)
    }

    /// Every path `owner` owns, in source order. Empty for a node that owns none.
    pub fn entries(&self, owner: NodeId) -> &[(Path, Res)] {
        self.paths.get(&owner).map_or(&[], |v| v.as_slice())
    }

    pub fn record_lang_items(&mut self, lang_items: LangItems) {
        self.lang_items = lang_items;
    }

    pub fn lang_items(&self) -> &LangItems {
        &self.lang_items
    }
}
```

If `LangItems` does not implement `Default`, either add `#[derive(Default)]` to it or store `lang_items: Option<LangItems>` and have `lang_items()` panic with a clear message if unset. Prefer adding `Default`.

- [ ] **Step 6: Create `src/nameres/surface.rs`**

```rust
//! AST-level name resolution.
//!
//! This is the resolver the pipeline is moving to: it runs on the `Ast`, before HIR lowering,
//! and produces a side table keyed by `NodeId`. The HIR-based resolver in this module's
//! siblings is still the one the pipeline runs; rewiring is a follow-up.
//!
//! See `docs/superpowers/specs/2026-08-07-ast-symbol-table-design.md`.

pub mod res;
pub mod results;
#[cfg(test)]
mod tests;

pub use res::{Local, Res, TyDef, Type};
pub use results::NameResolutions;
```

Add `pub mod surface;` to `src/nameres.rs` alongside the existing `pub mod` lines.

- [ ] **Step 7: Run the tests**

Run: `cargo test --lib nameres::surface`
Expected: PASS (5/5).

- [ ] **Step 8: Verify and commit**

Run: `cargo build && cargo test && cargo fmt --check && cargo clippy -- -D warnings`

Dead-code warnings on the new module are expected at this stage but must not be errors — if `-D warnings` trips on `dead_code`, add `#![allow(dead_code)]` to `src/nameres/surface.rs` with a comment saying it comes off when the pipeline is rewired.

```bash
git add -A
git commit -m "feat: add surface::Res and surface::NameResolutions"
```

---

## Task 4: `SymbolTable` and the collect phase

Builds one `ModuleScope` per module by walking the AST module tree, with a conflict check on every insert and a dedicated diagnostic for declaring an item named after a primitive.

**Files:**
- Create: `src/nameres/surface/symbol_table.rs`
- Create: `src/nameres/surface/diagnostics.rs`
- Modify: `src/nameres/surface.rs` (declare the two modules)
- Modify: `src/nameres/surface/tests.rs` (add tests)

**Interfaces:**
- Consumes: `res::{Res, Type, TyDef, Local}` from Task 3.
- Produces:
  - `pub struct SymbolTable<'ast>` with the exact field set in Step 3.
  - `struct ModuleScope { functions: HashMap<Symbol, NodeId>, types: HashMap<Symbol, TyDef>, mods: HashMap<Symbol, NodeId> }`
  - `SymbolTable::collect(ast: &'ast Ast) -> Self` (a partial constructor; Task 5 turns it into the full `new`).
  - `pub fn prim_ty(name: Symbol) -> Option<PrimTy>` in `symbol_table.rs`.
  - Diagnostics: `report_not_found(Ident)`, `report_conflict(Ident)`, `report_ambiguous_import(Ident)`, `report_shadows_builtin(Ident)`, `report_dyn_not_trait(SrcSpan)`, `report_self_unavailable(SrcSpan)`.
  - Tasks 5-9 all build on these.

- [ ] **Step 1: Create `src/nameres/surface/diagnostics.rs`**

Model these on `src/nameres/symbol_table.rs:425-469` — same `DiagCtx::emit(Diagnostic::error(...).with_label(...).with_help(...))` shape.

```rust
//! The diagnostics AST-level name resolution emits. All six go through `DiagCtx::emit`.

use crate::ast::Ident;
use crate::ast::interner::Interner;
use crate::diag::{DiagCtx, Diagnostic, SrcSpan};

pub fn report_not_found(name: Ident) {
    DiagCtx::emit(
        Diagnostic::error(
            format!("cannot find `{}` in this scope", Interner::resolve(name.text)),
            name.span,
        )
        .with_label("not found in this scope"),
    );
}

pub fn report_conflict(name: Ident) {
    DiagCtx::emit(
        Diagnostic::error(
            format!(
                "the name `{}` is defined multiple times",
                Interner::resolve(name.text)
            ),
            name.span,
        )
        .with_label("redefined here")
        .with_help("a name with the same spelling is already in scope"),
    );
}

/// An import whose path matches more than one namespace at once, so there is no single answer
/// for what the imported name should mean.
pub fn report_ambiguous_import(name: Ident) {
    DiagCtx::emit(
        Diagnostic::error(
            format!(
                "ambiguous import: `{}` refers to more than one item",
                Interner::resolve(name.text)
            ),
            name.span,
        )
        .with_label("ambiguous import")
        .with_help("this path matches more than one declaration; use a more specific path to disambiguate"),
    );
}

/// Declaring an item named after a primitive. Rejecting this at collect time is the
/// duplicate-name rule applied to builtins, and it is what lets type lookup check primitives
/// first instead of making every `i32` walk the whole module chain.
pub fn report_shadows_builtin(name: Ident) {
    DiagCtx::emit(
        Diagnostic::error(
            format!("`{}` shadows a built-in type", Interner::resolve(name.text)),
            name.span,
        )
        .with_label("shadows a built-in type")
        .with_help("built-in type names cannot be redeclared"),
    );
}

/// `dyn` applied to something that is not a trait. Recorded as `Res::Err` so this fires once
/// here rather than cascading into typeck.
pub fn report_dyn_not_trait(span: SrcSpan) {
    DiagCtx::emit(
        Diagnostic::error("`dyn` requires a trait".to_string(), span)
            .with_label("not a trait")
            .with_help("`dyn` dispatches dynamically over a trait; a struct or enum is used directly"),
    );
}

/// `Self` written outside any definition that introduces one.
pub fn report_self_unavailable(span: SrcSpan) {
    DiagCtx::emit(
        Diagnostic::error("`Self` is not available here".to_string(), span)
            .with_label("no enclosing struct, enum, trait, or `extend` block")
            .with_help("`Self` names the definition it is written inside"),
    );
}
```

Confirm `SrcSpan`'s import path and `Diagnostic`'s builder methods against `src/diag.rs` before writing.

- [ ] **Step 2: Write the failing collect tests**

Add to `src/nameres/surface/tests.rs`. Build the `Ast` through the real parser — read `src/nameres/tests.rs` and `src/testing.rs` for the existing helper that turns source text into an `Ast`, and reuse it rather than constructing an `Ast` by hand.

```rust
#[test]
fn collect_puts_a_function_in_the_value_namespace() {
    let ast = ast_from("fun f() {}");
    let table = SymbolTable::collect(&ast);
    assert!(table.lookup_function(ast.root_id(), Interner::intern("f")).is_some());
}

#[test]
fn collect_puts_a_struct_in_the_type_namespace() {
    let ast = ast_from("struct S {}");
    let table = SymbolTable::collect(&ast);
    assert!(matches!(
        table.lookup_type_name(ast.root_id(), Interner::intern("S")),
        Some(TyDef::Struct(_))
    ));
}

#[test]
fn collect_keeps_a_trait_and_an_enum_apart_by_tydef_kind() {
    let ast = ast_from("enum E { .a } trait T {}");
    let table = SymbolTable::collect(&ast);
    assert!(matches!(table.lookup_type_name(ast.root_id(), Interner::intern("E")), Some(TyDef::Enum(_))));
    assert!(matches!(table.lookup_type_name(ast.root_id(), Interner::intern("T")), Some(TyDef::Trait(_))));
}

#[test]
fn two_declarations_of_one_name_in_one_namespace_conflict() {
    let (_, diags) = collect_with_diags("fun f() {} fun f() {}");
    assert_eq!(diags.len(), 1);
    assert!(diags[0].message().contains("is defined multiple times"));
}

#[test]
fn a_declaration_named_after_a_primitive_is_rejected() {
    let (_, diags) = collect_with_diags("struct i32 {}");
    assert_eq!(diags.len(), 1);
    assert!(diags[0].message().contains("shadows a built-in type"));
}

#[test]
fn by_path_maps_a_canonical_path_to_its_module() {
    let ast = ast_from_files(&["module math::vector; fun dot() {}"]);
    let table = SymbolTable::collect(&ast);
    let id = table.module_by_path(&[Interner::intern("math"), Interner::intern("vector")]);
    assert!(id.is_some());
}
```

Write `ast_from`, `ast_from_files`, and `collect_with_diags` as helpers in the test module. `collect_with_diags` needs to capture what `DiagCtx` collected — read `src/diag.rs` and `src/nameres/tests.rs` for how the existing tests drain the thread-local diagnostic buffer, and follow that exactly. If no such helper exists, write one in `tests.rs` and note it in the report.

- [ ] **Step 3: Run and confirm failure**

Run: `cargo test --lib nameres::surface`
Expected: FAIL to compile — `SymbolTable` does not exist.

- [ ] **Step 4: Define `SymbolTable` and `ModuleScope`**

In `src/nameres/surface/symbol_table.rs`:

```rust
pub struct SymbolTable<'ast> {
    /// Locals, generics, and `Self` are all pushed on entering the construct that introduces
    /// them and popped on leaving it: locals per block and per match arm, generics per
    /// definition declaring `<...>`, `Self` per struct, enum, trait, and `extend`. A method
    /// seeing its `extend` block's `<T>` falls out of the stack rather than needing an
    /// owner-chain walk against a side table -- which is why `NameResolutions` has no
    /// `generics` table.
    local_scopes: Vec<HashMap<Symbol, Local>>,
    generic_scopes: Vec<HashMap<Symbol, Type>>,
    self_scopes: Vec<TyDef>,
    modules: HashMap<NodeId, ModuleScope>,
    /// A canonical, span-free path to the module it names, so a fully-qualified path resolves
    /// in one hash rather than a segment-by-segment walk.
    by_path: HashMap<Box<[Symbol]>, NodeId>,
    /// The prelude, once found. `None` if the core library is not part of this unit -- which
    /// should not happen in a real build, but leaves the resolver working rather than
    /// panicking if it is ever driven without one.
    prelude: Option<NodeId>,
    ast: &'ast Ast,
}

/// One module's declared items, split by namespace.
///
/// `types` holds `TyDef`, not `Type`: `Prim` and `Generic` can never live in a module's
/// namespace, and the narrower type says so.
struct ModuleScope {
    functions: HashMap<Symbol, NodeId>,
    types: HashMap<Symbol, TyDef>,
    mods: HashMap<Symbol, NodeId>,
}
```

- [ ] **Step 5: Write `prim_ty` and the namespace inserters**

```rust
/// The primitive named by `name`, if any. Type lookup consults this first, which is sound
/// because `insert_*` rejects any declaration that would shadow one.
pub fn prim_ty(name: Symbol) -> Option<PrimTy> {
    Some(match Interner::resolve(name).as_str() {
        "i8" => PrimTy::I8,
        "i16" => PrimTy::I16,
        "i32" => PrimTy::I32,
        "i64" => PrimTy::I64,
        "u8" => PrimTy::U8,
        "u16" => PrimTy::U16,
        "u32" => PrimTy::U32,
        "u64" => PrimTy::U64,
        "f32" => PrimTy::F32,
        "f64" => PrimTy::F64,
        "bool" => PrimTy::Bool,
        "char" => PrimTy::Char,
        _ => return None,
    })
}
```

Adjust `Interner::resolve`'s return type handling to match its real signature (`src/ast/interner.rs`) — it may return `&str` or `String`.

Each of `insert_function`, `insert_type`, `insert_mod` on `ModuleScope` follows the same shape as `src/nameres/symbol_table.rs:33-59`, with one addition — the primitive check comes first:

```rust
fn insert_type(&mut self, name: Ident, def: TyDef) {
    if prim_ty(name.text).is_some() {
        report_shadows_builtin(name);
        return;
    }
    match self.types.entry(name.text) {
        Entry::Occupied(_) => report_conflict(name),
        Entry::Vacant(e) => {
            e.insert(def);
        }
    }
}
```

Apply the same primitive guard in `insert_function` and `insert_mod` — a function or module named `i32` is equally a shadow.

- [ ] **Step 6: Write the collect walk**

`SymbolTable::collect(ast)` recurses the module tree from `ast.root_id()`, using `ast.module(id)` and `Module::children` (`src/ast.rs:32-38`). For each module, build a `ModuleScope` from its `items`, dispatching on `ItemKind`:

- `Function(f)` → `insert_function(f.name, item.id)`
- `Struct(s)` → `insert_type(s.name, TyDef::Struct(item.id))`
- `Enum(e)` → `insert_type(e.name, TyDef::Enum(item.id))`
- `Trait(t)` → `insert_type(t.name, TyDef::Trait(item.id))`
- `Extend(_)` → nothing; `extend` blocks are unnamed
- `ModuleDecl(_) | Import(_) | Error` → nothing

Note the `NodeId` recorded is `item.id`, the enclosing `Item`'s — `Function`/`Struct`/`Enum`/`Trait` have no id of their own (spec, "Which node owns which paths").

Submodules come from `Module::children`, not from `items`: insert each child's last path segment into `mods` keyed to the child's `NodeId`, then recurse.

Fill `by_path` in the same walk: each module's `Module::path` segments, mapped to `Box<[Symbol]>`, keyed to its `NodeId`.

- [ ] **Step 7: Add the read accessors the tests need**

```rust
pub fn lookup_function(&self, module: NodeId, name: Symbol) -> Option<NodeId>;
pub fn lookup_type_name(&self, module: NodeId, name: Symbol) -> Option<TyDef>;
pub fn lookup_mod(&self, module: NodeId, name: Symbol) -> Option<NodeId>;
pub fn module_by_path(&self, segments: &[Symbol]) -> Option<NodeId>;
```

Each is a two-level `HashMap` get, copying the value out, exactly like `src/nameres/symbol_table.rs:312-320`.

- [ ] **Step 8: Run the tests**

Run: `cargo test --lib nameres::surface`
Expected: PASS (all).

- [ ] **Step 9: Verify and commit**

Run: `cargo build && cargo test && cargo fmt --check && cargo clippy -- -D warnings`

```bash
git add -A
git commit -m "feat: surface SymbolTable collect phase and diagnostics"
```

---

## Task 5: Import resolution and prelude discovery

Phases 2 and 3 of construction, turning `SymbolTable::collect` into the full `SymbolTable::new`.

**Files:**
- Modify: `src/nameres/surface/symbol_table.rs`
- Modify: `src/nameres/surface/tests.rs`

**Interfaces:**
- Consumes: Task 4's `collect`, `ModuleScope`, inserters, and accessors.
- Produces: `SymbolTable::new(ast: &'ast Ast) -> Self` — the full three-phase constructor. Tasks 7-9 call this.

- [ ] **Step 1: Write the failing tests**

```rust
#[test]
fn an_import_binds_into_the_importing_modules_own_scope() {
    let ast = ast_from_files(&[
        "module math; pub fun dot() {}",
        "module app; import math::dot;",
    ]);
    let table = SymbolTable::new(&ast);
    let app = table.module_by_path(&[Interner::intern("app")]).unwrap();
    assert!(table.lookup_function(app, Interner::intern("dot")).is_some());
}

#[test]
fn an_import_resolves_absolutely_from_the_root_not_relative_to_where_it_is_written() {
    // `deep::inner` is written inside `app::nested`, and still resolves from the root.
    let ast = ast_from_files(&[
        "module deep; pub fun inner() {}",
        "module app::nested; import deep::inner;",
    ]);
    let table = SymbolTable::new(&ast);
    let nested = table
        .module_by_path(&[Interner::intern("app"), Interner::intern("nested")])
        .unwrap();
    assert!(table.lookup_function(nested, Interner::intern("inner")).is_some());
}

#[test]
fn an_import_may_name_a_module_the_collect_pass_had_not_reached() {
    // The importing module is parsed first; the imported one second.
    let ast = ast_from_files(&[
        "module app; import later::thing;",
        "module later; pub fun thing() {}",
    ]);
    let (table, diags) = new_with_diags(&ast);
    assert!(diags.is_empty(), "unexpected diagnostics: {diags:?}");
    let app = table.module_by_path(&[Interner::intern("app")]).unwrap();
    assert!(table.lookup_function(app, Interner::intern("thing")).is_some());
}

#[test]
fn a_glob_import_copies_every_name_from_the_source_module() {
    let ast = ast_from_files(&[
        "module math; pub fun dot() {} pub struct Vec2 {}",
        "module app; import math::*;",
    ]);
    let table = SymbolTable::new(&ast);
    let app = table.module_by_path(&[Interner::intern("app")]).unwrap();
    assert!(table.lookup_function(app, Interner::intern("dot")).is_some());
    assert!(table.lookup_type_name(app, Interner::intern("Vec2")).is_some());
}

#[test]
fn a_glob_import_colliding_with_a_declaration_conflicts() {
    let (_, diags) = new_with_diags_from(&[
        "module math; pub fun dot() {}",
        "module app; import math::*; fun dot() {}",
    ]);
    assert_eq!(diags.len(), 1);
    assert!(diags[0].message().contains("is defined multiple times"));
}

#[test]
fn an_import_matching_two_namespaces_is_ambiguous() {
    let (_, diags) = new_with_diags_from(&[
        "module math; pub fun thing() {} pub struct thing {}",
        "module app; import math::thing;",
    ]);
    assert!(diags.iter().any(|d| d.message().contains("ambiguous import")));
}

#[test]
fn an_import_naming_nothing_reports_not_found() {
    let (_, diags) = new_with_diags_from(&["module app; import nowhere::gone;"]);
    assert_eq!(diags.len(), 1);
    assert!(diags[0].message().contains("cannot find"));
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
```

The `struct thing` in the ambiguity test also produces a `report_conflict`-free result because the two live in different namespaces — assert with `.any(...)` rather than an exact count there, as written. `ast_with_core()` needs the real core library loaded; read `src/driver/source.rs`'s `SrcCollector::collect_core` and `src/nameres/tests.rs` to see how existing tests get a core-bearing `Ast`. If no such helper exists, mark this test `#[ignore]` with a comment and report it — do not fake a core library.

Add `pub fn prelude(&self) -> Option<NodeId>` to `SymbolTable`.

- [ ] **Step 2: Run and confirm failure**

Run: `cargo test --lib nameres::surface`
Expected: FAIL — `SymbolTable::new` does not exist.

- [ ] **Step 3: Write `resolve_imports`**

Port `src/nameres/symbol_table.rs:158-229` to the AST, keyed by `NodeId`. The differences from the HIR version:

- Iterate `ast.mod_ids()` rather than recursing — every module is visited, and phase ordering is already guaranteed because `collect` finished for the whole tree first.
- The imports of a module are `ast::Module::imports` (`src/ast.rs:35`), a `Vec<Import>`, not a `Vec<HirId>` needing a lookup.
- The three lookups resolve `from = ast.root_id()`, absolutely.
- `insert_type` takes a `TyDef`, so the type-namespace branch binds a `TyDef` rather than a bare id.

Keep the doc comment explaining *why* imports resolve absolutely and *why* this must run after the whole collect pass — that reasoning is the point of the phase split.

- [ ] **Step 4: Write `import_glob`**

Port `src/nameres/symbol_table.rs:241-284` unchanged in structure. Keep the doc comment: a glob goes through the same conflict checks as an ordinary declaration, with no "imports don't conflict with declarations" carve-out, and conflicts are blamed on the `import` statement's own span because a glob names no item individually.

- [ ] **Step 5: Write `find_prelude`**

```rust
/// The module unqualified lookups fall back to once the enclosing module chain is exhausted.
const PRELUDE_PATH: [&str; 2] = ["core", "prelude"];

/// Walks `PRELUDE_PATH` down from the root.
///
/// This must run after imports, because the prelude's namespace *is* the set of imports it
/// declares.
fn find_prelude(&self) -> Option<NodeId> {
    let mut current = self.ast.root_id();
    for segment in PRELUDE_PATH {
        current = self.lookup_mod(current, Interner::intern(segment))?;
    }
    Some(current)
}
```

- [ ] **Step 6: Assemble `new`**

```rust
pub fn new(ast: &'ast Ast) -> Self {
    let mut table = Self::collect(ast);
    table.resolve_imports();
    // The prelude's own namespace is filled in by the imports it declares, so it is not
    // usable as a fallback until those have resolved.
    table.prelude = table.find_prelude();
    table
}
```

- [ ] **Step 7: Run the tests**

Run: `cargo test --lib nameres::surface`
Expected: PASS (all, except any `#[ignore]`d core-library test).

- [ ] **Step 8: Verify and commit**

Run: `cargo build && cargo test && cargo fmt --check && cargo clippy -- -D warnings`

```bash
git add -A
git commit -m "feat: surface import resolution and prelude discovery"
```

---

## Task 6: The three scope stacks

Locals, generics, and `Self` — pushed on entering the construct that introduces them, popped on leaving it.

**Files:**
- Modify: `src/nameres/surface/symbol_table.rs`
- Modify: `src/nameres/surface/tests.rs`

**Interfaces:**
- Consumes: Task 4's `SymbolTable` fields.
- Produces, exactly as the spec's Public API section writes them:
  ```rust
  pub fn push_scope(&mut self);
  pub fn pop_scope(&mut self);
  pub fn insert_local(&mut self, name: Ident, local: Local);
  pub fn lookup_local(&self, name: Symbol) -> Option<Local>;
  pub fn push_generics(&mut self, params: HashMap<Symbol, Type>);
  pub fn pop_generics(&mut self);
  pub fn push_self(&mut self, ty: TyDef);
  pub fn pop_self(&mut self);
  pub fn lookup_generic(&self, name: Symbol) -> Option<Type>;
  pub fn current_self(&self) -> Option<TyDef>;
  ```
  Task 9 drives all of these.

- [ ] **Step 1: Write the failing tests**

```rust
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
    assert_eq!(t.lookup_local(Interner::intern("x")), Some(Local::Variable(second)));
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
```

Add an `ident(&str) -> Ident` helper to the test module.

- [ ] **Step 2: Run and confirm failure**

Run: `cargo test --lib nameres::surface`
Expected: FAIL to compile.

- [ ] **Step 3: Implement all ten methods**

```rust
pub fn push_scope(&mut self) {
    self.local_scopes.push(HashMap::new());
}

pub fn pop_scope(&mut self) {
    self.local_scopes.pop();
}

/// Binds `name` in the innermost scope.
///
/// Takes an `Ident`, not a `Path`: a local is always one segment, and accepting a `Path`
/// would imply `let a::b = ...` is representable. Returns `()`, not `Result` -- shadowing is
/// legal, so the innermost map is simply overwritten and there is no failure to report.
pub fn insert_local(&mut self, name: Ident, local: Local) {
    self.local_scopes
        .last_mut()
        .expect("insert_local requires an open scope")
        .insert(name.text, local);
}

/// Looks `name` up in every open local scope, innermost first.
pub fn lookup_local(&self, name: Symbol) -> Option<Local> {
    self.local_scopes.iter().rev().find_map(|s| s.get(&name).copied())
}

pub fn push_generics(&mut self, params: HashMap<Symbol, Type>) {
    self.generic_scopes.push(params);
}

pub fn pop_generics(&mut self) {
    self.generic_scopes.pop();
}

/// Looks `name` up in every open generic scope, innermost first. A method seeing its
/// `extend` block's `<T>` is just the outer scope still being on the stack.
pub fn lookup_generic(&self, name: Symbol) -> Option<Type> {
    self.generic_scopes.iter().rev().find_map(|s| s.get(&name).copied())
}

pub fn push_self(&mut self, ty: TyDef) {
    self.self_scopes.push(ty);
}

pub fn pop_self(&mut self) {
    self.self_scopes.pop();
}

/// What `Self` stands for here: the innermost enclosing struct, enum, trait, or `extend`
/// target. Neither a function nor a closure pushes a scope of its own, which is what lets a
/// method body and a closure inside it both see the enclosing definition's `Self`.
pub fn current_self(&self) -> Option<TyDef> {
    self.self_scopes.last().copied()
}
```

`insert_local` panicking on an empty stack is deliberate — unlike the HIR resolver's `bind`, which opened a scope implicitly. The AST resolver always opens a scope before binding, and a panic here catches a traversal bug rather than hiding it.

- [ ] **Step 4: Run the tests**

Run: `cargo test --lib nameres::surface`
Expected: PASS (all).

- [ ] **Step 5: Verify and commit**

Run: `cargo build && cargo test && cargo fmt --check && cargo clippy -- -D warnings`

```bash
git add -A
git commit -m "feat: surface SymbolTable scope stacks for locals, generics, and Self"
```

---

## Task 7: Path lookup

The module chain, single-segment lookup in both positions, and multi-segment resolution.

**Files:**
- Modify: `src/nameres/surface/symbol_table.rs`
- Modify: `src/nameres/surface/tests.rs`

**Interfaces:**
- Consumes: Tasks 4-6.
- Produces:
  ```rust
  pub fn lookup_value_path(&self, from: NodeId, path: &Path) -> Option<Res>;
  pub fn lookup_type_path(&self, from: NodeId, path: &Path) -> Option<Type>;
  pub fn lookup_mod_path(&self, from: NodeId, path: &Path) -> Option<NodeId>;
  pub fn lookup_variant(&self, enum_: NodeId, name: Symbol) -> Option<NodeId>;
  ```
  `from` is the **enclosing module's** `NodeId` — the traversal already tracks it, so no `module_of` walk is needed, unlike on the HIR side. Task 9 calls all four.

- [ ] **Step 1: Write the failing tests**

```rust
#[test]
fn a_sibling_item_resolves_without_qualification() {
    let ast = ast_from_files(&["module app; fun helper() {} fun main() {}"]);
    let table = SymbolTable::new(&ast);
    let app = table.module_by_path(&[Interner::intern("app")]).unwrap();
    assert!(matches!(
        table.lookup_value_path(app, &path(&["helper"])),
        Some(Res::Function(_))
    ));
}

#[test]
fn a_name_falls_back_to_an_ancestor_module() {
    let ast = ast_from_files(&[
        "module app; pub fun shared() {}",
        "module app::inner; fun main() {}",
    ]);
    let table = SymbolTable::new(&ast);
    let inner = table
        .module_by_path(&[Interner::intern("app"), Interner::intern("inner")])
        .unwrap();
    assert!(table.lookup_value_path(inner, &path(&["shared"])).is_some());
}

#[test]
fn a_fully_qualified_path_resolves_from_anywhere() {
    let ast = ast_from_files(&[
        "module math::vector; pub fun dot() {}",
        "module app::deep; fun main() {}",
    ]);
    let table = SymbolTable::new(&ast);
    let deep = table
        .module_by_path(&[Interner::intern("app"), Interner::intern("deep")])
        .unwrap();
    assert!(table
        .lookup_value_path(deep, &path(&["math", "vector", "dot"]))
        .is_some());
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
    let app = table.module_by_path(&[Interner::intern("app")]).unwrap();
    let g = NodeId::next();
    table.push_generics(HashMap::from([(Interner::intern("T"), Type::Generic(g))]));
    assert_eq!(table.lookup_type_path(app, &path(&["T"])), Some(Type::Generic(g)));
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
    let app = table.module_by_path(&[Interner::intern("app")]).unwrap();
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
        "module app::inner; pub struct S {}",
    ]);
    let table = SymbolTable::new(&ast);
    let app = table.module_by_path(&[Interner::intern("app")]).unwrap();
    assert!(matches!(
        table.lookup_type_path(app, &path(&["inner", "S"])),
        Some(Type::Def(TyDef::Struct(_)))
    ));
}

#[test]
fn an_unresolvable_path_is_none() {
    let ast = ast_from("fun main() {}");
    let table = SymbolTable::new(&ast);
    assert!(table.lookup_value_path(ast.root_id(), &path(&["nope"])).is_none());
}

#[test]
fn lookup_variant_finds_a_variant_by_name() {
    let ast = ast_from_files(&["module app; enum Shape { .circle, .square }"]);
    let table = SymbolTable::new(&ast);
    let app = table.module_by_path(&[Interner::intern("app")]).unwrap();
    let Some(Type::Def(TyDef::Enum(e))) = table.lookup_type_path(app, &path(&["Shape"])) else {
        panic!("Shape did not resolve to an enum");
    };
    assert!(table.lookup_variant(e, Interner::intern("circle")).is_some());
    assert!(table.lookup_variant(e, Interner::intern("triangle")).is_none());
}
```

Fix the enum literal syntax to match what the parser actually accepts — read `src/parser/item_parser.rs`'s enum tests.

- [ ] **Step 2: Run and confirm failure**

Run: `cargo test --lib nameres::surface`
Expected: FAIL to compile.

- [ ] **Step 3: Write the module chain**

```rust
/// The modules a path written inside `from` resolves against, innermost first: `from`, each
/// ancestor in turn, then the root.
///
/// Resolving against the enclosing module first is what makes a reference to a sibling item
/// work unqualified. Falling through the ancestors to the root is what keeps a fully-qualified
/// path (`math::vector::dot`) resolving from anywhere.
fn module_chain(&self, from: NodeId) -> Vec<NodeId> {
    let mut chain = Vec::new();
    let mut current = Some(from);
    while let Some(module) = current {
        chain.push(module);
        current = self.ast.parent(module);
    }
    chain
}

/// Runs `lookup` against each module in `from`'s chain, yielding the first hit, and falls back
/// to the prelude if none has one.
///
/// The prelude comes last, after the root, so it can only ever supply a name nothing else in
/// scope already does: an item the user declares shadows the core library's of the same name
/// rather than colliding with it. That is exactly the opposite of a glob import, whose names
/// enter the module's own scope and therefore do collide.
fn in_module_chain<T>(&self, from: NodeId, lookup: impl Fn(NodeId) -> Option<T>) -> Option<T> {
    self.module_chain(from)
        .into_iter()
        .chain(self.prelude)
        .find_map(lookup)
}
```

- [ ] **Step 4: Write the three path lookups**

`lookup_type_path` implements the spec's order — `Prim` → generics (innermost first) → `Self` → module chain → prelude — but only for a **single-segment** path. A multi-segment path skips straight to the module walk: `math::T` can never name a generic or a primitive.

```rust
pub fn lookup_type_path(&self, from: NodeId, path: &Path) -> Option<Type> {
    let (last, prefix) = path.segments.split_last()?;

    if prefix.is_empty() {
        if let Some(prim) = prim_ty(last.text) {
            return Some(Type::Prim(prim));
        }
        if let Some(generic) = self.lookup_generic(last.text) {
            return Some(generic);
        }
        if last.text == Interner::intern("Self") {
            return self.current_self().map(Type::Def);
        }
    }

    self.in_module_chain(from, |base| {
        let module = self.walk_modules(base, prefix)?;
        self.lookup_type_name(module, last.text).map(Type::Def)
    })
}
```

`lookup_value_path` implements locals → module chain `functions` → prelude, same single-segment gating for the local check:

```rust
pub fn lookup_value_path(&self, from: NodeId, path: &Path) -> Option<Res> {
    let (last, prefix) = path.segments.split_last()?;

    if prefix.is_empty()
        && let Some(local) = self.lookup_local(last.text)
    {
        return Some(Res::Local(local));
    }

    self.in_module_chain(from, |base| {
        let module = self.walk_modules(base, prefix)?;
        self.lookup_function(module, last.text).map(Res::Function)
    })
}
```

`lookup_mod_path` is the plain module walk, returning `Option<NodeId>`, mirroring `src/nameres/symbol_table.rs:343-349`.

`walk_modules` and `step_into_module` port directly from `src/nameres/symbol_table.rs:383-397`, with `NodeId` in place of `DefId`.

- [ ] **Step 5: Write `lookup_variant`**

```rust
/// Looks `name` up among `enum_`'s variants.
///
/// There is deliberately no way to search for a variant by name alone: the enum comes from
/// the expected type, and typeck calls this once it knows it. Scanning every enum in scope
/// for a matching variant name is exactly the ambiguity the leading `.` exists to avoid.
pub fn lookup_variant(&self, enum_: NodeId, name: Symbol) -> Option<NodeId> {
    let item = self.item(enum_)?;
    let ItemKind::Enum(e) = &item.kind else {
        return None;
    };
    e.variants
        .iter()
        .find(|v| v.name.text == name)
        .map(|v| v.id)
}
```

`ast::Ast` has no `item(NodeId)` accessor. Add one to `SymbolTable` as a private helper: keep a `HashMap<NodeId, &'ast Item>` filled during `collect`, keyed by `Item::id`. That map is also what Task 9 needs to reach a resolved item's contents, so build it once here.

- [ ] **Step 6: Run the tests**

Run: `cargo test --lib nameres::surface`
Expected: PASS (all).

- [ ] **Step 7: Verify and commit**

Run: `cargo build && cargo test && cargo fmt --check && cargo clippy -- -D warnings`

```bash
git add -A
git commit -m "feat: surface path lookup across the module chain"
```

---

## Task 8: `Self`, bare traits, and `dyn`

**Files:**
- Modify: `src/nameres/surface/symbol_table.rs`
- Modify: `src/nameres/surface/tests.rs`

**Interfaces:**
- Consumes: Task 7's `lookup_type_path`, Task 6's `current_self`, Task 4's diagnostics.
- Produces:
  ```rust
  /// Resolves a `TyKind::Dyn`'s path. Errors unless it names a trait.
  pub fn lookup_dyn_path(&self, from: NodeId, path: &Path) -> Res;
  /// Resolves a `TyKind::Path`'s path in type position, reporting and recording `Err` on
  /// failure rather than returning `None`.
  pub fn resolve_type_path(&self, from: NodeId, path: &Path) -> Res;
  ```
  Task 9 calls both instead of the raw `lookup_*` functions.

- [ ] **Step 1: Write the failing tests**

```rust
#[test]
fn a_bare_trait_path_in_type_position_resolves_to_a_trait() {
    // Static dispatch: the function is monomorphized over the concrete type, as Rust's
    // `impl Trait` does. This is legal and is not an error.
    let ast = ast_from_files(&["module app; trait Show {}"]);
    let table = SymbolTable::new(&ast);
    let app = table.module_by_path(&[Interner::intern("app")]).unwrap();
    assert!(matches!(
        table.resolve_type_path(app, &path(&["Show"])),
        Res::Type(Type::Def(TyDef::Trait(_)))
    ));
}

#[test]
fn dyn_on_a_trait_resolves() {
    let ast = ast_from_files(&["module app; trait Show {}"]);
    let table = SymbolTable::new(&ast);
    let app = table.module_by_path(&[Interner::intern("app")]).unwrap();
    assert!(matches!(
        table.lookup_dyn_path(app, &path(&["Show"])),
        Res::Type(Type::Def(TyDef::Trait(_)))
    ));
}

#[test]
fn dyn_on_a_struct_errors() {
    let ast = ast_from_files(&["module app; struct S {}"]);
    let table = SymbolTable::new(&ast);
    let app = table.module_by_path(&[Interner::intern("app")]).unwrap();
    let (res, diags) = with_diags(|| table.lookup_dyn_path(app, &path(&["S"])));
    assert_eq!(res, Res::Err);
    assert_eq!(diags.len(), 1);
    assert!(diags[0].message().contains("`dyn` requires a trait"));
}

#[test]
fn self_resolves_to_each_of_struct_enum_trait_and_extend() {
    let ast = ast_from("fun main() {}");
    let mut table = SymbolTable::new(&ast);
    for def in [
        TyDef::Struct(NodeId::next()),
        TyDef::Enum(NodeId::next()),
        TyDef::Trait(NodeId::next()),
    ] {
        table.push_self(def);
        assert_eq!(
            table.resolve_type_path(ast.root_id(), &path(&["Self"])),
            Res::Type(Type::Def(def))
        );
        table.pop_self();
    }
}

#[test]
fn self_outside_a_definition_errors() {
    let ast = ast_from("fun main() {}");
    let table = SymbolTable::new(&ast);
    let (res, diags) = with_diags(|| table.resolve_type_path(ast.root_id(), &path(&["Self"])));
    assert_eq!(res, Res::Err);
    assert_eq!(diags.len(), 1);
    assert!(diags[0].message().contains("`Self` is not available here"));
}

#[test]
fn an_unresolvable_type_path_reports_not_found_and_records_err() {
    let ast = ast_from("fun main() {}");
    let table = SymbolTable::new(&ast);
    let (res, diags) = with_diags(|| table.resolve_type_path(ast.root_id(), &path(&["Nope"])));
    assert_eq!(res, Res::Err);
    assert_eq!(diags.len(), 1);
    assert!(diags[0].message().contains("cannot find"));
}
```

Add a `with_diags<T>(f: impl FnOnce() -> T) -> (T, Vec<Diagnostic>)` helper that drains `DiagCtx` after running `f`, following whatever draining mechanism `src/diag.rs` exposes.

- [ ] **Step 2: Run and confirm failure**

Run: `cargo test --lib nameres::surface`
Expected: FAIL to compile.

- [ ] **Step 3: Implement `resolve_type_path`**

```rust
/// Resolves a type-position path, reporting and returning `Res::Err` on failure.
///
/// This is the entry point the traversal uses, rather than `lookup_type_path`: a failed
/// resolution has to be *recorded*, so that absence in `NameResolutions` keeps meaning "never
/// reached".
///
/// A bare path resolving to a trait is **legal** and means static dispatch -- the function is
/// monomorphized over the concrete type. `dyn` is the dynamic form and is a distinct node
/// kind, so the two are told apart structurally and never by inspecting the `Res`.
pub fn resolve_type_path(&self, from: NodeId, path: &Path) -> Res {
    let last = *path
        .segments
        .last()
        .expect("a path always has at least one segment");

    // `Self` is special-cased here rather than inside `lookup_type_path` so the empty-stack
    // case gets its own diagnostic instead of the generic "cannot find".
    if path.segments.len() == 1 && last.text == Interner::intern("Self") {
        return match self.current_self() {
            Some(def) => Res::Type(Type::Def(def)),
            None => {
                report_self_unavailable(last.span);
                Res::Err
            }
        };
    }

    match self.lookup_type_path(from, path) {
        Some(ty) => Res::Type(ty),
        None => {
            report_not_found(last);
            Res::Err
        }
    }
}
```

- [ ] **Step 4: Implement `lookup_dyn_path`**

```rust
/// Resolves a `TyKind::Dyn`'s path, which **must** name a trait.
///
/// Anything else is an error, recorded as `Res::Err` so the diagnostic fires once here and
/// does not cascade into typeck.
pub fn lookup_dyn_path(&self, from: NodeId, path: &Path) -> Res {
    let last = *path
        .segments
        .last()
        .expect("a path always has at least one segment");

    match self.lookup_type_path(from, path) {
        Some(Type::Def(TyDef::Trait(id))) => Res::Type(Type::Def(TyDef::Trait(id))),
        Some(_) => {
            report_dyn_not_trait(path.span);
            Res::Err
        }
        None => {
            report_not_found(last);
            Res::Err
        }
    }
}
```

Note that `dyn Self` falls into the `Some(_)` arm via `lookup_type_path`'s `Self` handling — which is correct, since `Self` never names a trait in a position where `dyn` is legal.

- [ ] **Step 5: Run the tests**

Run: `cargo test --lib nameres::surface`
Expected: PASS (all).

- [ ] **Step 6: Verify and commit**

Run: `cargo build && cargo test && cargo fmt --check && cargo clippy -- -D warnings`

```bash
git add -A
git commit -m "feat: surface Self, bare trait, and dyn resolution"
```

---

## Task 9: A shared `ast::visit`

The AST has no equivalent of `src/hir/visit.rs`. Build one, so that "what are this node's children" is answered in one place for every AST pass rather than re-derived in each — the same rationale `src/nameres.rs:56-62` already gives for the HIR visitor. Task 10's resolver, and the HIR-lowering follow-up, both consume it.

**Files:**
- Create: `src/ast/visit.rs`
- Modify: `src/ast.rs` (add `pub mod visit;`)
- Test: `src/ast/visit.rs` (a `#[cfg(test)] mod` at the end)

**Interfaces:**
- Consumes: nothing from earlier tasks.
- Produces: `ast::visit::{Visitor, walk_module, walk_item, walk_function, walk_struct, walk_enum, walk_trait, walk_extend, walk_closure, walk_generic, walk_param, walk_self_param, walk_closure_param, walk_field, walk_variant, walk_block, walk_stmt, walk_arm, walk_expr, walk_pat, walk_ty}` — signatures in Step 3. Task 10 implements `Visitor` for its resolver.

- [ ] **Step 1: Write the failing coverage test**

The one behavior worth testing is that the walk reaches everything. Count nodes.

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Default)]
    struct Counter {
        tys: usize,
        exprs: usize,
        pats: usize,
        generics: usize,
        params: usize,
        fields: usize,
    }

    impl<'ast> Visitor<'ast> for Counter {
        fn visit_ty(&mut self, ty: &'ast Ty) {
            self.tys += 1;
            walk_ty(self, ty);
        }
        fn visit_expr(&mut self, e: &'ast Expr) {
            self.exprs += 1;
            walk_expr(self, e);
        }
        fn visit_pat(&mut self, p: &'ast Pat) {
            self.pats += 1;
            walk_pat(self, p);
        }
        fn visit_generic(&mut self, g: &'ast Generic) {
            self.generics += 1;
            walk_generic(self, g);
        }
        fn visit_param(&mut self, p: &'ast Param) {
            self.params += 1;
            walk_param(self, p);
        }
        fn visit_field(&mut self, f: &'ast Field) {
            self.fields += 1;
            walk_field(self, f);
        }
    }

    #[test]
    fn the_walk_reaches_a_lets_annotation_its_initializer_and_its_pattern() {
        let ast = ast_from("fun f() { let x: i32 = 1; }");
        let mut c = Counter::default();
        c.visit_module(ast.root(), &ast);
        assert_eq!(c.tys, 1, "the `let`'s annotation was not visited");
        assert_eq!(c.exprs, 1, "the initializer was not visited");
        assert_eq!(c.pats, 1, "the binding pattern was not visited");
    }

    #[test]
    fn the_walk_reaches_generics_params_and_fields() {
        let ast = ast_from("struct S { a: i32 } fun f<T>(x: T) {}");
        let mut c = Counter::default();
        c.visit_module(ast.root(), &ast);
        assert_eq!(c.generics, 1);
        assert_eq!(c.params, 1);
        assert_eq!(c.fields, 1);
    }

    #[test]
    fn the_walk_reaches_an_extend_blocks_generics_and_methods() {
        let ast = ast_from("struct S {} extend<T> S { fun get(self) -> T {} }");
        let mut c = Counter::default();
        c.visit_module(ast.root(), &ast);
        assert!(c.generics >= 1, "extend's own generics were not visited");
        assert!(c.tys >= 1, "the method's return type was not visited");
    }

    #[test]
    fn the_walk_reaches_a_match_arms_pattern_and_body() {
        let ast = ast_from("fun f(e: i32) { match e { 1 => 2, } }");
        let mut c = Counter::default();
        c.visit_module(ast.root(), &ast);
        assert!(c.pats >= 1, "the arm's pattern was not visited");
    }
}
```

Reuse whatever `ast_from` helper the codebase already has for turning source into an `Ast` (see `src/testing.rs` and `src/nameres/tests.rs`). Fix the source snippets to syntax the parser actually accepts.

- [ ] **Step 2: Run and confirm failure**

Run: `cargo test --lib ast::visit`
Expected: FAIL to compile — the module does not exist.

- [ ] **Step 3: Write the visitor**

Model on `src/hir/visit.rs` throughout — same trait shape, same "default method calls the matching `walk_`" convention, same doc-comment style. Read that file in full first.

Two structural differences from the HIR version, both because the AST is a `Box`-linked tree rather than arenas addressed by id:

- **Visit methods take `&'ast` node references, not ids.** `fn visit_expr(&mut self, expr: &'ast Expr)`, not `fn visit_expr(&mut self, id: HirId)`. There is no arena to look an id up in.
- **Module traversal needs the `Ast`.** A `Module`'s children are `Vec<NodeId>` (`src/ast.rs:37`) resolved through `Ast::module`, so `visit_module` and `walk_module` take `&'ast Ast` alongside the module. Every other method needs only the node.

```rust
pub trait Visitor<'ast>: Sized {
    fn visit_module(&mut self, module: &'ast Module, ast: &'ast Ast) {
        walk_module(self, module, ast);
    }
    fn visit_item(&mut self, item: &'ast Item) {
        walk_item(self, item);
    }
    fn visit_function(&mut self, f: &'ast Function) {
        walk_function(self, f);
    }
    fn visit_struct(&mut self, s: &'ast Struct) {
        walk_struct(self, s);
    }
    fn visit_enum(&mut self, e: &'ast Enum) {
        walk_enum(self, e);
    }
    fn visit_trait(&mut self, t: &'ast Trait) {
        walk_trait(self, t);
    }
    fn visit_extend(&mut self, e: &'ast Extend) {
        walk_extend(self, e);
    }
    fn visit_import(&mut self, _import: &'ast Import) {}
    fn visit_generic(&mut self, g: &'ast Generic) {
        walk_generic(self, g);
    }
    fn visit_param(&mut self, p: &'ast Param) {
        walk_param(self, p);
    }
    fn visit_self_param(&mut self, _p: &'ast SelfParam) {}
    fn visit_closure_param(&mut self, p: &'ast ClosureParam) {
        walk_closure_param(self, p);
    }
    fn visit_field(&mut self, f: &'ast Field) {
        walk_field(self, f);
    }
    fn visit_variant(&mut self, v: &'ast Variant) {
        walk_variant(self, v);
    }
    fn visit_block(&mut self, b: &'ast Block) {
        walk_block(self, b);
    }
    fn visit_stmt(&mut self, s: &'ast Stmt) {
        walk_stmt(self, s);
    }
    fn visit_arm(&mut self, a: &'ast Arm) {
        walk_arm(self, a);
    }
    fn visit_expr(&mut self, e: &'ast Expr) {
        walk_expr(self, e);
    }
    fn visit_pat(&mut self, p: &'ast Pat) {
        walk_pat(self, p);
    }
    fn visit_ty(&mut self, t: &'ast Ty) {
        walk_ty(self, t);
    }
    /// A closure body. Kept separate from `visit_expr`'s other arms because a pass that opens
    /// a scope per closure needs a hook that fires exactly once per closure, and matching for
    /// `ExprKind::Closure` inside an overridden `visit_expr` would put that check on the hot
    /// path of every expression.
    fn visit_closure(&mut self, c: &'ast Closure) {
        walk_closure(self, c);
    }
}
```

Adjust the node type names to whatever `src/ast.rs` actually calls them (`Closure`, `Arm`, `Block` — confirm each exists and is exported).

- [ ] **Step 4: Write every `walk_` function**

Each destructures its node and visits every child, with **no `_ => {}` catch-all arm anywhere**. An exhaustive match on `ItemKind`, `ExprKind`, `StmtKind`, `PatKind`, and `TyKind` is the entire value of this module: it makes a later AST addition a compile error here rather than a subtree that silently stops being visited.

Points where the HIR version's shape is worth copying exactly (`src/hir/visit.rs` line numbers in parentheses):

- `walk_stmt` (:296) — visits a `let`'s annotation, its `else` block, and a `with` binding's annotation. The old resolver skipped all three silently; do not repeat that.
- `walk_expr` (:354) — the longest one; work through `ExprKind` variant by variant.
- `walk_ty` (:469) — `Array`'s `len` is an expression, not a type.
- `walk_extend` (:207) — visits `extend_generics`, `adt_generics`, `trait_generics`, and `methods`. It does **not** visit `adt_path` or `trait_path`; a `Path` is not a visitable node, and Task 10 reads those two fields directly off the `Extend`.

`walk_module` iterates `module.items` and `module.imports`, then recurses into `module.children` via `ast.module(child)`:

```rust
pub fn walk_module<'ast, V: Visitor<'ast>>(v: &mut V, module: &'ast Module, ast: &'ast Ast) {
    for import in &module.imports {
        v.visit_import(import);
    }
    for item in &module.items {
        v.visit_item(item);
    }
    for &child in &module.children {
        v.visit_module(ast.module(child), ast);
    }
}
```

- [ ] **Step 5: Run the tests**

Run: `cargo test --lib ast::visit`
Expected: PASS (4/4).

- [ ] **Step 6: Verify and commit**

Run: `cargo build && cargo test && cargo fmt --check && cargo clippy -- -D warnings`

```bash
git add -A
git commit -m "feat: add a shared ast::visit visitor"
```

---

## Task 10: The AST traversal

Drives `ast::visit` to push and pop the scope stacks and record an entry for every path written in the program.

**Files:**
- Create: `src/nameres/surface/resolver.rs`
- Modify: `src/nameres/surface.rs` (declare it, export `resolve`)
- Modify: `src/nameres/surface/tests.rs`

**Interfaces:**
- Consumes: everything from Tasks 3-9, in particular `ast::visit::Visitor`.
- Produces: `pub fn resolve(ast: &Ast) -> NameResolutions` — the pass entry point. Task 11 calls it.

- [ ] **Step 1: Write the failing integration tests**

```rust
#[test]
fn an_extend_blocks_two_paths_are_told_apart_by_what_they_name() {
    let ast = ast_from_files(&[
        "module app; struct Vec2 {} trait Show {} extend Vec2 with Show {}",
    ]);
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
fn a_block_scoped_binding_drops_at_the_closing_brace() {
    let ast = ast_from_files(&["module app; fun f() { { let x = 1; } let y = x; }"]);
    let (_, diags) = with_diags(|| resolve(&ast));
    assert!(diags.iter().any(|d| d.message().contains("cannot find `x`")));
}

#[test]
fn a_match_arm_binding_is_scoped_to_that_arm() {
    let ast = ast_from_files(&[
        "module app; enum E { .a(i32) } fun f(e: E) { match e { .a(n) => n, } let y = n; }",
    ]);
    let (_, diags) = with_diags(|| resolve(&ast));
    assert!(diags.iter().any(|d| d.message().contains("cannot find `n`")));
}

#[test]
fn a_generic_is_visible_in_a_method_of_the_extend_block_that_declares_it() {
    let ast = ast_from_files(&[
        "module app; struct S {} extend<T> S { fun get(self) -> T { } }",
    ]);
    let (_, diags) = with_diags(|| resolve(&ast));
    assert!(diags.is_empty(), "unexpected diagnostics: {diags:?}");
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
    assert_eq!(r.get(expr_id, &path(&["x"])), Some(Res::Local(Local::Variable(pat_id))));
}
```

The helpers (`extend_item_id`, `first_generic_id`, `param_ty_id`, `x_use_and_binding`) walk the parsed `Ast` to find the node under test. Write them in the test module. Fix all the source snippets to match the language the parser actually accepts — read `src/parser/item_parser.rs` and `src/parser/expr_parser.rs` tests for real syntax before writing these, and adjust the snippets rather than the assertions.

- [ ] **Step 2: Run and confirm failure**

Run: `cargo test --lib nameres::surface`
Expected: FAIL to compile — `resolve` does not exist.

- [ ] **Step 3: Write the resolver shell**

```rust
//! The AST walk that populates `NameResolutions`.
//!
//! The traversal is `ast::visit`'s, so that "what are this node's children" is answered in one
//! place for every AST pass rather than re-derived here. Only the nodes that need something
//! *around* or *instead of* the default walk are overridden: a block opens a scope, a path
//! records what it named, a binding pattern binds.

struct Resolver<'ast> {
    table: SymbolTable<'ast>,
    results: NameResolutions,
    /// The module the node currently being walked is written in. `SymbolTable`'s lookups take
    /// this as `from`, which is why no `module_of` walk is needed -- unlike on the HIR side,
    /// the traversal already tracks it.
    module: NodeId,
}

pub fn resolve(ast: &Ast) -> NameResolutions {
    let table = SymbolTable::new(ast);
    let mut r = Resolver {
        module: ast.root_id(),
        table,
        results: NameResolutions::new(),
    };
    for mod_id in ast.mod_ids() {
        r.module = mod_id;
        r.resolve_module(mod_id);
    }
    // Lang items can only be collected while the symbol table exists, but every consumer of
    // them is a later pass.
    let lang_items = crate::langitems::collect_ast(&r.table, ast.root_id());
    r.results.record_lang_items(lang_items);
    r.results
}
```

`langitems::collect` currently takes the HIR `SymbolTable` and a `DefId`. Add a parallel `collect_ast(&surface::SymbolTable, NodeId) -> LangItems` beside it rather than changing the existing one — the HIR resolver still calls the original. If `collect`'s body is small enough to share via a trait, do not bother: two short functions are clearer than one generic one, and the HIR version is scheduled for deletion.

- [ ] **Step 4: Implement `Visitor` for `Resolver` — the item overrides**

Each override follows the same shape: push whatever scopes the construct introduces, call the matching `visit::walk_*` for the children, pop. Never re-derive a node's children by hand — that is what Task 9 exists to prevent.

- `resolve_function(item_id, f)` — `push_generics` from `f.generics`, resolve each generic's bounds, `self_param`, `params`, `ret`, then `push_scope`, bind params and `self`, walk `f.block`, `pop_scope`, `pop_generics`. A function does **not** push a `Self` scope.
- `resolve_struct(item_id, s)` — `push_self(TyDef::Struct(item_id))`, `push_generics`, walk fields' types, pop both.
- `resolve_enum(item_id, e)` — same, with `TyDef::Enum`, walking each variant's payload.
- `resolve_trait(item_id, t)` — same, with `TyDef::Trait`, then each function.
- `resolve_extend(item_id, e)` — resolve `e.adt_path` and `e.trait_path` **against `item_id`** (both are entries on the enclosing `Item`), `push_generics` from `e.extend_generics`, `push_self` with whatever `adt_path` resolved to, walk `adt_generics`/`trait_generics`/`methods`, pop both.

For `extend`, guard the duplicate-path invariant: if `adt_path == trait_path`, report a conflict on `trait_path`'s span and record only `adt_path`. That is the `extend Foo with Foo` case the spec names.

For a generic's bounds, guard the same way: skip and report a conflict for a bound equal to one already recorded on that `Generic` node (`T: Show + Show`).

If `adt_path` does not resolve to a `TyDef`, push nothing onto `self_scopes` — a `Self` inside then reports "not available". This over-reports slightly versus the HIR resolver's `SelfTyRes::Err`; if that shows up as noise in the tests, add an `Err` marker to the self stack instead and note the deviation in the report.

- [ ] **Step 5: The type, expression, statement, and pattern overrides**

Each of these overrides a `visit_*` method and calls the matching `visit::walk_*` for its children.

- `TyKind::Path { path, args }` → `record(ty.id, path.clone(), table.resolve_type_path(module, path))`, then walk `args`.
- `TyKind::Dyn { path, args }` → `record(ty.id, path.clone(), table.lookup_dyn_path(module, path))`, then walk `args`.
- `TyKind::Ref`/`Any`/`Tuple`/`Array`/`Function` → walk children; `Array`'s `len` is an expression.
- `TyKind::Error` → nothing.
- `ExprKind::Path(path)` → `record(expr.id, path.clone(), table.lookup_value_path(module, path).unwrap_or_else(|| { report_not_found(last); Res::Err }))`.
- Blocks → `push_scope` / walk / `pop_scope`.
- Match arms → `push_scope` around the pattern **and** the body together, since an arm's bindings are scoped to both.
- `StmtKind::Let` → walk the initializer and the annotation **first**, then bind the pattern. A `let x = x;` must see the outer `x` on the right-hand side.
- `StmtKind::While`/`WhileLet`/`For` → scope around the pattern and body as for arms.
- A binding `Pat` → `table.insert_local(name, Local::Variable(pat.id))`.
- Closures → `push_scope`, bind `ClosureParam`s, walk the body, `pop_scope`. A closure pushes **no** generic or `Self` scope, so it sees its enclosing definition's.

Only override what needs something around or instead of the default walk. Anything that just needs its children visited is already handled by `ast::visit`'s default — do not override it. That is the difference between this and re-deriving the traversal.

- [ ] **Step 6: Run the tests**

Run: `cargo test --lib nameres::surface`
Expected: PASS (all).

- [ ] **Step 7: Verify and commit**

Run: `cargo build && cargo test && cargo fmt --check && cargo clippy -- -D warnings`

```bash
git add -A
git commit -m "feat: surface AST traversal producing NameResolutions"
```

---

## Task 11: Span-ordered debug dump and golden tests

**Files:**
- Modify: `src/driver/emit_debug.rs`
- Modify: `src/driver/cli.rs` (if a new dump flag is needed)
- Modify: `src/nameres/surface/tests.rs`

**Interfaces:**
- Consumes: Task 10's `resolve`.
- Produces: `emit_debug::print_surface_nameres(&Ast, &NameResolutions)`.

- [ ] **Step 1: Write the failing ordering test**

```rust
#[test]
fn the_dump_is_ordered_by_span_and_contains_no_node_ids() {
    let ast = ast_from_files(&["module app; struct A {} struct B {} fun f(x: B, y: A) {}"]);
    let r = resolve(&ast);
    let dump = emit_debug::surface_nameres_to_string(&ast, &r);

    // `B` is written before `A` in source order, so it must come first regardless of the
    // `NodeId`s the global counter happened to hand out.
    let b = dump.find("B").expect("B missing from dump");
    let a = dump.find("A").expect("A missing from dump");
    assert!(b < a, "dump is not span-ordered:\n{dump}");

    assert!(
        !dump.contains("NodeId"),
        "NodeId leaked into the dump:\n{dump}"
    );
}
```

- [ ] **Step 2: Run and confirm failure**

Run: `cargo test --lib nameres::surface`
Expected: FAIL to compile.

- [ ] **Step 3: Write the dump**

Read `src/driver/emit_debug.rs`'s existing `print_ast`/`print_hir`/`print_nameres` and match their formatting conventions exactly. Two rules override anything you find there:

1. **Sort every entry by source span before printing.** Collect `(SrcSpan, &Path, Res)` triples across all of `NameResolutions`, sort by the span's start offset, then print.
2. **Never print a `NodeId`.** Render a `Res` by what it names — the item's name and its span — not by its id. `Res::Function(id)` prints as the function's name; `Res::Local(Local::Variable(id))` as the binding's name; and so on. Use the `NodeId -> &Item` map from Task 7 (or add equivalent lookups) to recover names.

Both rules exist for the same reason: `NodeId` comes from a global atomic counter and is deterministic only while parsing is sequential. The moment parsing goes parallel — the stated reason `NodeId` is global at all — a golden file keyed on `NodeId` ordering starts to flap.

Factor the body as `surface_nameres_to_string(&Ast, &NameResolutions) -> String` with `print_surface_nameres` as a thin `println!` wrapper, so the test above can assert on the string.

- [ ] **Step 4: Wire it to a flag**

Add the dump behind the existing `--emit-debug`/dump-flag mechanism in `src/driver/cli.rs` and call it from `src/driver/pipeline.rs` — but only as an *additional* dump. Do **not** change the pipeline's pass order or replace the existing `resolve(&hir)` call. If wiring it into `pipeline.rs` means running `surface::resolve(&ast)` a second time alongside the HIR resolver, that is correct and intended for this plan: the two coexist until the follow-up.

- [ ] **Step 5: Run the tests**

Run: `cargo test --lib nameres::surface`
Expected: PASS.

- [ ] **Step 6: Full verification**

Run: `cargo build && cargo test && cargo fmt --check && cargo clippy -- -D warnings`
Expected: all pass. Run the compiler against the real core library and confirm the new dump emits without panicking and without new diagnostics that the HIR resolver does not also emit.

- [ ] **Step 7: Commit**

```bash
git add -A
git commit -m "feat: span-ordered surface nameres debug dump"
```

---

## The migration: Tasks 12-15

Tasks 1-11 build the AST resolver alongside the live HIR one. Tasks 12-15 switch the pipeline over and delete the old resolver.

**The design.** The HIR bridge does **not** re-key a side table from `NodeId` to `HirId`. Instead `hir::Path` becomes its own type carrying its resolution inline:

```rust
pub struct Path {
    pub segments: Vec<Ident>,
    pub span: SrcSpan,
    pub res: Res,
}
```

HIR nodes keep their `path` fields, so diagnostics and the debug dump still have the written name and span, and the HIR-side `NameResolutions` disappears entirely rather than being re-keyed.

**Why this is the only route that deletes `SelfTyRes`.** `hir::Extend`'s `adt_path` and `trait_path` are bare `Path` fields with no `HirId` of their own (`src/hir/items.rs:126-127`). `resolve_item.rs:207-231` therefore stores both resolutions in `self_tys`, keyed by the `Extend`'s `DefId`, because the per-node `HirId -> TypeRes` table has room for only one entry per node. Giving `Path` its own `res` is what gives a node's second path somewhere to live. This only works once resolution runs *before* lowering, which is Task 14.

---

### Task 12: Pre-allocate a `DefId` for every definition

Today `src/hir/lower.rs:32` pre-allocates `DefId`s only for modules; functions, structs, enums, traits, and `extend` blocks get theirs lazily inside `lower_item`. A forward reference (`x: Foo` lowered before `Foo`) therefore has no `DefId` to write into a node. Task 14 needs every definition's id up front.

**Files:**
- Modify: `src/hir/lower.rs` (extend the pre-allocation loop)
- Modify: `src/hir/lower/ctx.rs` (take ids from the map rather than allocating)
- Modify: `src/hir/lower/tests.rs`

**Interfaces:**
- Consumes: nothing from Tasks 1-11.
- Produces: `LoweringCtx` gains `def_ids: HashMap<NodeId, DefId>`, populated before any lowering, mapping every `Item`'s `NodeId` (and every module's) to its `DefId`. Task 14 reads it.

- [ ] **Step 1: Write the failing test**

```rust
#[test]
fn every_definition_has_a_def_id_before_any_body_is_lowered() {
    let ast = ast_from("struct A {} fun f() {} enum E { .x } trait T {} extend A {}");
    let hir = lower_unit(&ast);
    // Five definitions plus the root module.
    assert_eq!(hir.def_ids().count(), 6);
}

#[test]
fn a_forward_reference_resolves_to_an_already_allocated_def_id() {
    // `Foo` is declared after the function that names it.
    let ast = ast_from("fun f(x: Foo) {} struct Foo {}");
    let hir = lower_unit(&ast);
    assert_eq!(hir.def_ids().count(), 3);
}
```

- [ ] **Step 2: Run and confirm failure**

Run: `cargo test --lib hir::lower`
Expected: the second test may already pass; the point is that neither regresses. If both pass before the change, keep them as regression tests and say so in your report.

- [ ] **Step 3: Extend the pre-allocation loop**

`lower_unit` currently walks `ast.mod_ids()` allocating module `DefId`s. Extend it: after (or within) that loop, walk each module's `items` and allocate a `DefId` for every `ItemKind::Function`/`Struct`/`Enum`/`Trait`/`Extend`, parented to that module, keyed by the `Item`'s `NodeId`.

Order matters for the same reason the existing comment gives: a parent's `DefId` must exist before its children ask for it. Modules are allocated first, in `ast.mod_ids()` order (parents before children), then items within each module.

Methods of a trait or `extend` block are parented to that trait or block, not to the module — see `lower_trait`/`lower_extend` in `src/hir/lower/ctx.rs:159-190`. Allocate them in a second nested pass once their parent item has an id.

- [ ] **Step 4: Make lowering consume the map**

`lower_item`, `lower_function`, `lower_struct`, `lower_enum`, `lower_trait`, and `lower_extend` currently call `self.items.alloc(Some(parent))`. Change each to look its id up in `def_ids` instead. Keep `DefIdAllocator` — it is what filled the map.

Preserve the invariant `src/hir.rs:43-53` documents: every `DefId` slot must end up with an arena, so `finish` can order them into a dense `Vec<Arena>`. Allocating ids for definitions that are never lowered would break it. Verify the counts match.

- [ ] **Step 5: Run the tests**

Run: `cargo test`
Expected: all pass, including the existing lowering tests.

- [ ] **Step 6: Verify and commit**

Run: `cargo build && cargo test && cargo fmt --check && cargo clippy -- -D warnings`

```bash
git add -A
git commit -m "refactor: pre-allocate a DefId for every definition"
```

---

### Task 13: Lower generics before methods in traits and `extend` blocks

`lower_trait` (`src/hir/lower/ctx.rs:162`) and `lower_extend` (`:184`) lower their methods *before* their own generics. A method body naming its `extend` block's `<T>` needs a generic node that does not exist yet, which Task 14 cannot work around.

This is not a statement swap. The current order exists because `self.lower_function` needs `&mut LoweringCtx` while the `OwnerLowerer` holds that borrow. Restructuring that borrow is the substance of this task and the main risk in the migration.

**Files:**
- Modify: `src/hir/lower/ctx.rs` (`lower_trait`, `lower_extend`)
- Modify: `src/hir/lower/owner.rs` (if the borrow restructure needs it)
- Modify: `src/hir/lower/tests.rs`

**Interfaces:**
- Consumes: Task 12's `def_ids` map.
- Produces: for every owner, the generics it declares are lowered before any nested owner that could name them. Task 14 depends on this.

- [ ] **Step 1: Write the failing test**

```rust
#[test]
fn an_extend_blocks_generics_are_lowered_before_its_methods() {
    let ast = ast_from("struct S {} extend<T> S { fun get(self) -> T {} }");
    let hir = lower_unit(&ast);
    let extend = find_extend(&hir);
    let method = extend.methods[0];
    // The generic node must already exist in the extend's arena when the method's arena is
    // built, which shows up as the generic's HirId being allocated first.
    assert!(!extend.extend_generics.is_empty());
    assert!(hir.arena(method).owner().span().start >= hir.generic(extend.extend_generics[0]).span.start);
}

#[test]
fn a_traits_generics_are_lowered_before_its_functions() {
    let ast = ast_from("trait C<T> { fun get(self) -> T; }");
    let hir = lower_unit(&ast);
    let trait_ = find_trait(&hir);
    assert!(!trait_.generics.is_empty());
    assert_eq!(trait_.functions.len(), 1);
}
```

These are weak assertions — ordering inside lowering is hard to observe directly. Prefer strengthening them: if you can add a debug assertion inside lowering that a nested owner is never built before its parent's generics, do that and test the assertion fires on the old order. Say in your report which approach you took.

- [ ] **Step 2: Run and confirm failure**

Run: `cargo test --lib hir::lower`

- [ ] **Step 3: Restructure the borrow**

The shape to reach, for both functions: create the `OwnerLowerer`, reserve the root, lower the generics, then release the borrow before lowering the nested functions, then re-acquire to `fill` and `finish`.

`OwnerLowerer::new(self, item_id)` borrows `LoweringCtx` mutably. Options, in order of preference:

1. Split `OwnerLowerer` so the generic-lowering phase can finish and hand back its partial arena, with `lower_function` called between phases. Cleanest if `ArenaBuilder` supports being put down and picked up.
2. Lower the generics into a detached `ArenaBuilder` not holding the `LoweringCtx` borrow, then merge.
3. Collect the generic nodes first without the `OwnerLowerer` at all, since a `Generic`'s children are only its bound paths.

Read `src/hir/lower/owner.rs` in full before choosing. Whichever you pick, keep the doc comment at `src/hir/lower/ctx.rs:160-162` accurate — it currently explains the *old* order and its reason, and must be rewritten to explain the new one.

- [ ] **Step 4: Run the full suite**

Run: `cargo test`
Expected: all pass. Lowering is load-bearing for every later pass, so a regression here shows up far away — do not proceed on a partial pass.

- [ ] **Step 5: Verify and commit**

Run: `cargo build && cargo test && cargo fmt --check && cargo clippy -- -D warnings`

```bash
git add -A
git commit -m "refactor: lower generics before nested owners in traits and extend blocks"
```

---

### Task 14: `hir::Path` carries its resolution; reorder the pipeline

The switch-over. Resolution runs on the AST, lowering consumes its output, and every `hir::Path` is built with its answer already in it.

**Files:**
- Create: `src/hir/path.rs` (or add to `src/hir/types.rs`) — the new `hir::Path`
- Modify: `src/hir/items.rs`, `src/hir/types.rs`, `src/hir/expr.rs` (the `Path` fields)
- Modify: `src/hir/lower.rs`, `src/hir/lower/{ctx,items,ty,expr,pat,block}.rs`
- Modify: `src/driver/pipeline.rs`
- Modify: `src/hir/lower/tests.rs`

**Interfaces:**
- Consumes: Task 10's `surface::resolve`, Task 11's dump, Task 12's `def_ids`, Task 13's ordering.
- Produces:
  - `hir::Path { segments: Vec<Ident>, span: SrcSpan, res: hir::Res }`
  - `hir::Res` — the `HirId`/`DefId`-carrying analogue of `surface::Res`.
  - `lower_unit(ast: &Ast, res: &surface::NameResolutions) -> Hir`
  - Pipeline order becomes `parse -> surface::resolve -> lower_unit -> typeck`.

- [ ] **Step 1: Define `hir::Res`**

Mirror `surface::Res` arm for arm, with HIR ids in place of `NodeId`:

```rust
pub enum Res {
    Type(Type),
    Local(Local),
    Function(DefId),
    Module(DefId),
    Err,
}

pub enum Type {
    Prim(PrimTy),
    Generic(HirId),
    Def(TyDef),
}

pub enum TyDef {
    Struct(DefId),
    Enum(DefId),
    Trait(DefId),
}

pub enum Local {
    Param(HirId),
    SelfParam(HirId),
    Variable(HirId),
}
```

Nominal items and functions carry `DefId` because they *are* definitions; locals and generics carry `HirId` because they are arena nodes with no `DefId` of their own (`src/hir/ids.rs:5-12`).

- [ ] **Step 2: Write the translation**

`LoweringCtx` gains `node_to_hir: HashMap<NodeId, HirId>`, filled as each node is lowered, and translates a `surface::Res` into a `hir::Res` at the point a `Path` is built:

- `surface::Res::Function(node)` → `Res::Function(def_ids[&node])` — available from Task 12 regardless of lowering order.
- `surface::Res::Type(Type::Def(TyDef::Struct(node)))` → `TyDef::Struct(def_ids[&node])`, and likewise for enum and trait.
- `surface::Res::Type(Type::Generic(node))` → `Type::Generic(node_to_hir[&node])` — available because Task 13 lowers generics before anything that can name them.
- `surface::Res::Local(..)` → the matching `Local` via `node_to_hir` — available because bindings lower before uses in source order, and `HirId` is global so a closure capturing an outer local works.
- `surface::Res::Type(Type::Prim(p))` → `Type::Prim(p)`; `Err` → `Err`.

If a `node_to_hir` lookup misses, that is a lowering-order bug, not a resolution failure. Panic with a message naming the `NodeId` and what it was expected to be — do not silently produce `Res::Err`, which would hide the bug as a type error somewhere else.

- [ ] **Step 3: Replace `ast::Path` with `hir::Path` in HIR nodes**

`Extend::adt_path`, `Extend::trait_path`, `Generic::bounds`, `TyKind::Path`, `TyKind::Dyn`, `ExprKind::Path`, `ExprKind::Ctor`'s `path`, `Module::path`, `Import::path` (see the grep at `src/hir/items.rs:7`). Each is built by looking the AST path's answer out of `NameResolutions` with `get(owner_node_id, path)`.

`Module::path` and `Import::path` name no resolvable target in the same sense — keep them as `ast::Path`, or give them `Res::Err`, whichever reads better. State which you chose and why in your report.

- [ ] **Step 4: Reorder the pipeline**

In `src/driver/pipeline.rs::check`, move resolution ahead of lowering:

```rust
let ast = parse(lex());
if options.dumps.ast { emit_debug::print_ast(&ast); }

let nameres = surface::resolve(&ast);
if options.dumps.nameres { emit_debug::print_surface_nameres(&ast, &nameres); }

let hir = lower_unit(&ast, &nameres);
if options.dumps.hir { emit_debug::print_hir(&hir, options.exclude_core_in_emit); }

let checked = typeck::check(&hir, &nameres);
```

`typeck::check`'s second argument still takes the old `NameResolutions` at this point — Task 15 ports it. Keep both alive for exactly one task by having the pipeline still run the old `resolve(&hir)` for typeck's benefit. This is the one place a temporary double-run is correct, because it keeps Task 14 independently reviewable; Task 15 deletes it.

- [ ] **Step 5: Run the full suite**

Run: `cargo test`
Expected: all pass. The old resolver still runs, so typeck's behavior should be unchanged.

- [ ] **Step 6: Verify and commit**

Run: `cargo build && cargo test && cargo fmt --check && cargo clippy -- -D warnings`

```bash
git add -A
git commit -m "feat: hir::Path carries its resolution; resolve before lowering"
```

---

### Task 15: Port `typeck` and `langitems`; delete the old resolver

**Files:**
- Modify: `src/typeck/lower_ty.rs`, `src/typeck/traits/*.rs`, `src/typeck.rs`
- Modify: `src/langitems.rs`
- Modify: `src/driver/pipeline.rs` (drop the double-run)
- Delete: `src/nameres/{symbol_table,results,resolve_expr,resolve_item,resolve_ty}.rs`, `src/nameres/tests.rs`
- Modify: `src/nameres.rs` (flatten `surface` up, or re-export it)

**Interfaces:**
- Consumes: Task 14's `hir::Path` and `hir::Res`.
- Produces: `typeck::check(&Hir)` — no resolutions argument; every answer is in the nodes.

- [ ] **Step 1: Port `lower_ty`**

`lower_base` currently reads `self.nameres.ty(id)`. It now reads the `Path`'s own `res`. The `TypeRes::Def` arm's logic is unchanged apart from the source of its input.

`HirTyKind::SelfType` and `self_ty` are what this task exists to delete. `Self` now arrives as a `TyKind::Path` whose `res` is `Type::Def(TyDef::Struct|Enum|Trait)` — resolved by `surface::SymbolTable::current_self` back in Task 8. But `lower_base`'s `Def` arm cannot handle it as-is, for the two reasons Task 1 documents:

1. It reports `report_trait_as_ty` for a trait, and `Self` inside a trait body is legal.
2. It rejects `args.len() != declared`, and `Self` is written with no arguments.

So `Self` needs its own arm. Add a `res` discriminator distinguishing "this path was written as `Self`" — the simplest is a `Res::SelfTy(TyDef)` arm on `hir::Res`, produced by Task 14's translation when the AST path's single segment is `Self`. Its `lower_base` arm is today's `self_ty` body (`src/typeck/lower_ty.rs:200-239`): apply the struct's or enum's own parameters, the `extend` block's target generics, or the trait's `SelfTy` placeholder. Move that body across unchanged; only its input changes.

Add the `Res::SelfTy` arm to `hir::Res` as part of this task, not Task 14, so Task 14 stays reviewable on its own.

- [ ] **Step 2: Port the `trait_` consumers**

Anything that asked `self_ty(def)` for its `trait_` companion now reads the `Extend` node's own `trait_path.res` — which is where Task 14 put it, and the whole reason `hir::Path` carries a `res`. Find them with `grep -rn "self_ty\|SelfTyRes" src/typeck/ src/langitems.rs`.

`src/typeck/traits/members.rs`'s `TyKind::SelfTy(_)` is a *type-level* placeholder in typeck's own type layer, unrelated to `SelfTyRes`. Leave it alone.

- [ ] **Step 3: Port `langitems`**

`langitems::collect` takes the HIR `SymbolTable`. Replace it with the `collect_ast` written in Task 10 and delete the original.

- [ ] **Step 4: Delete the old resolver**

Remove the five modules and `src/nameres/tests.rs`, drop the double-run from `pipeline.rs`, and move `surface`'s contents up into `src/nameres/` so the module reads as the resolver rather than as one of two. Rename `surface::NameResolutions` to `NameResolutions` — the Global Constraint that kept the spec's names module-qualified expires here.

`PrimTy` currently lives in the deleted `src/nameres/results.rs` (`:9`) and is used by `surface::res` and `typeck`. Move it rather than deleting it.

- [ ] **Step 5: Verify against the real core library**

Run: `cargo build && cargo test && cargo fmt --check && cargo clippy -- -D warnings`

Then compile the core library end to end and diff the diagnostics against `master`'s output. Any new or missing diagnostic is a regression — report it rather than accepting it.

- [ ] **Step 6: Commit**

```bash
git add -A
git commit -m "refactor: delete the HIR-based resolver; typeck reads resolutions from nodes"
```
