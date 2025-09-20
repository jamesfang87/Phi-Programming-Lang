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

  auto Blocks = createIfStatementBlocks(S);
  generateIfCondition(S, Blocks);
  generateIfBranches(S, Blocks);

  Builder.SetInsertPoint(Blocks.ExitBB);
}

void CodeGen::visit(WhileStmt &S) {
  assert(CurrentFun != nullptr);

  auto Blocks = createWhileLoopBlocks();
  pushLoopContext(Blocks.ExitBB, Blocks.CondBB);

  generateWhileCondition(S, Blocks);
  generateWhileBody(S, Blocks);

  popLoopContext();
  Builder.SetInsertPoint(Blocks.ExitBB);
}

void CodeGen::visit(ForStmt &S) {
  assert(CurrentFun != nullptr);

  auto Blocks = createForLoopBlocks();
  auto RangeInfo = extractRangeInfo(S);
  pushLoopContext(Blocks.ExitBB, Blocks.IncBB);

  generateForInit(S, RangeInfo, Blocks);
  generateForCondition(S, RangeInfo, Blocks);
  generateForBody(S, Blocks);
  generateForIncrement(S, RangeInfo, Blocks);

  popLoopContext();
  Builder.SetInsertPoint(Blocks.ExitBB);
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
  WhileLoopBlocks Blocks;
  Blocks.CondBB = llvm::BasicBlock::Create(Context, "while.cond", CurrentFun);
  Blocks.BodyBB = llvm::BasicBlock::Create(Context, "while.body", CurrentFun);
  Blocks.ExitBB = llvm::BasicBlock::Create(Context, "while.exit", CurrentFun);
  return Blocks;
}

void CodeGen::generateWhileCondition(WhileStmt &S,
                                     const WhileLoopBlocks &Blocks) {
  breakIntoBB(Blocks.CondBB);
  llvm::Value *Cond = visit(S.getCond());
  assert(Cond->getType()->isIntegerTy(1));
  Builder.CreateCondBr(Cond, Blocks.BodyBB, Blocks.ExitBB);
}

void CodeGen::generateWhileBody(WhileStmt &S, const WhileLoopBlocks &Blocks) {
  Builder.SetInsertPoint(Blocks.BodyBB);
  visit(S.getBody());
  breakIntoBB(Blocks.CondBB);
}

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
  Info.Start = visit(Range->getStart());
  Info.End = visit(Range->getEnd());
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
  llvm::Value *Cond = visit(S.getCond());
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
