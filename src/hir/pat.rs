use crate::ast::{Ident, Literal};
use crate::driver::source::SrcSpan;
use crate::hir::expr::Payload;
use crate::hir::ids::{ArmId, BlockId, ExprId, PatId};

#[derive(Debug)]
pub struct Pat {
    pub hir_id: PatId,
    pub kind: PatKind,
    pub span: SrcSpan,
}

#[derive(Clone, Debug)]
pub enum PatKind {
    // TODO: no struct patterns (mirrors the `ast::PatKind` gap), so `let Point { x, y }`
    // never survives lowering even if the parser learned it.
    /// `_`. Matches anything and binds nothing.
    Wildcard,
    /// A plain name. Binds whatever it matches, such as `x` in `let x = ..` or `r` in
    /// `Circle(r)`.
    Binding {
        name: Ident,
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
    Tuple(Vec<PatId>),
    /// A pattern that failed to parse. Lowering carries it through rather than aborting.
    Error,
}

/// Records whether a binding takes its match by value, by immutable reference, or by mutable
/// reference. Typeck computes this per pattern while peeling references off the scrutinee and
/// records it in `PatAdjust`; MIR lowering reads it from there.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum BindingMode {
    Value,
    Ref,
    RefMut,
}

/// One arm of a `match` expression, of the form `pat => body` or `pat if guard => body`.
#[derive(Debug)]
pub struct Arm {
    pub hir_id: ArmId,
    pub pat: PatId,
    pub guard: Option<ExprId>,
    pub block: BlockId,
    pub span: SrcSpan,
}
