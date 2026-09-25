use smallvec::SmallVec;

use crate::ast::Ident;
use crate::driver::source::SrcSpan;
use crate::hir::{DefId, HirId};
use crate::mir::ids::BasicBlock;
use crate::mir::statement::Statement;
use crate::mir::terminator::Terminator;
use crate::typeck::ty::Ty;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum BodyKind {
    Function,
    Closure,
}

#[derive(Debug)]
pub struct Body {
    pub def_id: DefId,
    pub kind: BodyKind,
    pub basic_blocks: Vec<BasicBlockData>,
    pub local_decls: Vec<LocalDecl>,

    pub param_count: usize,

    pub generics: Vec<HirId>,
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

    pub name: Option<Ident>,
    pub span: SrcSpan,
}

#[derive(Debug)]
pub struct BasicBlockData {
    pub statements: Vec<Statement>,
    pub terminator: Terminator,
}
