#include "CodeGen/CodeGen.hpp"
#include "llvm/IR/BasicBlock.h"

#include <cassert>
#include <llvm/Support/Casting.h>

using namespace phi;

void CodeGen::visit(Block &B) {
  for (auto &Stmt : B.getStmts()) {
    visit(*Stmt);
  }
}

void CodeGen::visit(Stmt &S) { S.accept(*this); }

void CodeGen::visit(ReturnStmt &S) {
  // Execute all deferred statements before returning
  executeDefers();

  if (S.hasExpr()) {
    // Return with value
    llvm::Value *RetVal = visit(S.getExpr());
    Builder.CreateRet(RetVal);
  } else {
    // Void return
    Builder.CreateRetVoid();
  }

  // Note: Control flow ends here, so no need to set insert point
}

void CodeGen::visit(DeferStmt &S) {
  // Add the deferred expression to the defer stack
  // It will be executed in reverse order when the function returns
  pushDefer(S.getDeferred());
}

void CodeGen::visit(IfStmt &S) {
  assert(CurrentFun != nullptr);

  auto blocks = createIfStatementBlocks(S);
  generateIfCondition(S, blocks);
  generateIfBranches(S, blocks);

  Builder.SetInsertPoint(blocks.ExitBB);
}

void CodeGen::visit(WhileStmt &S) {
  assert(CurrentFun != nullptr);

  auto BasicBlocks = createWhileLoopBlocks();
  pushLoopContext(BasicBlocks.ExitBB, BasicBlocks.CondBB);

  generateWhileCondition(S, BasicBlocks);
  generateWhileBody(S, BasicBlocks);

  popLoopContext();
  Builder.SetInsertPoint(BasicBlocks.ExitBB);
}

void CodeGen::visit(ForStmt &S) {
  assert(CurrentFun != nullptr);

  auto BasicBlocks = createForLoopBlocks();
  auto RangeInfo = extractRangeInfo(S);
  pushLoopContext(BasicBlocks.ExitBB, BasicBlocks.IncBB);

  generateForInit(S, RangeInfo, BasicBlocks);
  generateForCondition(S, RangeInfo, BasicBlocks);
  generateForBody(S, BasicBlocks);
  generateForIncrement(S, RangeInfo, BasicBlocks);

  popLoopContext();
  Builder.SetInsertPoint(BasicBlocks.ExitBB);
}

void CodeGen::visit(DeclStmt &S) { visit(S.getDecl()); }

void CodeGen::visit(BreakStmt &S) {
  llvm::BasicBlock *BreakTarget = getCurrentBreakTarget();
  if (BreakTarget) {
    Builder.CreateBr(BreakTarget);
  }
  // Note: Control flow ends here, so no need to set insert point
}

void CodeGen::visit(ContinueStmt &S) {
  llvm::BasicBlock *ContinueTarget = getCurrentContinueTarget();
  if (ContinueTarget) {
    Builder.CreateBr(ContinueTarget);
  }
  // Note: Control flow ends here, so no need to set insert point
}

void CodeGen::visit(ExprStmt &S) { S.getExpr().accept(*this); }

//===----------------------------------------------------------------------===//
// Loop Generation Helper Methods
//===----------------------------------------------------------------------===//

CodeGen::WhileLoopBlocks CodeGen::createWhileLoopBlocks() {
  WhileLoopBlocks blocks;
  blocks.CondBB = llvm::BasicBlock::Create(Context, "while.cond", CurrentFun);
  blocks.BodyBB = llvm::BasicBlock::Create(Context, "while.body", CurrentFun);
  blocks.ExitBB = llvm::BasicBlock::Create(Context, "while.exit", CurrentFun);
  return blocks;
}

void CodeGen::generateWhileCondition(WhileStmt &S,
                                     const WhileLoopBlocks &blocks) {
  breakIntoBB(blocks.CondBB);
  llvm::Value *Cond = visit(S.getCond());
  assert(Cond->getType()->isIntegerTy(1));
  Builder.CreateCondBr(Cond, blocks.BodyBB, blocks.ExitBB);
}

void CodeGen::generateWhileBody(WhileStmt &S, const WhileLoopBlocks &blocks) {
  Builder.SetInsertPoint(blocks.BodyBB);
  visit(S.getBody());
  breakIntoBB(blocks.CondBB);
}

CodeGen::ForLoopBlocks CodeGen::createForLoopBlocks() {
  ForLoopBlocks blocks;
  blocks.InitBB = llvm::BasicBlock::Create(Context, "for.init", CurrentFun);
  blocks.CondBB = llvm::BasicBlock::Create(Context, "for.cond", CurrentFun);
  blocks.BodyBB = llvm::BasicBlock::Create(Context, "for.body", CurrentFun);
  blocks.IncBB = llvm::BasicBlock::Create(Context, "for.inc", CurrentFun);
  blocks.ExitBB = llvm::BasicBlock::Create(Context, "for.exit", CurrentFun);
  return blocks;
}

CodeGen::ForRangeInfo CodeGen::extractRangeInfo(ForStmt &S) {
  ForRangeInfo info;
  auto *Range = llvm::dyn_cast<RangeLiteral>(&S.getRange());
  assert(Range && "For loop only supports range literals for now");

  info.Range = Range;
  info.Start = visit(Range->getStart());
  info.End = visit(Range->getEnd());
  return info;
}

void CodeGen::generateForInit(ForStmt &S, const ForRangeInfo &rangeInfo,
                              const ForLoopBlocks &blocks) {
  breakIntoBB(blocks.InitBB);
  auto &Decl = S.getLoopVar();
  llvm::AllocaInst *Var = stackAlloca(Decl);
  DeclMap[&Decl] = Var;
  store(rangeInfo.Start, Var, Decl.getType());
}

void CodeGen::generateForCondition(ForStmt &S, const ForRangeInfo &rangeInfo,
                                   const ForLoopBlocks &blocks) {
  breakIntoBB(blocks.CondBB);
  auto &Decl = S.getLoopVar();
  llvm::Value *CurVal = load(DeclMap[&Decl], Decl.getType());
  llvm::Value *Cond = rangeInfo.Range->isInclusive()
                          ? Builder.CreateICmpSLE(CurVal, rangeInfo.End)
                          : Builder.CreateICmpSLT(CurVal, rangeInfo.End);
  Builder.CreateCondBr(Cond, blocks.BodyBB, blocks.ExitBB);
}

void CodeGen::generateForBody(ForStmt &S, const ForLoopBlocks &blocks) {
  Builder.SetInsertPoint(blocks.BodyBB);
  visit(S.getBody());
  breakIntoBB(blocks.IncBB);
}

void CodeGen::generateForIncrement(ForStmt &S, const ForRangeInfo &rangeInfo,
                                   const ForLoopBlocks &blocks) {
  Builder.SetInsertPoint(blocks.IncBB);
  auto &Decl = S.getLoopVar();
  llvm::Value *CurrentVal = load(DeclMap[&Decl], Decl.getType());
  llvm::Value *IncrementedVal = Builder.CreateAdd(
      CurrentVal, llvm::ConstantInt::get(Decl.getType().toLLVM(Context), 1));
  Builder.CreateStore(IncrementedVal, DeclMap[&Decl]);
  breakIntoBB(blocks.CondBB);
}

//===----------------------------------------------------------------------===//
// If Statement Generation Helper Methods
//===----------------------------------------------------------------------===//

CodeGen::IfStatementBlocks CodeGen::createIfStatementBlocks(IfStmt &S) {
  IfStatementBlocks blocks;
  blocks.ThenBB = llvm::BasicBlock::Create(Context, "if.then", CurrentFun);
  blocks.ExitBB = llvm::BasicBlock::Create(Context, "if.exit", CurrentFun);
  blocks.ElseBB = S.hasElse()
                      ? llvm::BasicBlock::Create(Context, "if.else", CurrentFun)
                      : blocks.ExitBB;
  return blocks;
}

void CodeGen::generateIfCondition(IfStmt &S, const IfStatementBlocks &blocks) {
  llvm::Value *Cond = visit(S.getCond());
  assert(Cond->getType()->isIntegerTy(1));
  Builder.CreateCondBr(Cond, blocks.ThenBB, blocks.ElseBB);
}

void CodeGen::generateIfBranches(IfStmt &S, const IfStatementBlocks &blocks) {
  // Generate then branch
  Builder.SetInsertPoint(blocks.ThenBB);
  visit(S.getThen());
  breakIntoBB(blocks.ExitBB);

  // Generate else branch if it exists
  if (S.hasElse()) {
    Builder.SetInsertPoint(blocks.ElseBB);
    visit(S.getElse());
    breakIntoBB(blocks.ExitBB);
  }
}
