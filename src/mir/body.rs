use smallvec::SmallVec;

use crate::ast::{Ident, Mutability};
use crate::driver::source::SrcSpan;
use crate::hir::DefId;
use crate::mir::ids::BasicBlock;
use crate::mir::statement::Statement;
use crate::mir::terminator::Terminator;
use crate::typeck::ty::Ty;

/// `Body` is the MIR of one definition.
/// Only definitions with executable code has a `Body`.
/// This means that only functions and closures have a `Body` while
/// structs, enums, and traits do not
#[derive(Debug)]
pub struct Body {
    pub def_id: DefId,
    pub basic_blocks: Vec<BasicBlockData>,
    pub local_decls: Vec<LocalDecl>,
    /// `arg_count` is the number of `local_decls` that are parameters, `self` included. Slots
    /// `1..=arg_count` are the parameters in declared order, slot `0` is always the return place,
    /// and every slot after `arg_count` is a `let` binding or a compiler-introduced temporary.
    pub param_count: usize,
    pub span: SrcSpan,
}

impl Body {
    pub fn successors(&self, block: BasicBlock) -> impl Iterator<Item = BasicBlock> + '_ {
        self.basic_blocks[block.index()].terminator.successors()
    }

    pub fn predecessors(&self) -> Predecessors {
        let mut preds = vec![SmallVec::new(); self.basic_blocks.len()];
        for (index, block) in self.basic_blocks.iter().enumerate() {
            let from = BasicBlock::from_usize(index);
            for target in block.terminator.successors() {
                preds[target.index()].push(from);
            }
        }
        Predecessors(preds)
    }
}

#[derive(Debug)]
pub struct Predecessors(Vec<SmallVec<[BasicBlock; 4]>>);

impl Predecessors {
    pub fn of(&self, block: BasicBlock) -> &[BasicBlock] {
        &self.0[block.index()]
    }
}

#[derive(Debug)]
pub struct LocalDecl {
    pub ty: Ty,
    pub mutability: Mutability,
    /// `name` is the source name of a user-written local. It is `None` for a compiler-introduced temporary.
    pub name: Option<Ident>,
    pub span: SrcSpan,
}

/// `BasicBlockData` holds one basic block: a sequence of `statements` ending in
/// exactly one `terminator`. There is no fallthrough between adjacent blocks in
/// `Body::basic_blocks`. A block that merely continues into the next still ends with an explicit
/// `Goto`.
#[derive(Debug)]
pub struct BasicBlockData {
    pub statements: Vec<Statement>,
    pub terminator: Terminator,
}
