#![allow(dead_code)]

mod builder;
mod expr_impls;
pub mod interner;
mod node_id;
mod type_impls;
pub mod visit;

use std::collections::HashMap;

use builder::AstBuilder;
pub use interner::Symbol;
pub use node_id::NodeId;

use crate::driver::source::SrcSpan;

#[derive(Debug)]
pub struct Ast {
    modules: Vec<Module>,
    /// Where each module in `modules` sits, keyed by its `NodeId`.
    ///
    /// A module's `NodeId` is allocated from the same global counter as every other AST node,
    /// so it isn't a dense index into `modules` the way the old `ModId` was. This is what makes
    /// looking a module back up by id an `O(1)` map lookup instead of direct indexing.
    positions: HashMap<NodeId, usize>,
    /// `parents[i]` is the parent of `modules[i]`, positionally aligned the same way.
    parents: Vec<NodeId>,
    root: NodeId,
}

#[derive(Debug)]
pub struct Module {
    pub id: NodeId,
    pub path: Path,
    pub imports: Vec<Import>,
    pub items: Vec<Item>,
    pub children: Vec<NodeId>,
}

// ===========================================================================
// Utils
// ===========================================================================

#[derive(Clone, Copy, Debug)]
pub enum Visibility {
    Public,
    Private,
}

#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum Mutability {
    Immutable,
    Mutable,
}

/// A single identifier token, such as a variable, function, or type name.
#[derive(Clone, Copy, Debug)]
pub struct Ident {
    pub text: Symbol,
    pub span: SrcSpan,
}

/// A name that may be qualified with `::`, such as `math::Vector2D`.
#[derive(Clone, Debug)]
pub struct Path {
    pub segments: Vec<Ident>,
    pub span: SrcSpan,
}

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

// ===========================================================================
// Items
// ===========================================================================

/// The parsed contents of one source file.
#[derive(Clone, Debug)]
pub struct ParsedSrcFile {
    /// The file's `module math::vector;` declaration, if it has one.
    pub module: Option<ModuleDecl>,
    pub imports: Vec<Import>,
    pub items: Vec<Item>,
    pub span: SrcSpan,
}

#[derive(Clone, Debug)]
pub struct Item {
    pub id: NodeId,
    pub kind: ItemKind,
    pub span: SrcSpan,
}

#[derive(Clone, Debug)]
pub enum ItemKind {
    ModuleDecl(ModuleDecl),
    Import(Import),
    Function(Function),
    Struct(Struct),
    Enum(Enum),
    Trait(Trait),
    Extend(Extend),
    Error,
}

/// A module declaration, such as `module math::vector;`.
#[derive(Clone, Debug)]
pub struct ModuleDecl {
    pub id: NodeId,
    pub path: Path,
    pub span: SrcSpan,
}

#[derive(Clone, Debug)]
pub struct Import {
    pub id: NodeId,
    pub path: Path,
    /// This is `true` when the import is a glob import, such as `import math::*;`.
    pub glob: bool,
    pub alias: Option<Ident>,
    pub span: SrcSpan,
}

#[derive(Clone, Debug)]
pub struct Function {
    pub visibility: Visibility,
    pub name: Ident,
    pub generics: Vec<Generic>,
    pub self_param: Option<SelfParam>,
    pub params: Vec<Param>,
    pub ret: Option<Ty>,
    pub block: Option<Block>,
    pub span: SrcSpan,
}

#[derive(Clone, Debug)]
pub struct Struct {
    pub visibility: Visibility,
    pub name: Ident,
    pub generics: Option<Vec<Generic>>,
    pub fields: Vec<Field>,
    pub span: SrcSpan,
}

#[derive(Clone, Debug)]
pub struct Enum {
    pub visibility: Visibility,
    pub name: Ident,
    pub generics: Option<Vec<Generic>>,
    pub variants: Vec<Variant>,
    pub span: SrcSpan,
}

#[derive(Clone, Debug)]
pub struct Trait {
    pub visibility: Visibility,
    pub name: Ident,
    pub generics: Option<Vec<Generic>>,
    pub functions: Vec<Function>,
    pub span: SrcSpan,
}

/// An `extend` block, such as `extend<T> Foo<T> with Bar<T> { ... }`.
#[derive(Clone, Debug)]
pub struct Extend {
    /// The type parameters the `extend` block itself introduces, from `extend<T>`.
    pub extend_generics: Option<Vec<Generic>>,
    /// The extended type's own generic arguments, from `Foo<T>`.
    pub adt_generics: Option<Vec<Ty>>,
    /// The optional `with`-clause trait's generic arguments, from `with Bar<T>`.
    pub trait_generics: Option<Vec<Ty>>,
    pub adt_path: Path,
    pub trait_path: Option<Path>,
    pub methods: Vec<Function>,
    pub span: SrcSpan,
}

// ===========================================================================
// Locals
// ===========================================================================

#[derive(Clone, Debug)]
pub struct SelfParam {
    pub id: NodeId,
    pub mode: SelfMode,
    pub span: SrcSpan,
}

/// The way a method binds `self`.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum SelfMode {
    /// `&self` borrows `self` immutably.
    Immutable,
    /// `&mut self` borrows `self` mutably.
    Mutable,
    /// Bare `self` takes ownership of it.
    Move,
    /// `any self` accepts `self` bound in any of the other three ways.
    Any,
}

#[derive(Clone, Debug)]
pub struct Generic {
    pub id: NodeId,
    pub name: Ident,
    pub bounds: Option<Vec<Path>>,
    pub span: SrcSpan,
}

#[derive(Clone, Debug)]
pub struct Param {
    pub id: NodeId,
    pub name: Ident,
    pub ty: Ty,
    pub span: SrcSpan,
}

#[derive(Clone, Debug)]
pub struct Field {
    pub id: NodeId,
    pub visibility: Visibility,
    pub name: Ident,
    pub ty: Ty,
    pub span: SrcSpan,
}

#[derive(Clone, Debug)]
pub struct Variant {
    pub id: NodeId,
    pub name: Ident,
    pub payload: VariantPayload,
    pub span: SrcSpan,
}

#[derive(Clone, Debug)]
pub enum VariantPayload {
    /// The variant carries no payload, such as `.none`.
    Unit,
    /// The variant carries a single unnamed value, such as `.circle(f64)`.
    Type(Ty),
    /// The variant carries named fields, such as `.square { l: f64 }`.
    Record(Vec<Field>),
}

#[derive(Clone, Debug)]
pub struct ClosureParam {
    pub id: NodeId,
    pub name: Ident,
    pub ty: Option<Ty>,
    pub span: SrcSpan,
}

// ===========================================================================
// Type
// ===========================================================================

#[derive(Clone, Debug)]
pub struct Ty {
    pub id: NodeId,
    pub kind: TyKind,
    pub span: SrcSpan,
}

#[derive(Clone, Debug)]
pub enum TyKind {
    Path {
        path: Path,
        args: Vec<Ty>,
    },
    Ref {
        base: Box<Ty>,
        mutability: Mutability,
    },
    Any(Box<Ty>),
    Tuple(Vec<Ty>),
    Array {
        elem: Box<Ty>,
        len: Option<Box<Expr>>,
    },
    Function {
        params: Vec<Ty>,
        ret: Option<Box<Ty>>,
    },
    Dyn {
        path: Path,
        args: Vec<Ty>,
    },
    Error,
}

// ===========================================================================
// Stmt
// ===========================================================================

#[derive(Clone, Debug)]
pub struct Stmt {
    pub id: NodeId,
    pub kind: StmtKind,
    pub span: SrcSpan,
}

#[derive(Clone, Debug)]
pub enum StmtKind {
    While {
        cond: Expr,
        block: Block,
    },
    WhileLet {
        pat: Pat,
        scrutinee: Expr,
        block: Block,
    },
    For {
        pat: Pat,
        iter: Expr,
        block: Block,
    },
    Break,
    Continue,
    Return(Expr),
    /// `defer expr;`. The expression runs just before the enclosing scope exits.
    Defer(Expr),
    /// A `let` binding, of the form `let [mut] pat[: ty] = init;`.
    Let {
        mutability: Mutability,
        pat: Pat,
        ty: Option<Ty>,
        init: Expr,
        else_block: Option<Block>,
    },
    /// A `with` block, such as `with px = &mut point.x, py = &mut point.y { ... }`.
    ///
    /// Each binding in `lends` is scoped to `block` and stops projecting its source at the
    /// closing brace, regardless of where its last use inside the block falls.
    With {
        lends: Vec<WithLend>,
        block: Block,
    },
    Expr {
        expr: Expr,
        semi: bool,
    },
    Error,
}

/// One binding in a `with` block, such as `px = &mut point.x`.
#[derive(Clone, Debug)]
pub struct WithLend {
    pub id: NodeId,
    pub pat: Pat,
    pub ty: Option<Ty>,
    pub init: Expr,
    pub span: SrcSpan,
}

// ===========================================================================
// Expr
// ===========================================================================

#[derive(Clone, Debug)]
pub struct Expr {
    pub id: NodeId,
    pub kind: ExprKind,
    pub span: SrcSpan,
}

#[derive(Clone, Debug)]
pub enum ExprKind {
    Literal(Literal),
    Path(Path),
    Unary {
        op: UnaryOp,
        operand: Box<Expr>,
    },
    Binary {
        op: BinaryOp,
        lhs: Box<Expr>,
        rhs: Box<Expr>,
    },
    /// `lhs = rhs`.
    Assign {
        lhs: Box<Expr>,
        rhs: Box<Expr>,
    },
    /// A compound assignment, such as `lhs += rhs` or `lhs -= rhs`.
    ///
    /// `op` names the underlying binary operator, `+` or `-` in those examples.
    AssignOp {
        op: BinaryOp,
        lhs: Box<Expr>,
        rhs: Box<Expr>,
    },
    Borrow {
        mutability: Mutability,
        operand: Box<Expr>,
    },
    Call {
        callee: Box<Expr>,
        args: Vec<Expr>,
    },
    /// A `.` access, such as `base.member` or `base.member(args)`.
    ///
    /// `args` records how it was written; see [`AccessArgs`] for why that's needed.
    Access {
        base: Box<Expr>,
        member: Ident,
        args: AccessArgs,
    },
    Index {
        base: Box<Expr>,
        index: Box<Expr>,
    },
    /// A struct literal, such as `Foo { a: 1 }`.
    ///
    /// `path` is `None` for the elided form, `.{ a: 1 }`, which takes its type from context.
    Ctor {
        path: Option<Path>,
        payload: Vec<PayloadField<Expr>>,
    },
    /// An enum variant construction, such as `.circle(1.24)` or `Shape.circle(1.24)`.
    Variant {
        variant: Ident,
        payload: Payload<Expr>,
    },
    Tuple(Vec<Expr>),
    /// `lo..hi` or, when `inclusive` is set, `lo..=hi`. Either bound may be omitted.
    Range {
        lo: Option<Box<Expr>>,
        hi: Option<Box<Expr>>,
        inclusive: bool,
    },
    /// The `?` operator: propagates an error result out of the enclosing function.
    Try(Box<Expr>),
    If {
        cond: Box<Expr>,
        then_block: Block,
        else_expr: Option<Box<Expr>>,
    },
    IfLet {
        pat: Pat,
        scrutinee: Box<Expr>,
        then_block: Block,
        else_expr: Option<Box<Expr>>,
    },
    Match {
        scrutinee: Box<Expr>,
        arms: Vec<Arm>,
    },
    /// `spawn { ... }`. Launches the block as a concurrent task and evaluates to a handle for it.
    Spawn(Block),
    /// `concurrent { ... }`. Runs the block, then waits for every task it `spawn`ed before
    /// producing a value.
    Concurrent(Block),
    Block(Block),
    Closure {
        params: Vec<ClosureParam>,
        ret: Option<Ty>,
        body: Box<Expr>,
    },
    /// `expr as Ty`, e.g. `x as i64`. Only ever a conversion between two primitive types; see
    /// [`crate::typeck::cast`] for exactly which pairs are allowed and why.
    Cast {
        expr: Box<Expr>,
        ty: Ty,
    },
    Error,
}

#[derive(Clone, Copy, Debug)]
pub enum Literal {
    /// `suffix` is the type named after the `_` in `42_i64`, if the literal was written with
    /// one.
    Int {
        value: Symbol,
        suffix: Option<Symbol>,
    },
    /// `suffix` is the type named after the `_` in `3.14_f32`, if the literal was written with
    /// one.
    Float {
        value: Symbol,
        suffix: Option<Symbol>,
    },
    Str(Symbol),
    Bool(bool),
    Char(char),
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum UnaryOp {
    /// Numeric negation, `-x`.
    Neg,
    /// Logical negation, `!x`.
    Not,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum BinaryOp {
    Add,
    Sub,
    Mul,
    Div,
    Rem,
    Eq,
    Ne,
    Lt,
    Le,
    Gt,
    Ge,
    And,
    Or,
}

#[derive(Clone, Debug)]
pub struct Block {
    pub id: NodeId,
    pub stmts: Vec<Stmt>,
    pub span: SrcSpan,
}

impl ExprKind {
    /// Reports whether this expression already ends in a `}`, such as `if`, `match`, or a
    /// bare block.
    ///
    /// The parser uses this to decide whether an expression statement needs a trailing `;`:
    /// block-bodied expressions don't.
    pub fn is_block_bodied(&self) -> bool {
        matches!(
            self,
            ExprKind::If { .. }
                | ExprKind::IfLet { .. }
                | ExprKind::Match { .. }
                | ExprKind::Spawn(_)
                | ExprKind::Concurrent(_)
                | ExprKind::Block(_)
        )
    }
}

/// The payload an enum variant carries, shared between constructing a variant
/// ([`ExprKind::Variant`]) and matching one ([`PatternKind::Variant`]).
#[derive(Clone, Debug)]
pub enum Payload<T> {
    /// The variant has no payload at all, such as bare `.none`.
    None,
    /// The variant has one unnamed value, such as `.circle(1.24)`.
    Single(Box<T>),
    /// The variant has named fields, declared inline as `{ l: f64 }` and written as
    /// `.square { l: 4.0 }`.
    Record(Vec<PayloadField<T>>),
}

/// `AccessArgs` describes how an access was written.
///
/// As the grammar is ambiguous in this case, the parser can't yet tell a field access, a
/// method call, and a payload-carrying variant construction apart.
///
/// Later analysis resolves this distinction, once `base`'s type is known.
#[derive(Clone, Debug)]
pub enum AccessArgs {
    /// `base.member`. This could be a field, a payload-less variant, or a method referenced as
    /// a value.
    None,
    /// `base.member(a, b)`. This could be a method call, or a variant whose single payload is
    /// `a`.
    Call(Vec<Expr>),
    /// `base.member { f: v }`. This can only be a variant with a record payload.
    Record(Vec<PayloadField<Expr>>),
}

/// This can represent either a field initalizer in
/// ([`ExprKind::Ctor`]), or one field of a variant's record payload
#[derive(Clone, Debug)]
pub struct PayloadField<T> {
    pub id: NodeId,
    pub name: Ident,
    pub value: Option<T>,
    pub span: SrcSpan,
}

#[derive(Clone, Debug)]
pub struct Arm {
    pub id: NodeId,
    pub pat: Pat,
    /// The `if cond` in `pat if cond => body`, if the arm has one. When present, the arm only
    /// matches if `cond` also holds, exactly as in Rust.
    pub guard: Option<Box<Expr>>,
    pub body: Box<Expr>,
    pub span: SrcSpan,
}

// ===========================================================================
// Pattern
// ===========================================================================

#[derive(Clone, Debug)]
pub struct Pat {
    pub id: NodeId,
    pub kind: PatKind,
    pub span: SrcSpan,
}

#[derive(Clone, Debug)]
pub enum PatKind {
    Wildcard,
    Binding(Ident),
    Literal(Literal),
    /// An enum variant pattern, such as `.circle(r)`, `.square { l }`, or `.none`.
    Variant {
        variant: Ident,
        payload: Payload<Pat>,
    },
    /// A tuple pattern, such as the `(x, y)` in `let (x, y) = point;`.
    Tuple(Vec<Pat>),
    Error,
}

impl Ast {
    /// Collects every parsed file of a build into one module tree.
    pub fn new(files: Vec<ParsedSrcFile>) -> Ast {
        let mut builder = AstBuilder::new();
        for file in files {
            let module = match &file.module {
                Some(decl) => builder.module_for_path(&decl.path.segments),
                None => builder.ast.root,
            };
            let position = builder.ast.positions[&module];
            let target = &mut builder.ast.modules[position];
            target.imports.extend(file.imports);
            // `Parser::assemble_file` has already sorted this file's `module` header and its
            // imports out of `items`, so what is left is definitions.
            target.items.extend(file.items);
        }
        builder.ast
    }
    pub fn root(&self) -> &Module {
        self.module(self.root)
    }

    pub fn root_id(&self) -> NodeId {
        self.root
    }

    pub fn module(&self, id: NodeId) -> &Module {
        &self.modules[self.positions[&id]]
    }

    /// Returns the module `id` is declared inside, or `None` if `id` names the root.
    ///
    /// The root is stored as its own parent, which is how callers get a termination condition
    /// walking upwards, without needing a sentinel id.
    pub fn parent(&self, id: NodeId) -> Option<NodeId> {
        let parent = self.parents[self.positions[&id]];
        (parent != id).then_some(parent)
    }

    /// Iterates every module, parents before children.
    pub fn mod_ids(&self) -> impl Iterator<Item = NodeId> + '_ {
        self.modules.iter().map(|module| module.id)
    }
}

#[cfg(test)]
mod path_eq_tests {
    use super::*;
    use interner::Interner;
    use std::collections::hash_map::DefaultHasher;
    use std::hash::{Hash, Hasher};

    fn path(segments: &[&str], start: usize) -> Path {
        let span = SrcSpan::new(start, start + 1);
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
