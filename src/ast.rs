#![allow(dead_code)]

mod expr_impls;
mod type_impls;

pub mod interner;

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
pub enum ItemKind {
    Module(ModuleDecl),
    Import(Import),
    Function(Function),
    Struct(Struct),
    Enum(Enum),
    Trait(Trait),
    Extend(Extend),
    Error,
}

/// `module math::vector;`
#[derive(Clone, Debug)]
pub struct ModuleDecl {
    pub path: Path,
    pub span: SrcSpan,
}

/// import math as m;
/// import math::Vector2D;
/// import math::*;
#[derive(Clone, Debug)]
pub struct Import {
    pub path: Path,
    /// `true` for a glob import, `import math::*;`.
    pub glob: bool,
    pub alias: Option<Ident>,
    pub span: SrcSpan,
}

#[derive(Clone, Debug)]
pub struct Item {
    pub kind: ItemKind,
    pub span: SrcSpan,
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

#[derive(Clone, Debug)]
pub struct Extend {
    pub extend_generics: Option<Vec<Type>>,
    pub adt_generics: Option<Vec<Type>>,
    pub trait_generics: Option<Vec<Type>>,
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
    pub bounds: Option<Vec<Path>>,
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
    Type(Type),
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
    Array {
        elem: Box<Ty>,
        len: Option<Box<Expr>>,
    },
    /// `fun(i32, i32) -> i32`, `fun(&str)` (no `->` means no return value).
    Fn {
        params: Vec<Ty>,
        ret: Option<Box<Ty>>,
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
    Break,
    Continue,
    Return {
        ret: Expr,
    },
    Defer {
        defer: Expr,
    },
    Decl(DeclStmt),
    With {
        lends: Vec<WithStmtLend>,
        body: Block,
    },
    Expr(Expr),
    Error,
}

#[derive(Clone, Debug)]
pub struct DeclStmt {
    pub mutability: Mutability,
    pub name: Pattern,
    pub ty: Option<Type>,
    pub expr: Expr,
    pub span: SrcSpan,
}

#[derive(Clone, Debug)]
pub struct WithStmtLend {
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
        payload: Vec<CtorPayload>,
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
    Closure {
        params: Vec<ClosureParam>,
        ret: Option<Type>,
        body: Box<Expr>,
    },
    Error,
}

/// A closure parameter. Unlike a function's [`Param`], the type annotation is optional — Rust-like
/// closures infer unannotated parameter types from context.
#[derive(Clone, Debug)]
pub struct ClosureParam {
    pub name: Ident,
    pub ty: Option<Type>,
    pub span: SrcSpan,
}

#[derive(Clone, Copy, Debug)]
pub enum Literal {
    Int { value: Symbol, suffix: Symbol },
    Float { value: Symbol, suffix: Symbol },
    Str(Symbol),
    Bool(bool),
    Char(char),
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

#[derive(Clone, Debug)]
pub struct CtorPayload {
    pub name: Ident,
    pub expr: Box<Expr>,
    pub span: SrcSpan,
}

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
