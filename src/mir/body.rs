//! This module defines [`Body`], the MIR of one definition, and the tables it owns:
//! [`LocalDecl`], one per [`Local`](crate::mir::Local), and [`BasicBlockData`], one per
//! [`BasicBlock`](crate::mir::BasicBlock).

use crate::ast::{Ident, Mutability};
use crate::driver::source::SrcSpan;
use crate::hir::DefId;
use crate::mir::statement::Statement;
use crate::mir::terminator::Terminator;
use crate::typeck::ty::Ty;

/// `Body` is the MIR of one definition. Owning a `Body` is exactly what it means for a HIR
/// definition to have executable code, so a function, a method, and a closure each lower to
/// exactly one `Body`, while a struct, an enum, or a trait does not.
#[derive(Debug)]
pub struct Body {
    pub def_id: DefId,
    pub basic_blocks: Vec<BasicBlockData>,
    pub local_decls: Vec<LocalDecl>,
    /// `arg_count` is the number of `local_decls` that are parameters, `self` included. Slots
    /// `1..=arg_count` are the parameters in declared order, slot `0` is always the return place,
    /// and every slot after `arg_count` is a `let` binding or a compiler-introduced temporary.
    pub arg_count: usize,
    pub span: SrcSpan,
}

#[derive(Debug)]
pub struct LocalDecl {
    pub ty: Ty,
    pub mutability: Mutability,
    /// `name` is the source name of a user-written local, for `--emit-debug` dumps and
    /// diagnostics. It is `None` for a compiler-introduced temporary.
    pub name: Option<Ident>,
    pub span: SrcSpan,
}

/// `BasicBlockData` holds one basic block: a straight-line sequence of `statements` ending in
/// exactly one `terminator`. There is no fallthrough between adjacent blocks in
/// `Body::basic_blocks`. A block that merely continues into the next still ends with an explicit
/// `Goto`.
#[derive(Debug)]
pub struct BasicBlockData {
    pub statements: Vec<Statement>,
    pub terminator: Terminator,
}
