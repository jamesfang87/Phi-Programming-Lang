#![allow(dead_code)]

//! The abstract syntax tree the parser builds from a token stream.
//!
//! Every node carries a [`SrcSpan`] so diagnostics and later passes can point back at the
//! source text that produced it. This tree is purely syntactic: paths aren't resolved, types
//! aren't checked, and `Error` variants stand in for anything the parser recovered from. Name
//! resolution and typechecking happen on the HIR the tree gets lowered to, in [`crate::hir`].

mod expr_impls;
mod tree;
mod type_impls;

pub mod interner;

pub use tree::{Ast, AstModule, ModId};

use crate::lexer::src_span::SrcSpan;

// ===========================================================================
// Identifiers, paths, literals
// ===========================================================================

/// An interned string.
///
/// Comparing two `Symbol`s is a cheap integer comparison.
///
/// Use [`interner::Interner::resolve`] to get the underlying text back.
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

/// A module declaration, such as `module math::vector;`.
#[derive(Clone, Debug)]
pub struct ModuleDecl {
    pub path: Path,
    pub span: SrcSpan,
}

/// An import statement, such as `import math as m;`, `import math::Vector2D;`, or
/// `import math::*;`.
#[derive(Clone, Debug)]
pub struct Import {
    pub path: Path,
    /// This is `true` when the import is a glob import, such as `import math::*;`.
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
    ///
    /// These are declarations, like a `struct`'s or a `fun`'s, and are parsed as such. The two
    /// groups below are argument lists that apply types -- possibly these ones.
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
    pub name: Ident,
    pub bounds: Option<Vec<Path>>,
    pub span: SrcSpan,
}

#[derive(Clone, Debug)]
pub struct Param {
    pub name: Ident,
    pub ty: Ty,
    pub span: SrcSpan,
}

#[derive(Clone, Debug)]
pub struct Field {
    pub visibility: Visibility,
    pub name: Ident,
    pub ty: Ty,
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
    /// The variant carries no payload, such as `.none`.
    Unit,
    /// The variant carries a single unnamed value, such as `.circle(f64)`.
    Type(Ty),
    /// The variant carries named fields, such as `.square { l: f64 }`.
    Record(Vec<Field>),
}

#[derive(Clone, Debug)]
pub struct ClosureParam {
    pub name: Ident,
    pub ty: Option<Ty>,
    pub span: SrcSpan,
}

// ===========================================================================
// Type
// ===========================================================================

#[derive(Clone, Debug)]
pub struct Ty {
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
    /// `any T`. Only meaningful as a parameter or return type.
    ///
    /// It lets the callee hand back a projection into that parameter.
    ///
    /// Monomorphization picks a concrete `&T`, `&mut T`, or owned `T` for each call site at
    /// compile time.
    Any(Box<Ty>),
    Tuple(Vec<Ty>),
    Array {
        elem: Box<Ty>,
        len: Option<Box<Expr>>,
    },
    /// A function type, such as `fun(i32, i32) -> i32`.
    ///
    /// `ret` is `None` when there's no `->`. That means the function returns no value.
    Function {
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
    Error,
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

/// One named field and the value bound to it: a field initializer in a struct literal
/// ([`ExprKind::Ctor`]), or one field of a variant's record payload, whether the payload is
/// being built or matched.
#[derive(Clone, Debug)]
pub struct PayloadField<T> {
    pub name: Ident,
    /// `value` is `None` for the field shorthand `{ l }`.
    ///
    /// In a pattern, the shorthand binds `l` to that field.
    pub value: Option<T>,
    pub span: SrcSpan,
}

#[derive(Clone, Debug)]
pub struct Arm {
    pub pat: Pat,
    pub body: Box<Expr>,
    pub span: SrcSpan,
}

// ===========================================================================
// Pattern
// ===========================================================================

#[derive(Clone, Debug)]
pub struct Pat {
    pub kind: PatKind,
    pub span: SrcSpan,
}

#[derive(Clone, Debug)]
pub enum PatKind {
    Wildcard,
    Binding(Ident),
    Literal(Literal),
    /// An enum variant pattern, such as `.circle(r)`, `.square { l }`, or bare `.none`.
    Variant {
        variant: Ident,
        payload: Payload<Pat>,
    },
    /// A tuple destructuring pattern, such as the `(x, y)` in `let (x, y) = point;`.
    Tuple(Vec<Pat>),
    Error,
}
