# AST-Level Symbol Table and Name Resolution

**Date:** 2026-08-07
**Status:** Approved design, not yet implemented

## Summary

Name resolution moves off the HIR and onto the AST. This document specifies the AST-level
`SymbolTable`, the `Res` family of resolution results, the `NameResolutions` side table those
results are recorded in, and the contract by which HIR lowering carries that table forward.

This realizes the pipeline `docs/architecture.md` already declares:

> Name Resolution operates on the AST to produce a side table mapping `Path`s in the AST to
> references to Nodes in the AST. Since paths cannot uniquely identify variables (such as in the
> case of variable shadowing), we identify using each node's `NodeId` and append a list of two
> tuples where the first element is a Path owned by the node and the second element is what that
> Path names.

## Pipeline position

```
parse -> Ast -> nameres(&Ast) -> NameResolutions -> lower(&Ast, &NameResolutions) -> Hir -> typeck
```

Today's resolver runs *after* lowering and keys its output by `HirId`/`DefId`. Under this design
resolution runs first, and lowering becomes a consumer: as it walks, it re-keys the AST-side table
into a `HirId`-keyed one, so typeck, the trait solver, and lang items keep their existing
`results.value(hir_id)` shaped API untouched.

## Scope

**In scope**

- The AST-level `SymbolTable`: types, construction algorithm, lookup order, public API, invariants.
- `Res`, `Type`, `TyDef`, `Local`, `ModuleScope`, `NameResolutions`.
- Module collection, import and glob resolution, prelude discovery.
- Diagnostics and error cases.
- The HIR bridge *contract* — the shape of the re-keying and the `NodeId -> HirId` map it needs.

**Out of scope** (follow-up spec)

- Rewiring `src/hir/lower/*` to consume `NameResolutions`.
- Porting `typeck`, `typeck/traits`, and `langitems` call sites.
- Deleting the `HirId`-based `src/nameres/symbol_table.rs`.
- The `driver::pipeline` reorder.

## Required AST changes

### 1. `TyKind::SelfType` is removed

`Self` parses as an ordinary `TyKind::Path` whose single segment is the symbol `Self`. The
resolver special-cases that symbol against the enclosing definition and answers with a `TyDef`.

This deletes the `self_tys` side table outright. Every written `Self` becomes an ordinary path
entry — keyed, recorded, and looked up like any other path — rather than a property of the
definition it appears inside, tracked in a table of its own.

Touches `src/parser/type_parser.rs:95`, `src/ast.rs:281`, and the `SelfType` arms in
`src/hir/lower/ty.rs:33` and `src/hir/visit.rs:502`.

### 2. `TyKind::Dyn { path, args }` stays a distinct variant

`dyn`-ness is structural: it is carried by the node kind, never by the `Res`. A `Res` answers only
*what the path named*, not how it is dispatched. Keeping the variant is what makes the two forms
separately checkable — see "Traits in type position".

### 3. `Path` gains `PartialEq`, `Eq`, and `Hash` over segments only

Defined over `segments[..].text`, **ignoring every span**. A `Path` identifies a name, not a
source location; two writings of `math::vector` are the same path. Entry lookup within a node
depends on this — see "Entry lookup".

## Data model

### Resolution results

```rust
enum Res {
    Type(Type),
    Local(Local),
    Function(NodeId),   // the Item wrapping a `fun`
    Module(NodeId),     // the Module
    Err,
}

enum Type {
    Prim(PrimTy),       // i32, bool, ... — never gets a NodeId
    Generic(NodeId),    // the ast::Generic that declares it
    Def(TyDef),
}

enum TyDef {
    Struct(NodeId),
    Enum(NodeId),
    Trait(NodeId),
}

enum Local {
    Param(NodeId),      // ast::Param
    SelfParam(NodeId),  // ast::SelfParam
    Variable(NodeId),   // the binding ast::Pat
}
```

`Type` nests rather than flattening so that a type-position lookup has exactly one return type and
consumers narrow once. `TyDef` combines struct, enum, and trait because all three are nominal
items sharing one namespace; a consumer that needs only "a nominal item, give me its `NodeId`"
matches a single arm.

`SelfParam` is kept apart from `Variable` because `self` is not an ordinary local: it carries a
`SelfMode` rather than a declared type, and its type is the enclosing item's `Self`. Every consumer
handles it specially anyway; a distinct variant is what forces that to be exhaustive.

There is deliberately no `Variant` arm. A `.variant` names no enum of its own — the enum comes from
the expected type, so typeck resolves it once it knows that type. Scanning every enum in scope for
a matching variant name is exactly the ambiguity the leading `.` exists to avoid.

`Err` is *recorded*, never left absent. Absence must mean "never reached"; conflating it with
"resolved, unsuccessfully" leaves every consumer to tell the two apart from context it does not
have.

### The symbol table

```rust
pub struct SymbolTable<'ast> {
    local_scopes:   Vec<HashMap<Symbol, Local>>,
    generic_scopes: Vec<HashMap<Symbol, Type>>,
    self_scopes:    Vec<TyDef>,
    modules:        HashMap<NodeId, ModuleScope>,
    by_path:        HashMap<Box<[Symbol]>, NodeId>,
    prelude:        Option<NodeId>,
    ast:            &'ast Ast,
}

struct ModuleScope {
    functions: HashMap<Symbol, NodeId>,
    types:     HashMap<Symbol, TyDef>,
    mods:      HashMap<Symbol, NodeId>,
}
```

**Three scope stacks, one mechanism.** Locals, generics, and `Self` are all pushed on entering the
construct that introduces them and popped on leaving it. Locals push per block and per match arm;
generics push per definition declaring `<...>`; `Self` pushes per struct, enum, trait, and
`extend`. A method seeing its `extend` block's `<T>` falls out of the stack rather than requiring
an owner-chain walk against a side table — which is why `NameResolutions` needs no `generics`
table.

**Modules are keyed by `NodeId`**, which is what `Res::Module` carries, with `by_path` mapping a
canonical span-free `Box<[Symbol]>` to that id so a fully-qualified path resolves in one hash
rather than a segment-by-segment walk.

`ModuleScope.types` holds `TyDef`, not `Type`: `Prim` and `Generic` can never live in a module's
namespace, and the narrower type says so.

### The output

```rust
pub struct NameResolutions {
    paths:      HashMap<NodeId, SmallVec<[(Path, Res); 2]>>,
    lang_items: LangItems,
}
```

One table plus the lang items. Today's `bounds`, `self_tys`, and `generics` tables are all
subsumed:

- **`bounds`** — an `ast::Generic` owns its bound `Path`s, so they are simply further entries in
  that node's list, in source order.
- **`self_tys`** — subsumed by the `SelfType` removal above.
- **`generics`** — replaced by `generic_scopes`, which is resolver-internal state, not output.

`lang_items` remains pass output: resolving it is name resolution's job and can only be done while
the symbol table exists, but every consumer of it is a later pass.

**`SmallVec<[_; 2]>` is inline capacity, not a cap.** Two is the common maximum — `extend Vec<T>
with Show` puts `adt_path` and `trait_path` on one `Item` node. A generic with three bounds spills
to the heap and remains correct.

**Which node owns which paths.** `Extend`, `Function`, `Struct`, `Enum`, and `Trait` have no
`NodeId` of their own; they sit inside `Item`, which does (`src/ast.rs:85`). So an `extend` block's
two paths key off the enclosing `Item`. `ast::Generic` (`src/ast.rs:200`) and `ast::Ty`
(`src/ast.rs:255`) each carry their own id.

### Entry lookup

```rust
pub fn get(&self, owner: NodeId, path: &Path) -> Option<Res>;
pub fn entries(&self, owner: NodeId) -> &[(Path, Res)];
```

`get` matches on the path's **segment symbols**, per AST change (3). For `extend Vec<T> with Show`,
the caller holding `adt_path` matches `[Vec]` and the caller holding `trait_path` matches `[Show]`.
No positional contract and no role discriminant — nothing for a later edit to silently invalidate.

**Invariant: within one node, no two entries may have equal paths.** A node owning two textually
identical paths (`extend Foo with Foo`, or a duplicate bound `T: Show + Show`) is rejected by the
check that owns it. This is an ordinary duplicate-name rule, and making it an invariant is what
leaves `get` unambiguous by construction rather than by a first-match tiebreak.

## Construction

`SymbolTable::new(&Ast)` runs three ordered phases.

**1. Collect.** Recurse the module tree from `ast.root_id()`, building one `ModuleScope` per
module. Functions land in `functions`; structs, enums, and traits in `types`; submodules in `mods`.
Every insert goes through a conflict check.

**2. Resolve imports.** Runs only after *every* module's scope exists, not just the current
subtree's: an import may name any module in the tree by absolute path, including one the collect
pass would not have reached yet. Each import resolves **absolutely from the root** regardless of
where it is written, then binds into the *importing* module's own scope. After this pass, an
imported name is looked up exactly like a declared one — there is no separate "imports" concept
anywhere downstream.

A glob (`import math::*;`) copies every name from the source module's three maps into the
destination's, through the same conflict checks as an ordinary declaration. There is no
"imports don't conflict with declarations" carve-out: a glob is meant to behave as if its contents
had been spelled out by hand, and a hand-written duplicate would conflict too. Since a glob names
no item individually, conflicts are blamed on the `import` statement's own span.

**3. Find prelude.** Walk `core::prelude` down from the root. This must follow imports, because the
prelude's namespace *is* the set of imports it declares. `None` if the core library is not part of
the unit — which should not happen in a real build, but leaves the resolver working rather than
panicking if it is ever driven without one.

## Lookup

### Single-segment, type position

`Prim` → `generic_scopes` (innermost first) → `Self` → module chain → prelude.

Primitives are checked **first**, and declaring an item named after one is rejected at collect time
with a dedicated diagnostic. This is the duplicate-name rule applied to builtins, and it is cheaper
than making every `i32` walk the whole chain.

### Single-segment, value position

`local_scopes` (innermost first) → module chain `functions` → prelude.

Shadowing works because a later `let` overwrites the entry in the innermost map; the outer binding
is restored when that scope pops.

### The module chain

For a path written in module `M`: `M`, then each ancestor in turn, then the root, then the prelude.

Resolving against the enclosing module first is what makes a reference to a sibling item work
unqualified. Falling through the ancestors to the root is what keeps a fully-qualified path
(`math::vector::dot`) resolving from anywhere. The prelude sits last, after the root, so it can
only ever supply a name nothing else in scope already does — an item the user declares shadows the
core library's of the same name rather than colliding with it. This is exactly the opposite of a
glob import, whose names enter the module's own scope and therefore do collide.

### Multi-segment

Pick a base from the chain, walk all but the last segment through `mods`, then look the final
segment up in the target namespace. The first base yielding a hit wins.

## `Self`, traits, and `dyn`

**`Self`** reads the innermost `self_scopes` entry. Inside a method or closure this is the
enclosing struct, enum, trait, or `extend` target, since neither a function nor a closure pushes a
scope of its own. An empty stack is an error.

One consequence to note: today's `SelfTyRes::Ty { adt, trait_ }` carried a `trait_` companion that
a single `Res` cannot express. Anything asking "which trait is this `extend` implementing" reads
the `Extend` item's own `trait_path` entry out of the path table instead — which is already
recorded there, so nothing is lost.

**Traits in type position.** A bare `TyKind::Path` resolving to `TyDef::Trait` is **legal**. It
means static dispatch: the function is monomorphized over the concrete type, as Rust's `impl Trait`
does. It resolves to `Res::Type(Type::Def(TyDef::Trait(id)))` like any other nominal item.

`TyKind::Dyn { path, args }` is the dynamic-dispatch form. Its path **must** resolve to a
`TyDef::Trait`; anything else is an error, recorded as `Res::Err` so the diagnostic fires once here
and does not cascade into typeck.

The two forms are therefore distinguished structurally, by node kind, and never by inspecting the
`Res`.

## Public API

```rust
impl<'ast> SymbolTable<'ast> {
    pub fn new(ast: &'ast Ast) -> Self;

    pub fn push_scope(&mut self);
    pub fn pop_scope(&mut self);
    pub fn insert_local(&mut self, name: Ident, local: Local);
    pub fn lookup_local(&self, name: Symbol) -> Option<Local>;

    pub fn push_generics(&mut self, params: HashMap<Symbol, Type>);
    pub fn pop_generics(&mut self);
    pub fn push_self(&mut self, ty: TyDef);
    pub fn pop_self(&mut self);

    pub fn lookup_value_path(&self, from: NodeId, path: &Path) -> Option<Res>;
    pub fn lookup_type_path (&self, from: NodeId, path: &Path) -> Option<Type>;
    pub fn lookup_mod_path  (&self, from: NodeId, path: &Path) -> Option<NodeId>;
    pub fn lookup_variant   (&self, enum_: NodeId, name: Symbol) -> Option<NodeId>;
}
```

`insert_local` takes an `Ident`, not a `Path`: a local is always one segment, and accepting a
`Path` would imply `let a::b = ...` is representable. It returns `()`, not `Result` — shadowing is
legal, so the innermost map is simply overwritten and there is no failure mode to report.

`from` is the **enclosing module's** `NodeId`, which the traversal already tracks. No `module_of`
walk is needed, unlike on the HIR side.

## Diagnostics

| Case | Message |
| --- | --- |
| Unresolved final segment | `cannot find X in this scope` |
| Two declarations, one name, one namespace | `the name X is defined multiple times` |
| Import path matching more than one namespace | `ambiguous import: X refers to more than one item` |
| Declaration named after a primitive | `X shadows a built-in type` |
| Duplicate path on one node | reported by the owning check (duplicate bound / self-extend) |
| `dyn` applied to a non-trait | `dyn requires a trait` |
| `Self` outside a definition that introduces one | `Self is not available here` |

All are emitted through `DiagCtx::emit`, as today.

## HIR bridge contract

Lowering carries the table across as it walks:

```rust
for (path, res) in ast_res.entries(node_id) {
    hir_res.record(hir_id, path.clone(), translate(res));
}
```

The keys are the easy half. **`Res` also holds `NodeId`s pointing at AST nodes** —
`Local::Variable(NodeId)` names an `ast::Pat`, while typeck wants the HIR one. So lowering must
maintain

```rust
node_to_hir: HashMap<NodeId, HirId>
```

and translate every `NodeId` *inside* every `Res`, not merely the table's keys. This map is the
entire interface to the follow-up spec, and it is the piece most likely to be underestimated.

## Testing

**Unit tests** in `src/nameres/tests.rs`, one per behavior:

- Shadowing within a scope, and restoration when the scope pops.
- Block-scoped and match-arm-scoped bindings dropping at the closing brace.
- Sibling item resolution without qualification.
- Ancestor fallback, root fallback, prelude fallback, and their ordering.
- A user declaration shadowing a prelude name (legal) versus a glob import colliding (error).
- Generic parameter visible in a nested method; not visible outside its definition.
- `Self` in each of struct, enum, trait, and `extend`; error when unavailable.
- Bare trait path in type position resolving successfully; `dyn` on a non-trait erroring.
- Each diagnostic in the table above.

**Golden tests** via the `--emit-debug` dump, with one hard constraint:

> **`NodeId` values must never appear in golden output, and the dump must be ordered by source
> span, not by `NodeId`.**

`NodeId` is allocated from a global atomic counter (`src/ast/node_id.rs:14`) and is deterministic
only because parsing is currently sequential. The moment parsing goes parallel — which is the
stated reason `NodeId` is global at all — any golden file keyed on `NodeId` ordering begins to
flap. Building the dump span-ordered from the start costs nothing now and avoids a confusing
intermittent failure later.

## Open risks

1. **The `NodeId -> HirId` translation is the real migration cost.** Every `Res` payload needs
   rewriting during lowering. Underestimating this is the most likely way the follow-up stalls.
2. **`SelfTyRes.trait_` loses its dedicated home.** The information survives in the `Extend` item's
   `trait_path` entry, but every current consumer of `self_ty` must be re-pointed at it.
3. **Removing `TyKind::SelfType` touches the parser, the AST, lowering, and the visitor** before
   any of the new resolver exists. It is worth landing as its own commit ahead of the rest.
