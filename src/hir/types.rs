//! HIR representation of type annotations, as written in source: a param's type, a field's type,
//! a return type, and so on. This is distinct from the types the type checker infers and works
//! with internally.

use crate::ast::{Mutability, Path};
use crate::hir::ids::HirId;
use crate::lexer::src_span::SrcSpan;

#[derive(Debug)]
pub struct Ty {
    pub hir_id: HirId,
    pub kind: TyKind,
    pub span: SrcSpan,
}

#[derive(Debug)]
pub enum TyKind {
    /// A named type, optionally with generic arguments, such as `i32`, `String`, or `Array<T>`.
    Path {
        path: Path,
        args: Vec<HirId>, // -> Node::Ty
    },
    /// Represents `&T` or `&mut T`.
    Ref {
        base: HirId, // -> Node::Ty
        mutability: Mutability,
    },
    /// Represents `any T`.
    Any(HirId), // -> Node::Ty
    /// Represents `(T, U, ..)`.
    Tuple(Vec<HirId>), // -> Node::Ty
    /// A fixed-size array, written `[T; N]`.
    Array {
        elem: HirId,        // -> Node::Ty
        len: Option<HirId>, // -> Node::Expr, the constant expression `N`
    },
    /// A function type, such as `fun(i32, i32) -> i32` or `fun(&str)`.
    Function {
        params: Vec<HirId>, // -> Node::Ty
        ret: Option<HirId>, // -> Node::Ty
    },
    /// Represents `Self`, which refers back to the type being defined or extended, used inside
    /// that type's own `struct`, `trait`, or `extend` body.
    SelfType,
    /// Represents `dyn Trait`, a type implementing `Trait` that is resolved dynamically.
    Dyn(Path),
    Error,
}
