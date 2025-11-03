#include "CodeGen/CodeGen.hpp"

#include <cassert>
#include <llvm/IR/BasicBlock.h>
#include <llvm/Support/Casting.h>

namespace phi {

//===----------------------------------------------------------------------===//
// While Loop Generation Helper Methods
//===----------------------------------------------------------------------===//

CodeGen::WhileLoopBlocks CodeGen::createWhileLoopBlocks() {
  WhileLoopBlocks Blocks;
  Blocks.CondBB = llvm::BasicBlock::Create(Context, "while.cond", CurrentFun);
  Blocks.BodyBB = llvm::BasicBlock::Create(Context, "while.body", CurrentFun);
  Blocks.ExitBB = llvm::BasicBlock::Create(Context, "while.exit", CurrentFun);
  return Blocks;
}

void CodeGen::generateWhileCondition(WhileStmt &S,
                                     const WhileLoopBlocks &Blocks) {
  breakIntoBB(Blocks.CondBB);
  llvm::Value *Cond = load(visit(S.getCond()), S.getCond().getType());
  assert(Cond->getType()->isIntegerTy(1));
  Builder.CreateCondBr(Cond, Blocks.BodyBB, Blocks.ExitBB);
}

void CodeGen::generateWhileBody(WhileStmt &S, const WhileLoopBlocks &Blocks) {
  Builder.SetInsertPoint(Blocks.BodyBB);
  visit(S.getBody());
  breakIntoBB(Blocks.CondBB);
}

//===----------------------------------------------------------------------===//
// For Loop Generation Helper Methods
//===----------------------------------------------------------------------===//

CodeGen::ForLoopBlocks CodeGen::createForLoopBlocks() {
  ForLoopBlocks Blocks;
  Blocks.InitBB = llvm::BasicBlock::Create(Context, "for.init", CurrentFun);
  Blocks.CondBB = llvm::BasicBlock::Create(Context, "for.cond", CurrentFun);
  Blocks.BodyBB = llvm::BasicBlock::Create(Context, "for.body", CurrentFun);
  Blocks.IncBB = llvm::BasicBlock::Create(Context, "for.inc", CurrentFun);
  Blocks.ExitBB = llvm::BasicBlock::Create(Context, "for.exit", CurrentFun);
  return Blocks;
}

CodeGen::ForRangeInfo CodeGen::extractRangeInfo(ForStmt &S) {
  ForRangeInfo Info;
  auto *Range = llvm::dyn_cast<RangeLiteral>(&S.getRange());
  assert(Range && "For loop only supports range literals for now");

  Info.Range = Range;
  Info.Start = load(visit(Range->getStart()), Range->getStart().getType());
  Info.End = load(visit(Range->getEnd()), Range->getEnd().getType());
  return Info;
}

void CodeGen::generateForInit(ForStmt &S, const ForRangeInfo &RangeInfo,
                              const ForLoopBlocks &Blocks) {
  breakIntoBB(Blocks.InitBB);
  auto &Decl = S.getLoopVar();
  llvm::AllocaInst *Var = stackAlloca(Decl);
  DeclMap[&Decl] = Var;
  store(RangeInfo.Start, Var, Decl.getType());
}

void CodeGen::generateForCondition(ForStmt &S, const ForRangeInfo &RangeInfo,
                                   const ForLoopBlocks &Blocks) {
  breakIntoBB(Blocks.CondBB);
  auto &Decl = S.getLoopVar();
  llvm::Value *CurVal = load(DeclMap[&Decl], Decl.getType());
  llvm::Value *Cond = RangeInfo.Range->isInclusive()
                          ? Builder.CreateICmpSLE(CurVal, RangeInfo.End)
                          : Builder.CreateICmpSLT(CurVal, RangeInfo.End);
  Builder.CreateCondBr(Cond, Blocks.BodyBB, Blocks.ExitBB);
}

void CodeGen::generateForBody(ForStmt &S, const ForLoopBlocks &Blocks) {
  Builder.SetInsertPoint(Blocks.BodyBB);
  visit(S.getBody());
  breakIntoBB(Blocks.IncBB);
}

void CodeGen::generateForIncrement(ForStmt &S, const ForRangeInfo &RangeInfo,
                                   const ForLoopBlocks &Blocks) {
  Builder.SetInsertPoint(Blocks.IncBB);
  auto &Decl = S.getLoopVar();
  llvm::Value *CurrentVal = load(DeclMap[&Decl], Decl.getType());
  llvm::Value *IncrementedVal = Builder.CreateAdd(
      CurrentVal, llvm::ConstantInt::get(Decl.getType().toLLVM(Context), 1));
  Builder.CreateStore(IncrementedVal, DeclMap[&Decl]);
  breakIntoBB(Blocks.CondBB);
}

//===----------------------------------------------------------------------===//
// If Statement Generation Helper Methods
//===----------------------------------------------------------------------===//

CodeGen::IfStatementBlocks CodeGen::createIfStatementBlocks(IfStmt &S) {
  IfStatementBlocks Blocks;
  Blocks.ThenBB = llvm::BasicBlock::Create(Context, "if.then", CurrentFun);
  Blocks.ExitBB = llvm::BasicBlock::Create(Context, "if.exit", CurrentFun);
  Blocks.ElseBB = S.hasElse()
                      ? llvm::BasicBlock::Create(Context, "if.else", CurrentFun)
                      : Blocks.ExitBB;
  return Blocks;
}

void CodeGen::generateIfCondition(IfStmt &S, const IfStatementBlocks &Blocks) {
  llvm::Value *Cond = load(visit(S.getCond()), S.getCond().getType());
  assert(Cond->getType()->isIntegerTy(1));
  Builder.CreateCondBr(Cond, Blocks.ThenBB, Blocks.ElseBB);
}

void CodeGen::generateIfBranches(IfStmt &S, const IfStatementBlocks &Blocks) {
  // Generate then branch
  Builder.SetInsertPoint(Blocks.ThenBB);
  visit(S.getThen());
  breakIntoBB(Blocks.ExitBB);

  // Generate else branch if it exists
  if (S.hasElse()) {
    Builder.SetInsertPoint(Blocks.ElseBB);
    visit(S.getElse());
    breakIntoBB(Blocks.ExitBB);
  }
}

} // namespace phi
