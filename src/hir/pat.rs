use crate::ast::{Ident, Literal};
use crate::driver::source::SrcSpan;
use crate::hir::expr::Payload;
use crate::hir::ids::HirId;

#[derive(Debug)]
pub struct Pat {
    pub hir_id: HirId,
    pub kind: PatKind,
    pub span: SrcSpan,
}

#[derive(Clone, Debug)]
pub enum PatKind {
    /// `_`. Matches anything and binds nothing.
    Wildcard,
    /// A plain name. Binds whatever it matches, such as `x` in `let x = ..` or `r` in
    /// `Circle(r)`.
    Binding {
        name: Ident,
        mode: BindingMode,
    },
    Literal(Literal),
    /// An enum variant pattern, such as `.circle(r)`, `.square { l }`, or a bare `.none`. The
    /// scrutinee's type determines which enum the variant belongs to, so this node leaves that
    /// unresolved.
    Variant {
        variant: Ident,
        payload: Payload, // -> Node::Pat
    },
    /// `(a, b, ..)`. Destructures a tuple.
    Tuple(Vec<HirId>), // -> Node::Pat
    /// A pattern that failed to parse. Lowering carries it through rather than aborting.
    Error,
}

/// Records whether a binding pattern takes its match by value, by immutable reference, or by
/// mutable reference.
///
/// Lowering always produces `Inferred`.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum BindingMode {
    Inferred,
    Value,
    Ref,
    RefMut,
}

/// One arm of a `match` expression, of the form `pat => body` or `pat if guard => body`.
#[derive(Debug)]
pub struct Arm {
    pub hir_id: HirId,
    pub pat: HirId,           // -> Node::Pat
    pub guard: Option<HirId>, // -> Node::Expr
    pub block: HirId,         // -> Node::Block
    pub span: SrcSpan,
}
