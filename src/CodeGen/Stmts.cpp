#include "CodeGen/CodeGen.hpp"

#include <llvm/Support/Casting.h>

using namespace phi;

llvm::AllocaInst *
CodeGen::ensureReturnAllocaForCurrentFunction(llvm::Type *RetTy) {
  if (!CurrentFunction)
    return nullptr;
  auto It = ReturnAllocaMap.find(CurrentFunction);
  if (It != ReturnAllocaMap.end())
    return It->second;

  llvm::BasicBlock &Entry = CurrentFunction->getEntryBlock();
  llvm::IRBuilder<> TmpBuilder(&Entry, Entry.begin());
  llvm::AllocaInst *Alloca =
      TmpBuilder.CreateAlloca(RetTy, nullptr, "ret.addr");
  ReturnAllocaMap[CurrentFunction] = Alloca;
  return Alloca;
}

void CodeGen::recordDeferForCurrentFunction(Stmt *S) {
  if (!CurrentFunction)
    throw std::runtime_error("defer used outside function");
  DeferMap[CurrentFunction].push_back(S);
}

void CodeGen::emitDeferredForFunction(llvm::Function *F) {
  auto It = DeferMap.find(F);
  if (It == DeferMap.end())
    return;
  auto &Vec = It->second;
  for (auto Rit = Vec.rbegin(); Rit != Vec.rend(); ++Rit) {
    Stmt *DS = *Rit;
    if (DS) {
      // Cast back to DeferStmt and execute the deferred expression
      if (auto *DeferS = llvm::dyn_cast<DeferStmt>(DS)) {
        DeferS->getDeferred().accept(*this);
      }
    }
  }
  Vec.clear();
}

void CodeGen::visit(ReturnStmt &S) {
  llvm::Type *RTy = CurrentFunction->getReturnType();
  if (S.hasExpr()) {
    llvm::Value *Rv = S.getExpr().accept(*this);
    if (!RTy->isVoidTy()) {
      llvm::AllocaInst *RA = ensureReturnAllocaForCurrentFunction(RTy);
      Builder.CreateStore(Rv, RA);
    }
  }

  // branch to cleanup block
  llvm::BasicBlock *Cleanup = nullptr;
  auto CI = CleanupBlockMap.find(CurrentFunction);
  if (CI == CleanupBlockMap.end()) {
    Cleanup =
        llvm::BasicBlock::Create(Context, "func.cleanup", CurrentFunction);
    CleanupBlockMap[CurrentFunction] = Cleanup;
  } else
    Cleanup = CI->second;

  Builder.CreateBr(Cleanup);

  // continue code generation safely
  llvm::BasicBlock *Dummy =
      llvm::BasicBlock::Create(Context, "after_return", CurrentFunction);
  Builder.SetInsertPoint(Dummy);
}

void CodeGen::visit(DeferStmt &S) { recordDeferForCurrentFunction(&S); }

void CodeGen::visit(IfStmt &S) {
  llvm::Function *F = Builder.GetInsertBlock()->getParent();
  llvm::Value *CondV = S.getCond().accept(*this);
  if (!CondV->getType()->isIntegerTy(1))
    CondV = Builder.CreateICmpNE(CondV,
                                 llvm::ConstantInt::get(CondV->getType(), 0));

  bool HasElse = S.hasElse();
  bool HasThen = !S.getThen().getStmts().empty();
  bool HasElseBody = HasElse && !S.getElse().getStmts().empty();

  llvm::BasicBlock *ThenBB =
      HasThen ? llvm::BasicBlock::Create(Context, "if.then", F) : nullptr;
  llvm::BasicBlock *ElseBB =
      HasElseBody ? llvm::BasicBlock::Create(Context, "if.else", F) : nullptr;
  llvm::BasicBlock *MergeBB =
      (HasThen || HasElseBody) ? llvm::BasicBlock::Create(Context, "if.end", F)
                               : nullptr;

  if (ThenBB && ElseBB)
    Builder.CreateCondBr(CondV, ThenBB, ElseBB);
  else if (ThenBB)
    Builder.CreateCondBr(CondV, ThenBB, MergeBB);
  else if (ElseBB)
    Builder.CreateCondBr(CondV, MergeBB, ElseBB);
  else
    Builder.CreateBr(MergeBB);

  if (ThenBB) {
    Builder.SetInsertPoint(ThenBB);
    for (auto &St : S.getThen().getStmts())
      St->accept(*this);
    if (!Builder.GetInsertBlock()->getTerminator())
      Builder.CreateBr(MergeBB);
  }
  if (ElseBB) {
    Builder.SetInsertPoint(ElseBB);
    for (auto &St : S.getElse().getStmts())
      St->accept(*this);
    if (!Builder.GetInsertBlock()->getTerminator())
      Builder.CreateBr(MergeBB);
  }
  if (MergeBB)
    Builder.SetInsertPoint(MergeBB);
}

void CodeGen::visit(WhileStmt &S) {
  llvm::Function *F = Builder.GetInsertBlock()->getParent();
  llvm::BasicBlock *CondBB = llvm::BasicBlock::Create(Context, "while.cond", F);
  llvm::BasicBlock *BodyBB = llvm::BasicBlock::Create(Context, "while.body", F);
  llvm::BasicBlock *ExitBB = llvm::BasicBlock::Create(Context, "while.exit", F);

  LoopStack.push_back({ExitBB, CondBB});
  Builder.CreateBr(CondBB);
  Builder.SetInsertPoint(CondBB);

  llvm::Value *CondV = S.getCond().accept(*this);
  if (!CondV->getType()->isIntegerTy(1))
    CondV = Builder.CreateICmpNE(CondV,
                                 llvm::ConstantInt::get(CondV->getType(), 0));

  Builder.CreateCondBr(CondV, BodyBB, ExitBB);

  Builder.SetInsertPoint(BodyBB);
  for (auto &St : S.getBody().getStmts())
    St->accept(*this);
  if (!Builder.GetInsertBlock()->getTerminator())
    Builder.CreateBr(CondBB);

  LoopStack.pop_back();
  Builder.SetInsertPoint(ExitBB);
}

void CodeGen::visit(ForStmt &S) {
  llvm::Function *F = Builder.GetInsertBlock()->getParent();
  llvm::BasicBlock *InitBB = llvm::BasicBlock::Create(Context, "for.init", F);
  llvm::BasicBlock *CondBB = llvm::BasicBlock::Create(Context, "for.cond", F);
  llvm::BasicBlock *BodyBB = llvm::BasicBlock::Create(Context, "for.body", F);
  llvm::BasicBlock *IncBB = llvm::BasicBlock::Create(Context, "for.inc", F);
  llvm::BasicBlock *ExitBB = llvm::BasicBlock::Create(Context, "for.exit", F);

  Builder.CreateBr(InitBB);
  Builder.SetInsertPoint(InitBB);

  VarDecl &LV = S.getLoopVar();
  llvm::Type *VT = LV.getType().toLLVM(Context);
  llvm::AllocaInst *Alloc = Builder.CreateAlloca(VT, nullptr, LV.getId());
  DeclMap[&LV] = Alloc;

  auto *Range = llvm::dyn_cast<RangeLiteral>(&S.getRange());
  if (!Range)
    throw std::runtime_error("for supports only range literal");

  llvm::Value *StartV = Range->getStart().accept(*this);
  Builder.CreateStore(StartV, Alloc);
  Builder.CreateBr(CondBB);

  Builder.SetInsertPoint(CondBB);
  llvm::Value *CurrV = Builder.CreateLoad(VT, Alloc, "loop.var");
  llvm::Value *EndV = Range->getEnd().accept(*this);

  llvm::Value *Cmp = Range->isInclusive() ? Builder.CreateICmpSLE(CurrV, EndV)
                                          : Builder.CreateICmpSLT(CurrV, EndV);
  if (!Cmp->getType()->isIntegerTy(1))
    Cmp = Builder.CreateICmpNE(Cmp, llvm::ConstantInt::get(Cmp->getType(), 0));
  Builder.CreateCondBr(Cmp, BodyBB, ExitBB);

  LoopStack.push_back({ExitBB, IncBB});
  Builder.SetInsertPoint(BodyBB);
  for (auto &St : S.getBody().getStmts())
    St->accept(*this);
  if (!Builder.GetInsertBlock()->getTerminator())
    Builder.CreateBr(IncBB);

  Builder.SetInsertPoint(IncBB);
  llvm::Value *IncV = Builder.CreateAdd(CurrV, llvm::ConstantInt::get(VT, 1));
  Builder.CreateStore(IncV, Alloc);
  Builder.CreateBr(CondBB);

  LoopStack.pop_back();
  Builder.SetInsertPoint(ExitBB);
}

void CodeGen::visit(DeclStmt &S) {
  VarDecl &VD = S.getDecl();
  llvm::Type *T = VD.getType().toLLVM(Context);
  llvm::AllocaInst *A = Builder.CreateAlloca(T, nullptr, VD.getId());
  DeclMap[&VD] = A;
  if (VD.hasInit()) {
    llvm::Value *InitV = VD.getInit().accept(*this);
    Builder.CreateStore(InitV, A);
  }
}

void CodeGen::visit(BreakStmt &S) {
  (void)S;
  if (LoopStack.empty())
    throw std::runtime_error("break used outside loop");
  Builder.CreateBr(LoopStack.back().BreakTarget);
  llvm::BasicBlock *Dummy = llvm::BasicBlock::Create(
      Context, "after_break", Builder.GetInsertBlock()->getParent());
  Builder.SetInsertPoint(Dummy);
}

void CodeGen::visit(ContinueStmt &S) {
  (void)S;
  if (LoopStack.empty())
    throw std::runtime_error("continue used outside loop");
  Builder.CreateBr(LoopStack.back().ContinueTarget);
  llvm::BasicBlock *Dummy = llvm::BasicBlock::Create(
      Context, "after_continue", Builder.GetInsertBlock()->getParent());
  Builder.SetInsertPoint(Dummy);
}

void CodeGen::visit(ExprStmt &S) {
  // Evaluate the expression but ignore the result
  S.getExpr().accept(*this);
}

llvm::Value *CodeGen::visit(Expr &S) {
  // evaluate and ignore result
  return S.accept(*this);
}
