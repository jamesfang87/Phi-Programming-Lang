//! Trait solving: the machinery behind the one question traits and `extend` blocks exist to
//! raise -- *does this type implement this trait?*
//!
//! The question is harder than a table lookup because a type is not just a name. `Foo<i32>`,
//! `Foo<T>` and `Foo<T, i32>` are different types that may be implemented separately, and an
//! `extend` block's own generics make its header a *pattern* rather than a fact. So the query is
//! a matching problem -- and matching means two `extend` blocks can both apply to one type, which
//! has to be an error, or the answer isn't a function at all.
//!
//! Six modules, one job each:
//!
//! - [`index`] collects every `extend` block into an [`ImplIndex`](index::ImplIndex), keyed for
//!   lookup, and rejects the ones whose self type is not a struct or enum.
//! - [`overlap`] answers whether two impl headers can both apply to one type. It knows nothing
//!   about diagnostics.
//! - [`coherence`] is the whole-program conflict check, built on [`overlap`].
//! - [`members`] checks each impl against the trait it implements: the right methods, at the
//!   right signatures.
//! - [`solve`] is the query itself, plus the [`ParamEnv`](solve::ParamEnv) of assumptions it is
//!   asked against.
//! - [`bounds`] asks the query where a bound is instantiated, deferring each goal into an
//!   [`ObligationCx`](bounds::ObligationCx) until inference has settled enough to answer it.
//! - [`method`] is where the query finally reaches an expression: `x.foo()` finds the one method
//!   it means, out of the inherent blocks, the impls, the bounds in scope and the `dyn` receivers
//!   that could each offer one.
//!
//! Everything here is phrased in interned [`Ty`](crate::typeck::ty::Ty). The solver never
//! resolves a name: by the time it runs, every `extend` header has been lowered by
//! [`collect_module`](crate::typeck::Typeck::collect_module), which is why this stage sits
//! between collection and body checking and cannot move.
//!
//! ## On the `dead_code` allowance
//!
//! Every piece of this design is now written and reached from a body, save for a handful of
//! `len`/`is_empty`/`empty` accessors and the [`TraitName::Def`](solve::TraitName::Def) variant
//! that the tests beside each module use and nothing else does yet. One allowance on the module
//! reads better than several scattered over the pieces, the way [`hir::items`](crate::hir)
//! already does it.
//!
//! [`solve::Solution::Holds`] once carried an answer of its own -- an `ImplSource` recording
//! *why* a goal held, meant for method resolution to instantiate the method it found with. It
//! never ended up reading one: a call site collects its own candidates, because a bound and a
//! `dyn` offer methods the query has no impl to point at, and it substitutes through the
//! candidate it picked directly rather than through anything the solver hands back (see
//! [`method`]). Nothing else read it either -- every real caller already collapsed `Holds(_)` to
//! `()` -- so it carried real weight (an [`ImplId`](index::ImplId) and a substitution, copied out
//! of the index on every successful query) for no reader, and is gone.

#![allow(dead_code)]

use crate::hir::DefId;
use crate::typeck::ty::Ty;

pub mod bounds;
pub mod coherence;
pub mod index;
pub mod members;
pub mod method;
pub mod overlap;
pub mod solve;

/// A trait applied to its arguments: `Show`, or `Index<K, V>`.
///
/// `Self` is deliberately not in `args`. Every place a `TraitRef` is used already carries the
/// self type separately -- an [`Obligation`](solve::Obligation) asks about one, an
/// [`ImplHeader`](index::ImplHeader) declares one -- and folding it in would make the two
/// halves of every question drift apart, since only `args` gets substituted through.
///
/// Lives in the module root rather than in one of the four files below because all four speak
/// it: the index stores one, overlap unifies two, coherence groups by one, and the solver
/// matches against one.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct TraitRef {
    /// The trait being named. Always an [`OwnerNode::Trait`](crate::hir::OwnerNode::Trait);
    /// [`index`] is what rejects anything else before one of these is built.
    pub def: DefId,

    /// The trait's own generic arguments. Empty for a bound written `T: Show`, which is the only
    /// form the surface syntax can express today -- a bound is a bare path with no argument list.
    pub args: Vec<Ty>,
}
