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

  // all if statements will have a BB for then and BB to exit to
  // else BB is exit BB if no else statement; otherwise, create new
  auto ThenBB = llvm::BasicBlock::Create(Context, "if.then", CurrentFun);
  auto ExitBB = llvm::BasicBlock::Create(Context, "if.exit", CurrentFun);
  auto ElseBB = (!S.hasElse())
                    ? ExitBB
                    : llvm::BasicBlock::Create(Context, "if.else", CurrentFun);

  llvm::Value *Cond = visit(S.getCond());
  assert(Cond->getType()->isIntegerTy(1));
  Builder.CreateCondBr(Cond, ThenBB, ElseBB);

  // insert all these into current function
  // also generate their code blocks too
  Builder.SetInsertPoint(ThenBB);
  visit(S.getThen());
  breakIntoBB(ExitBB);

  if (S.hasElse()) {
    Builder.SetInsertPoint(ElseBB);
    visit(S.getElse());
    breakIntoBB(ExitBB);
  }

  // set the builder to the exit BB for the next stmt
  Builder.SetInsertPoint(ExitBB);
}

void CodeGen::visit(WhileStmt &S) {
  assert(CurrentFun != nullptr);

  auto CondBB = llvm::BasicBlock::Create(Context, "while.cond", CurrentFun);
  auto ThenBB = llvm::BasicBlock::Create(Context, "while.then", CurrentFun);
  auto ExitBB = llvm::BasicBlock::Create(Context, "while.exit", CurrentFun);

  // Push loop context for break/continue statements
  pushLoopContext(ExitBB, CondBB);

  // Jump to condition block to start the loop
  breakIntoBB(CondBB);

  llvm::Value *Cond = visit(S.getCond());
  assert(Cond->getType()->isIntegerTy(1));
  Builder.CreateCondBr(Cond, ThenBB, ExitBB);

  // Generate the loop body
  Builder.SetInsertPoint(ThenBB);
  visit(S.getBody());
  breakIntoBB(CondBB); // go to conditional when done

  // Pop loop context and set insert point to exit
  popLoopContext();
  Builder.SetInsertPoint(ExitBB);
}

void CodeGen::visit(ForStmt &S) {
  assert(CurrentFun != nullptr);

  auto *InitBB = llvm::BasicBlock::Create(Context, "for.init", CurrentFun);
  auto *CondBB = llvm::BasicBlock::Create(Context, "for.cond", CurrentFun);
  auto *BodyBB = llvm::BasicBlock::Create(Context, "for.body", CurrentFun);
  auto *IncBB = llvm::BasicBlock::Create(Context, "for.inc", CurrentFun);
  auto *ExitBB = llvm::BasicBlock::Create(Context, "for.exit", CurrentFun);

  auto &Decl = S.getLoopVar();
  auto *Range = llvm::dyn_cast<RangeLiteral>(&S.getRange());
  assert(Range && "For loop only supports range literals for now");
  llvm::Value *Start = visit(Range->getStart());
  llvm::Value *End = visit(Range->getEnd());

  // Push loop context for break/continue statements
  pushLoopContext(ExitBB, IncBB);

  /* Generate Init Block */
  breakIntoBB(InitBB);
  llvm::AllocaInst *Var = stackAlloca(Decl);
  DeclMap[&Decl] = Var;
  store(Start, Var, Decl.getType());

  /* Generate Cond Block */
  breakIntoBB(CondBB);
  llvm::Value *CurVal = load(Var, Decl.getType());
  llvm::Value *Cond = Range->isInclusive() ? Builder.CreateICmpSLE(CurVal, End)
                                           : Builder.CreateICmpSLT(CurVal, End);
  Builder.CreateCondBr(Cond, BodyBB, ExitBB);

  /* Generate Body Block */
  Builder.SetInsertPoint(BodyBB);
  visit(S.getBody());
  breakIntoBB(IncBB);

  /* Generate Inc Block */
  Builder.SetInsertPoint(IncBB);
  llvm::Value *CurrentVal = load(Var, Decl.getType());
  llvm::Value *IncrementedVal = Builder.CreateAdd(
      CurrentVal, llvm::ConstantInt::get(Decl.getType().toLLVM(Context), 1));
  Builder.CreateStore(IncrementedVal, Var);
  breakIntoBB(CondBB);

  /* Pop loop context and set the insert point to exit for the next stmt */
  popLoopContext();
  Builder.SetInsertPoint(ExitBB);
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
