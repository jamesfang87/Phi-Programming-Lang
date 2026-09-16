#![allow(dead_code)]

use crate::ast::{Ident, Path as AstPath, SelfMode, Visibility};
use crate::driver::source::SrcSpan;
use crate::hir::ids::{BlockId, DefId, HirId, TyId};
use crate::hir::path::Path;

// TODO: no `Const`/`Static`/`TypeAlias`/inline-`Module` owners exist here either (mirrors the
// `ast::ItemKind` gap), so HIR has nowhere to hold global constants or type aliases even if
// the parser learned them.

#[derive(Debug)]
pub struct Module {
    pub hir_id: HirId,
    pub path: AstPath,
    pub items: Vec<DefId>,
    pub imports: Vec<HirId>, // -> Node::Import
    pub span: SrcSpan,
}

#[derive(Debug)]
pub struct Import {
    pub hir_id: HirId,
    pub path: AstPath,
    pub glob: bool,
    pub alias: Option<Ident>,
    pub span: SrcSpan,
}

#[derive(Debug)]
pub struct Function {
    pub hir_id: HirId,
    pub visibility: Visibility,
    pub name: Ident,
    pub generics: Vec<HirId>,      // -> Node::Generic
    pub self_param: Option<HirId>, // -> Node::SelfParam
    pub params: Vec<HirId>,        // -> Node::Param
    pub ret: Option<TyId>,
    pub block: Option<BlockId>,
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
    pub ty: TyId,
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

#[derive(Debug)]
pub enum VariantPayload {
    Unit,
    Type(TyId),
    Record(Vec<HirId>), // -> Node::Field
}

#[derive(Debug)]
pub struct Trait {
    pub hir_id: HirId,
    pub visibility: Visibility,
    pub name: Ident,
    pub generics: Vec<HirId>, // -> Node::Generic
    pub functions: Vec<DefId>,
    pub span: SrcSpan,
}

#[derive(Debug)]
pub struct Extend {
    pub hir_id: HirId,
    pub extend_generics: Vec<HirId>, // -> Node::Generic
    pub self_ty: TyId,
    pub trait_generics: Vec<TyId>,
    pub trait_path: Option<Path>,
    pub methods: Vec<DefId>,
    pub span: SrcSpan,
}

#[derive(Debug)]
pub struct Closure {
    pub hir_id: HirId,
    pub params: Vec<HirId>, // -> Node::ClosureParam
    pub ret: Option<TyId>,
    pub block: BlockId,
    pub span: SrcSpan,
}

#[derive(Debug)]
pub struct Generic {
    pub hir_id: HirId,
    pub name: Ident,
    pub bounds: Vec<Bound>,
    pub span: SrcSpan,
}

/// A trait bound on a generic parameter, such as `T: Conv<i32>`
#[derive(Debug)]
pub struct Bound {
    pub path: Path,
    pub args: Vec<TyId>,
    pub span: SrcSpan,
}

#[derive(Debug)]
pub struct Param {
    pub hir_id: HirId,
    pub name: Ident,
    pub ty: TyId,
    pub span: SrcSpan,
}

#[derive(Debug)]
pub struct ClosureParam {
    pub hir_id: HirId,
    pub name: Ident,
    pub ty: Option<TyId>,
    pub span: SrcSpan,
}
