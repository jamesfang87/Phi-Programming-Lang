use crate::{
    ast::Mutability,
    hir::{DefId, LocalId},
    lexer::src_span::SrcSpan,
};

#[derive(Debug)]
pub struct Ty {
    pub kind: TyKind,
    pub span: SrcSpan,
}

#[derive(Debug)]
pub enum TyKind {
    Var,
    IntVar,
    FloatVar,
    Primitive,
    Adt {
        def: DefId,
        args: Vec<TyKind>,
    },
    Generic {
        def: DefId,
    },
    Ref {
        base: Box<TyKind>,
        mutability: Mutability,
    },
    Any(Box<TyKind>),
    Tuple(Vec<TyKind>),
    Array {
        elem: Box<TyKind>,
        len: Option<LocalId>, // -> Node::Expr, the constant expression `N`
    },
    Fun {
        params: Vec<TyKind>,
        ret: Option<Box<TyKind>>,
    },
    SelfType,
    Dyn(DefId),
    Error,
}
