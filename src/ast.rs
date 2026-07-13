//! # Phi surface AST

#![allow(dead_code)]

mod expr_impls;
mod type_impls;

use crate::lexer::src_span::SrcSpan;

// ===========================================================================
// Identifiers, paths, literals
// ===========================================================================

#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub struct Symbol(u32);

impl Symbol {
    pub(crate) fn from_id(id: u32) -> Symbol {
        Symbol(id)
    }

    pub(crate) fn id(self) -> u32 {
        self.0
    }
}

#[derive(Clone, Copy, Debug)]
pub enum Visibility {
    Public,
    Private,
}

#[derive(Clone, Copy, Debug)]
pub enum Mutability {
    Immutable,
    Mutable,
}

/// An identifier.
#[derive(Clone, Copy, Debug)]
pub struct Ident {
    pub text: Symbol,
    pub span: SrcSpan,
}

/// A (possibly qualified) name.
#[derive(Clone, Debug)]
pub struct Path {
    pub segments: Vec<Ident>,
    pub span: SrcSpan,
}

/// A literal. Leaf data only — the span lives on whatever wraps it (`Expr`, `Pattern`).
#[derive(Clone, Copy, Debug)]
pub enum Literal {
    Int { value: Symbol, suffix: Symbol },
    Float { value: Symbol, suffix: Symbol },
    Str(Symbol),
    Bool(bool),
    Char(char),
}

// ===========================================================================
// Items
// ===========================================================================

#[derive(Clone, Debug)]
pub struct SrcUnit {
    pub module: Option<ModuleDecl>, // `module math::vector;`
    pub imports: Vec<Import>,
    pub items: Vec<Item>,
    pub span: SrcSpan,
}

#[derive(Clone, Debug)]
pub struct ModuleDecl {
    pub path: Path,
    pub span: SrcSpan,
}

#[derive(Clone, Debug)]
pub struct Import {
    pub path: Path,
    /// `true` for a glob import, `import math::*;`.
    pub glob: bool,
    pub span: SrcSpan,
}

#[derive(Clone, Debug)]
pub struct Item {
    pub kind: ItemKind,
    pub span: SrcSpan,
}

#[derive(Clone, Debug)]
pub enum ItemKind {
    Function(Function),
    Struct(Struct),
    Enum(Enum),
    Trait(Trait),
    // NOTE: added here — `Extend` was defined below but never reachable from `ItemKind` in the
    // original, even though it's clearly a top-level declaration (`extend Foo: Bar { ... }`).
    // Remove this if that omission was intentional rather than an oversight.
    Extend(Extend),
    Error,
}

#[derive(Clone, Debug)]
pub struct Function {
    pub visibility: Visibility,
    pub name: Ident,
    pub generics: Vec<Generic>,
    pub self_param: Option<SelfParam>,
    pub params: Vec<Param>,
    pub ret: Option<Type>,
    pub body: Option<Block>,
    pub span: SrcSpan,
}

#[derive(Clone, Debug)]
pub struct Struct {
    pub visibility: Visibility,
    pub name: Ident,
    pub generics: Vec<Generic>,
    pub fields: Vec<Field>,
    pub span: SrcSpan,
}

#[derive(Clone, Debug)]
pub struct Enum {
    pub visibility: Visibility,
    pub name: Ident,
    pub generics: Vec<Generic>,
    pub fields: Vec<Variant>,
    pub span: SrcSpan,
}

#[derive(Clone, Debug)]
pub struct Trait {
    pub visibility: Visibility,
    pub name: Ident,
    pub generics: Vec<Generic>,
    pub functions: Vec<Function>,
    pub span: SrcSpan,
}

#[derive(Clone, Debug)]
pub struct Extend {
    pub generics: Vec<Generic>,
    pub ty: Type,
    pub trait_path: Option<Path>,
    pub methods: Vec<Function>,
    pub span: SrcSpan,
}

// ===========================================================================
// Locals
// ===========================================================================

#[derive(Clone, Debug)]
pub struct SelfParam {
    pub mode: SelfMode,
    pub span: SrcSpan,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum SelfMode {
    Immutable,
    Mutable,
    Move,
    Any,
}

#[derive(Clone, Debug)]
pub struct Generic {
    pub name: Ident,
    pub bounds: Vec<Trait>,
    pub span: SrcSpan,
}

#[derive(Clone, Debug)]
pub struct Param {
    pub name: Ident,
    pub ty: Type,
    pub span: SrcSpan,
}

#[derive(Clone, Debug)]
pub struct Field {
    pub name: Ident,
    pub ty: Type,
    pub visibility: Visibility,
    pub span: SrcSpan,
}

#[derive(Clone, Debug)]
pub struct Variant {
    pub name: Ident,
    pub payload: VariantPayload,
    pub span: SrcSpan,
}

#[derive(Clone, Debug)]
pub enum VariantPayload {
    Unit,
    Type,
    Record(Vec<Field>),
}

// ===========================================================================
// Type
// ===========================================================================

/// Already in the wrapper-struct shape (`kind` + `span`) — no change needed here, this is what
/// `Item`/`Stmt`/`Expr`/`Pattern` are now converging on.
#[derive(Clone, Debug)]
pub struct Type {
    pub kind: Ty,
    pub span: SrcSpan,
}

#[derive(Clone, Debug)]
pub enum Ty {
    Base {
        base: Path,
        args: Vec<Ty>,
    },
    Ref {
        base: Box<Ty>,
        mutability: Mutability,
    },
    Any(Box<Ty>),
    Tuple(Vec<Ty>),
    Slice {
        elem: Box<Ty>,
        len: Box<Expr>,
    },
    SelfType,
    Dyn(Path),
    Error,
}

// ===========================================================================
// Block
// ===========================================================================

#[derive(Clone, Debug)]
pub struct Block {
    pub stmts: Vec<Stmt>,
    pub span: SrcSpan,
}

// ===========================================================================
// Stmt
// ===========================================================================

#[derive(Clone, Debug)]
pub struct Stmt {
    pub kind: StmtKind,
    pub span: SrcSpan,
}

#[derive(Clone, Debug)]
pub enum StmtKind {
    While {
        cond: Expr,
        body: Block,
    },
    For {
        name: Pattern,
        iter: Expr,
        body: Block,
    },
    Continue,
    Break,
    Return(Expr),
    Defer(Expr),
    Decl(DeclStmt),
    With {
        lends: Vec<DeclStmt>,
        body: Block,
    },
    Expr(Expr),
    Error,
}

/// Kept as its own struct (rather than inlined into `StmtKind::Decl { .. }`) since it's reused
/// verbatim inside `StmtKind::With`'s `lends: Vec<DeclStmt>`.
#[derive(Clone, Debug)]
pub struct DeclStmt {
    pub mutability: Mutability,
    pub name: Pattern,
    pub ty: Option<Type>,
    pub expr: Expr,
    pub span: SrcSpan,
}

// ===========================================================================
// Expr
// ===========================================================================

#[derive(Clone, Debug)]
pub struct Expr {
    pub kind: ExprKind,
    pub span: SrcSpan,
}

#[derive(Clone, Debug)]
pub enum ExprKind {
    Literal(Literal),
    DeclRef(Path),
    Unary {
        op: UnaryOp,
        operand: Box<Expr>,
    },
    Binary {
        op: BinaryOp,
        lhs: Box<Expr>,
        rhs: Box<Expr>,
    },
    Borrow {
        mutability: Mutability,
        operand: Box<Expr>,
    },
    FunCall {
        callee: Box<Expr>,
        args: Vec<Expr>,
    },
    MethodCall {
        receiver: Box<Expr>,
        method: Ident,
        args: Vec<Expr>,
    },
    Field {
        base: Box<Expr>,
        field: Ident,
    },
    Index {
        base: Box<Expr>,
        index: Box<Expr>,
    },
    Ctor {
        path: Path,
        payload: CtorPayload,
    },
    Tuple(Vec<Expr>),
    Range {
        lo: Option<Box<Expr>>,
        hi: Option<Box<Expr>>,
        inclusive: bool,
    },
    Try(Box<Expr>),
    If {
        cond: Box<Expr>,
        then_branch: Block,
        else_branch: Option<Box<Expr>>,
    },
    Match {
        scrutinee: Box<Expr>,
        arms: Vec<MatchArm>,
    },
    Spawn(Block),
    Concurrent(Block),
    Block(Block),
    Error,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum UnaryOp {
    Neg, // -
    Not, // !
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum BinaryOp {
    Add,
    Sub,
    Mul,
    Div,
    Rem, // + - * / %
    Eq,
    Ne,
    Lt,
    Le,
    Gt,
    Ge, // == != < <= > >=
    And,
    Or, // && ||
}

/// Kept as its own struct (rather than inlined into `ExprKind::Ctor { .. }`) since a ctor payload
/// has its own internal span independent of the surrounding `Expr`'s span — e.g. `Circle { r: 1 }`
/// vs. the payload `{ r: 1 }` alone.
#[derive(Clone, Debug)]
pub struct CtorPayload {
    pub name: Ident,
    pub expr: Box<Expr>,
    pub span: SrcSpan,
}

/// Kept as its own struct for the same reason: a match arm's span (pattern + `=>` + body) is
/// distinct from the `Expr::Match` wrapper's span (which covers the whole `match { ... }`).
#[derive(Clone, Debug)]
pub struct MatchArm {
    pub pat: Pattern,
    pub body: Box<Expr>,
    pub span: SrcSpan,
}

// ===========================================================================
// Pattern
// ===========================================================================

#[derive(Clone, Debug)]
pub struct Pattern {
    pub kind: PatternKind,
    pub span: SrcSpan,
}

#[derive(Clone, Debug)]
pub enum PatternKind {
    Wildcard,
    Binding(Ident),
    Literal(Literal),
    /// PascalCase constructor pattern — enum variant or struct destructure:
    /// `Circle(r)`, `Parallelogram(b, h)`, bare `Rectangle`.
    Ctor {
        path: Path,
        payload: Vec<Ident>,
    },
    /// `(x, y)` — tuple destructuring (`let (x, y) = point;`).
    Tuple(Vec<Pattern>),
    Error,
}
