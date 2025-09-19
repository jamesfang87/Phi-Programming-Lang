#include "CodeGen/CodeGen.hpp"
#include "llvm/IR/BasicBlock.h"

#include <cassert>
#include <llvm/Support/Casting.h>

using namespace phi;

void CodeGen::visit(Block &B) {
  for (auto &Stmt : B.getStmts()) {
    visit(*Stmt);

    if (!Builder.GetInsertBlock())
      break;
  }
}

void CodeGen::visit(Stmt &S) { S.accept(*this); }

void CodeGen::visit(ReturnStmt &S) {}

void CodeGen::visit(DeferStmt &S) {}

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

  llvm::Value *Cond = visit(S.getCond());
  assert(Cond->getType()->isIntegerTy(1));

  // we first go to the BB for the cond and eval
  Builder.SetInsertPoint(CondBB);
  Builder.CreateCondBr(Cond, ThenBB, ExitBB);

  // now we go to the then BB
  Builder.SetInsertPoint(ThenBB);
  visit(S.getBody());
  breakIntoBB(CondBB); // go to conditional when done
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
  llvm::Value *IncrementedVal = Builder.CreateAdd(
      CurVal, llvm::ConstantInt::get(Decl.getType().toLLVM(Context), 1));
  Builder.CreateStore(IncrementedVal, Var);
  breakIntoBB(CondBB);

  /* Lastly, set the insert point to exit for the next stmt */
  Builder.SetInsertPoint(ExitBB);
}

void CodeGen::visit(DeclStmt &S) {
  auto &Decl = S.getDecl();
  llvm::AllocaInst *Var = stackAlloca(Decl);

  if (Decl.hasInit())
    store(visit(Decl.getInit()), Var, Decl.getInit().getType());

  DeclMap[&Decl] = Var;
}

void CodeGen::visit(BreakStmt &S) {}

void CodeGen::visit(ContinueStmt &S) {}

void CodeGen::visit(ExprStmt &S) { S.getExpr().accept(*this); }
