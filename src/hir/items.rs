//! HIR representation of items: the top-level and nested things that own an [`crate::hir::Arena`]
//! (modules, functions, structs, enums, traits, `extend` blocks, closures) and the smaller
//! declarations nested inside them (imports, fields, variants, params, generics).

#![allow(dead_code)]

use crate::ast::{Ident, Path as AstPath, SelfMode, Visibility};
use crate::driver::source::SrcSpan;
use crate::hir::ids::{DefId, HirId};
use crate::hir::path::Path;

/// A module. `items` holds every definition declared directly inside it, each already lowered
/// and given its own [`DefId`]; `imports` holds the `import` statements, which stay local to
/// this module's arena since an import is not itself a definition.
///
/// `path` stays an `ast::Path`, not an `hir::Path`: a module names no resolvable target in the
/// sense `Res` answers for -- it doesn't refer to something else, it *is* the thing, addressed
/// here by its own `hir_id`. There is no answer to attach.
#[derive(Debug)]
pub struct Module {
    pub hir_id: HirId,
    pub path: AstPath,
    pub items: Vec<DefId>,
    pub imports: Vec<HirId>, // -> Node::Import
    pub span: SrcSpan,
}

/// `path` stays an `ast::Path`, for the same reason as [`Module::path`]: an import statement
/// names the thing it brings into scope, but nothing here needs to answer "what did this path
/// resolve to" the way a use of it elsewhere in the program does. Its effect is on the importing
/// module's namespace, not on some downstream consumer reading `res` back out.
#[derive(Debug)]
pub struct Import {
    pub hir_id: HirId,
    pub path: AstPath,
    pub glob: bool,
    pub alias: Option<Ident>,
    pub span: SrcSpan,
}

/// A free function or a method. `block` is `None` for a trait method with no default
/// implementation; every other function has one.
#[derive(Debug)]
pub struct Function {
    pub hir_id: HirId,
    pub visibility: Visibility,
    pub name: Ident,
    pub generics: Vec<HirId>,      // -> Node::Generic
    pub self_param: Option<HirId>, // -> Node::SelfParam
    pub params: Vec<HirId>,        // -> Node::Param
    pub ret: Option<HirId>,        // -> Node::Ty
    pub block: Option<HirId>,      // -> Node::Block
    pub span: SrcSpan,
}

#[derive(Debug)]
pub struct SelfParam {
    pub hir_id: HirId,
    pub mode: SelfMode,
    pub span: SrcSpan,
}

#[derive(Debug)]
pub struct Struct {
    pub hir_id: HirId,
    pub visibility: Visibility,
    pub name: Ident,
    pub generics: Vec<HirId>, // -> Node::Generic
    pub fields: Vec<HirId>,   // -> Node::Field
    pub span: SrcSpan,
}

#[derive(Debug)]
pub struct Field {
    pub hir_id: HirId,
    pub name: Ident,
    pub ty: HirId, // -> Node::Ty
    pub visibility: Visibility,
    pub span: SrcSpan,
}

#[derive(Debug)]
pub struct Enum {
    pub hir_id: HirId,
    pub visibility: Visibility,
    pub name: Ident,
    pub generics: Vec<HirId>, // -> Node::Generic
    pub variants: Vec<HirId>, // -> Node::Variant
    pub span: SrcSpan,
}

#[derive(Debug)]
pub struct Variant {
    pub hir_id: HirId,
    pub name: Ident,
    pub payload: VariantPayload,
    pub span: SrcSpan,
}

/// The payload an enum variant is declared with. Compare [`crate::hir::Payload`], which is the
/// payload an enum variant is built or matched with at a use site.
#[derive(Debug)]
pub enum VariantPayload {
    Unit,
    Type(HirId),        // -> Node::Ty
    Record(Vec<HirId>), // -> Node::Field
}

/// A trait definition. `functions` holds the [`DefId`] of each method the trait declares,
/// whether or not it has a default implementation.
#[derive(Debug)]
pub struct Trait {
    pub hir_id: HirId,
    pub visibility: Visibility,
    pub name: Ident,
    pub generics: Vec<HirId>, // -> Node::Generic
    pub functions: Vec<DefId>,
    pub span: SrcSpan,
}

/// An `extend` block, either an inherent `extend Foo { ... }` or a trait implementation
/// `extend Foo: Bar { ... }` (`trait_path` is `None` for the inherent form).
///
/// The three angle-bracket groups are kept separate because they mean different things.
/// `extend_generics` *declares* the type parameters the block introduces, so it holds
/// `Node::Generic`s exactly as a `struct` or `fun` does. `adt_generics` and `trait_generics`
/// *apply* arguments -- to the type being extended and to the trait being implemented -- so they
/// hold types, which may be those parameters or anything else.
#[derive(Debug)]
pub struct Extend {
    pub hir_id: HirId,
    pub extend_generics: Vec<HirId>, // -> Node::Generic
    pub adt_generics: Vec<HirId>,    // -> Node::Ty
    pub trait_generics: Vec<HirId>,  // -> Node::Ty
    /// `adt_path` and `trait_path` are the reason `hir::Path` carries its own `res` at all:
    /// `Extend` has no `HirId` of its own to key a second per-node table entry on, so a table
    /// re-keyed by `HirId` alone would have nowhere to put the second path. Giving each `Path`
    /// its own `res` field is what gives both fields somewhere to live on the same node.
    pub adt_path: Path,
    pub trait_path: Option<Path>,
    pub methods: Vec<DefId>,
    pub span: SrcSpan,
}

/// A closure's own owner. Lowering gives every closure literal a [`DefId`] and its own arena,
/// just like a function, so that its block can be lowered and later type-checked independently
/// of the function it's declared in.
#[derive(Debug)]
pub struct Closure {
    pub hir_id: HirId,
    pub params: Vec<HirId>, // -> Node::ClosureParam
    pub ret: Option<HirId>, // -> Node::Ty
    pub block: HirId,       // -> Node::Block
    pub span: SrcSpan,
}

#[derive(Debug)]
pub struct Generic {
    pub hir_id: HirId,
    pub name: Ident,
    /// A bound is a bare path on the parameter rather than a node of its own, so each one's
    /// resolution lives on the `Path` itself, in source order, rather than in a table keyed by
    /// some id the bound doesn't have.
    pub bounds: Vec<Path>,
    pub span: SrcSpan,
}

#[derive(Debug)]
pub struct Param {
    pub hir_id: HirId,
    pub name: Ident,
    pub ty: HirId, // -> Node::Ty
    pub span: SrcSpan,
}

/// A closure parameter. Unlike a function's [`Param`], the type annotation is optional; an
/// unannotated parameter has its type inferred from context.
#[derive(Debug)]
pub struct ClosureParam {
    pub hir_id: HirId,
    pub name: Ident,
    pub ty: Option<HirId>, // -> Node::Ty
    pub span: SrcSpan,
}
