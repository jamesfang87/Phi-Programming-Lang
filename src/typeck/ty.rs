//! The types the type checker reasons about, as opposed to [`hir::Ty`](crate::hir::Ty), which is
//! the type *annotation* the user wrote. The two differ in what they address: a `hir::Ty` holds
//! unresolved paths and points at sibling nodes by [`HirId`](crate::hir::HirId), while a
//! [`Ty`] here has every path already resolved to the definition it names, and every nested type
//! replaced by another [`Ty`].
//!
//! A [`Ty`] is a handle into the [`TyCtx`](crate::typeck::tyctx::TyCtx) that produced it, not a
//! tree. Structurally equal types intern to the same handle, so `==` on two `Ty`s answers "are
//! these the same type?" in one integer comparison, and a `Ty` is `Copy` no matter how large the
//! type it stands for. The flip side is that a handle only means something paired with its own
//! `TyCtx`: [`TyCtx::kind`](crate::typeck::tyctx::TyCtx::kind) is the only way to look one back
//! up, and handles must never be mixed between two contexts.

use crate::ast::Mutability;
use crate::hir::{DefId, HirId};
use crate::nameres::results::PrimTy;

/// A handle to a type interned in a [`TyCtx`](crate::typeck::tyctx::TyCtx).
///
/// See the [module docs](self) for why types are addressed this way instead of being passed
/// around as trees.
#[derive(Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Debug)]
pub struct Ty(u32);

impl Ty {
    pub(crate) fn from_usize(index: usize) -> Self {
        Ty(index as u32)
    }

    pub fn index(self) -> usize {
        self.0 as usize
    }
}

/// An inference variable: a type the checker has not pinned down yet.
///
/// The three kinds differ in what they are allowed to unify with. [`TyVar::Any`] accepts any
/// type at all; [`TyVar::Int`] and [`TyVar::Float`] are the fallback-carrying variables an
/// unsuffixed literal such as `1` or `1.0` starts out as, and only unify with an integer or
/// float type respectively. Ids come from a single counter in the [`TyCtx`], so no two variables
/// share one, whatever their kind.
///
/// [`TyCtx`]: crate::typeck::tyctx::TyCtx
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum TyVar {
    Any(u32),
    Int(u32),
    Float(u32),
}

impl TyVar {
    pub fn id(self) -> u32 {
        match self {
            TyVar::Any(id) | TyVar::Int(id) | TyVar::Float(id) => id,
        }
    }

    pub fn index(self) -> usize {
        self.id() as usize
    }
}

/// What a [`Ty`] actually is. Every nested position holds a [`Ty`] rather than a `Box<TyKind>`,
/// so a `TyKind` is shallow: it names its components, it doesn't contain them.
#[derive(Clone, PartialEq, Eq, Hash, Debug)]
pub enum TyKind {
    /// A type not yet known, standing in until inference resolves it.
    Var(TyVar),
    /// A built-in type such as `i32` or `bool`.
    Primitive(PrimTy),
    /// A user-declared `struct` / `enum`/ trait, applied to its generic arguments. `args` always has
    /// exactly as many entries as `def` declares type parameters -- a mismatch is reported and
    /// becomes [`TyKind::Error`] instead, so later passes can substitute positionally without
    /// re-checking.
    Adt { def: DefId, args: Vec<Ty> },
    /// A generic type parameter, such as `T` inside `fun identity<T>(x: T) -> T`.
    ///
    /// It is addressed by the [`HirId`] of the node that declares it -- a
    /// [`Node::Generic`](crate::hir::Node::Generic), or, for an `extend` block's own `<T>` list,
    /// the [`Node::Ty`](crate::hir::Node::Ty) standing in for one (see
    /// [`resolve_extend_generics`](crate::nameres::NameResolver)). That is exactly what name
    /// resolution
    /// hands back in [`TypeRes::Generic`](crate::nameres::results::TypeRes::Generic), so no
    /// separate numbering has to be built or kept in sync to name a parameter here.
    Generic(HirId),
    /// The implicit `Self` parameter a trait declares, naming the trait it belongs to.
    ///
    /// This only ever appears inside a trait's own body, where `Self` stands for whichever type
    /// eventually implements the trait and so has no structure to speak of. Inside an `extend`
    /// block `Self` is concrete, so it lowers to the extended [`TyKind::Adt`] directly and never
    /// reaches this variant.
    SelfTy(DefId),
    /// `&T` or `&mut T`.
    Ref { base: Ty, mutability: Mutability },
    /// `any T`.
    Any(Ty),
    /// `()`, the type of an expression evaluated only for its side effects, such as a function
    /// with no declared return type or a block with no trailing expression. Distinct from
    /// [`TyKind::Tuple`] with no elements even though both are written `()`, so that "this
    /// produces nothing" is one type instead of an incidental zero-length tuple.
    Unit,
    /// `(T, U, ..)`.
    Tuple(Vec<Ty>),
    /// A fixed-size array, `[T; N]`.
    ///
    /// `len` addresses the constant expression `N` rather than its value, because constant
    /// evaluation does not exist yet. Two array types whose lengths are written as two separate
    /// expressions therefore intern as two distinct types even when both evaluate to the same
    /// number; that resolves itself once `len` can hold an evaluated constant instead.
    Array {
        elem: Ty,
        len: Option<HirId>, // -> Node::Expr, the constant expression `N`
    },
    /// A function type, such as `fun(i32, i32) -> i32`.
    Fun { params: Vec<Ty>, ret: Option<Ty> },
    /// `dyn Trait`: some type implementing `Trait`, known only at run time.
    Dyn { trait_: DefId, args: Vec<Ty> },
    /// The type of an expression that never produces a value, such as a `return`. It coerces to
    /// every other type, since there is no value for the coercion to be wrong about.
    Never,
    /// A type that could not be determined because of an error already reported. It unifies with
    /// anything, so one mistake does not cascade into a second diagnostic.
    Error,
}
