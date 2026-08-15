use crate::ast::Mutability;
use crate::driver::source::SrcSpan;
use crate::hir::ids::HirId;
use crate::hir::path::Path;

#[derive(Debug)]
pub struct Ty {
    pub hir_id: HirId,
    pub kind: TyKind,
    pub span: SrcSpan,
}

#[derive(Debug)]
pub enum TyKind {
    /// A named type, optionally with generic arguments, such as `i32`, `String`, or `Array<T>`.
    ///
    /// `Self` lowers to this kind too, rather than to a kind of its own: it is an ordinary
    /// single-segment path as far as the AST and lowering are concerned, and what sets it apart
    /// is carried on `path.res` instead, as `Res::SelfTy` rather than an ordinary
    /// `Res::Type(Type::Def(_))` -- see `crate::hir::path::Res::SelfTy`.
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
    /// Represents `dyn Trait`, a type implementing `Trait` that is resolved dynamically,
    /// applied to the trait's own generic arguments if it declares any.
    Dyn {
        path: Path,
        args: Vec<HirId>, // -> Node::Ty
    },
    Error,
}
