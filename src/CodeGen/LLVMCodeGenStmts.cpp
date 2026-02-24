#include "CodeGen/LLVMCodeGen.hpp"

#include <llvm/ADT/TypeSwitch.h>
#include <llvm/IR/Constants.h>
#include <llvm/IR/DerivedTypes.h>
#include <llvm/IR/Verifier.h>

using namespace phi;

//===----------------------------------------------------------------------===//
// Phase 3: Desugaring
//===----------------------------------------------------------------------===//

void CodeGen::desugar() {
  for (auto *M : Ast) {
    desugarModule(M);
  }
}

void CodeGen::desugarModule(ModuleDecl *M) {
  for (auto &Item : M->getItems()) {
    if (auto *F = llvm::dyn_cast<FunDecl>(Item.get())) {
      desugarFunction(F);
    } else if (auto *S = llvm::dyn_cast<StructDecl>(Item.get())) {
      for (auto &Method : S->getMethods()) {
        desugarMethod(Method.get());
      }
    } else if (auto *E = llvm::dyn_cast<EnumDecl>(Item.get())) {
      for (auto &Method : E->getMethods()) {
        desugarMethod(Method.get());
      }
    }
  }
}

void CodeGen::desugarFunction(FunDecl *F) { desugarBlock(&F->getBody()); }

void CodeGen::desugarMethod(MethodDecl *M) { desugarBlock(&M->getBody()); }

void CodeGen::desugarBlock(Block *B) {
  for (auto &S : B->getStmts()) {
    desugarStmt(S.get());
  }
}

void CodeGen::desugarStmt(Stmt *S) {
  if (auto *DS = llvm::dyn_cast<DeclStmt>(S)) {
    if (DS->getDecl().hasInit()) {
      desugarExpr(&DS->getDecl().getInit());
    }
  } else if (auto *RS = llvm::dyn_cast<ReturnStmt>(S)) {
    if (RS->hasExpr()) {
      desugarExpr(&RS->getExpr());
    }
  } else if (auto *IS = llvm::dyn_cast<IfStmt>(S)) {
    desugarExpr(&IS->getCond());
    desugarBlock(&IS->getThen());
    if (IS->hasElse()) {
      desugarBlock(&IS->getElse());
    }
  } else if (auto *WS = llvm::dyn_cast<WhileStmt>(S)) {
    desugarExpr(&WS->getCond());
    desugarBlock(&WS->getBody());
  } else if (auto *FS = llvm::dyn_cast<ForStmt>(S)) {
    desugarExpr(&FS->getRange());
    desugarBlock(&FS->getBody());
  } else if (auto *ES = llvm::dyn_cast<ExprStmt>(S)) {
    desugarExpr(&ES->getExpr());
  }
}

void CodeGen::desugarExpr(Expr *E) {
  if (!E)
    return;

  // Recursively process child expressions first
  if (auto *AI = llvm::dyn_cast<AdtInit>(E)) {
    for (auto &Init : AI->getInits()) {
      desugarExpr(Init->getInitValue());
    }
  } else if (auto *FC = llvm::dyn_cast<FunCallExpr>(E)) {
    for (auto &Arg : FC->getArgs()) {
      desugarExpr(Arg.get());
    }
  } else if (auto *MC = llvm::dyn_cast<MethodCallExpr>(E)) {
    desugarExpr(MC->getBase());
    for (auto &Arg : MC->getArgs()) {
      desugarExpr(Arg.get());
    }
    // Note: actual method->function transformation happens in codegen
  } else if (auto *BO = llvm::dyn_cast<BinaryOp>(E)) {
    desugarExpr(&BO->getLhs());
    desugarExpr(&BO->getRhs());
  } else if (auto *UO = llvm::dyn_cast<UnaryOp>(E)) {
    desugarExpr(&UO->getOperand());
  } else if (auto *FA = llvm::dyn_cast<FieldAccessExpr>(E)) {
    desugarExpr(FA->getBase());
  } else if (auto *ME = llvm::dyn_cast<MatchExpr>(E)) {
    desugarExpr(ME->getScrutinee());
    for (auto &Arm : ME->getArms()) {
      desugarBlock(Arm.Body.get());
      if (Arm.Return)
        desugarExpr(Arm.Return);
    }
  } else if (auto *IE = llvm::dyn_cast<TupleIndex>(E)) {
    desugarExpr(IE->getBase());
    desugarExpr(IE->getIndex());
  } else if (auto *TL = llvm::dyn_cast<TupleLiteral>(E)) {
    for (auto &Elem : TL->getElements()) {
      desugarExpr(Elem.get());
    }
  }
  // TODO: ArrayLiteral desugar
}

//===----------------------------------------------------------------------===//
// Phase 4: Statement Codegen
//===----------------------------------------------------------------------===//

void CodeGen::codegenBlock(Block *B) {
  for (auto &S : B->getStmts()) {
    if (hasTerminator())
      break;
    codegenStmt(S.get());
  }
}

void CodeGen::codegenStmt(Stmt *S) {
  if (auto *DS = llvm::dyn_cast<DeclStmt>(S)) {
    codegenDeclStmt(DS);
  } else if (auto *RS = llvm::dyn_cast<ReturnStmt>(S)) {
    codegenReturnStmt(RS);
  } else if (auto *IS = llvm::dyn_cast<IfStmt>(S)) {
    codegenIfStmt(IS);
  } else if (auto *WS = llvm::dyn_cast<WhileStmt>(S)) {
    codegenWhileStmt(WS);
  } else if (auto *FS = llvm::dyn_cast<ForStmt>(S)) {
    codegenForStmt(FS);
  } else if (auto *BS = llvm::dyn_cast<BreakStmt>(S)) {
    codegenBreakStmt(BS);
  } else if (auto *CS = llvm::dyn_cast<ContinueStmt>(S)) {
    codegenContinueStmt(CS);
  } else if (auto *ES = llvm::dyn_cast<ExprStmt>(S)) {
    codegenExprStmt(ES);
  }
}

void CodeGen::codegenDeclStmt(DeclStmt *S) {
  auto &Decl = S->getDecl();
  llvm::Type *Ty = getLLVMType(Decl.getType());

  auto *Alloca = createEntryBlockAlloca(CurrentFunction, Decl.getId(), Ty);
  NamedValues[&Decl] = Alloca;

  if (Decl.hasInit()) {
    llvm::Value *InitVal = codegenExpr(&Decl.getInit());    
    if (InitVal->getType() != Alloca->getAllocatedType()) {
        llvm::errs() << "TYPE MISMATCH IN STORE!\n";
    }

    Builder.CreateStore(InitVal, Alloca);
  }
}

void CodeGen::codegenReturnStmt(ReturnStmt *S) {
  if (S->hasExpr()) {
    llvm::Value *RetVal = codegenExpr(&S->getExpr());
    Builder.CreateRet(RetVal);
  } else {
    Builder.CreateRetVoid();
  }
}

void CodeGen::codegenIfStmt(IfStmt *S) {
  llvm::Value *Cond = codegenExpr(&S->getCond());

  // Convert to i1 if needed
  if (!Cond->getType()->isIntegerTy(1)) {
    Cond = Builder.CreateICmpNE(
        Cond, llvm::Constant::getNullValue(Cond->getType()), "ifcond");
  }

  auto *ThenBB = llvm::BasicBlock::Create(Context, "then", CurrentFunction);
  auto *ElseBB = llvm::BasicBlock::Create(Context, "else", CurrentFunction);
  auto *MergeBB = llvm::BasicBlock::Create(Context, "ifcont", CurrentFunction);

  Builder.CreateCondBr(Cond, ThenBB, ElseBB);

  // Then block
  Builder.SetInsertPoint(ThenBB);
  codegenBlock(&S->getThen());
  if (!hasTerminator())
    Builder.CreateBr(MergeBB);

  // Else block
  Builder.SetInsertPoint(ElseBB);
  if (S->hasElse()) {
    codegenBlock(&S->getElse());
  }
  if (!hasTerminator())
    Builder.CreateBr(MergeBB);

  Builder.SetInsertPoint(MergeBB);
}

void CodeGen::codegenWhileStmt(WhileStmt *S) {
  auto *CondBB =
      llvm::BasicBlock::Create(Context, "while.cond", CurrentFunction);
  auto *BodyBB =
      llvm::BasicBlock::Create(Context, "while.body", CurrentFunction);
  auto *AfterBB =
      llvm::BasicBlock::Create(Context, "while.end", CurrentFunction);

  Builder.CreateBr(CondBB);

  // Condition
  Builder.SetInsertPoint(CondBB);
  llvm::Value *Cond = codegenExpr(&S->getCond());
  if (!Cond->getType()->isIntegerTy(1)) {
    Cond = Builder.CreateICmpNE(
        Cond, llvm::Constant::getNullValue(Cond->getType()), "whilecond");
  }
  Builder.CreateCondBr(Cond, BodyBB, AfterBB);

  // Body
  Builder.SetInsertPoint(BodyBB);
  LoopStack.emplace_back(CondBB, AfterBB);
  codegenBlock(&S->getBody());
  LoopStack.pop_back();
  if (!hasTerminator())
    Builder.CreateBr(CondBB);

  Builder.SetInsertPoint(AfterBB);
}

void CodeGen::codegenForStmt(ForStmt *S) {
  // For now, simplified: for x in start..end
  auto *InitBB = llvm::BasicBlock::Create(Context, "for.init", CurrentFunction);
  auto *CondBB = llvm::BasicBlock::Create(Context, "for.cond", CurrentFunction);
  auto *BodyBB = llvm::BasicBlock::Create(Context, "for.body", CurrentFunction);
  auto *IncBB = llvm::BasicBlock::Create(Context, "for.inc", CurrentFunction);
  auto *AfterBB = llvm::BasicBlock::Create(Context, "for.end", CurrentFunction);

  Builder.CreateBr(InitBB);

  // Init: allocate loop variable
  Builder.SetInsertPoint(InitBB);
  auto &LoopVar = S->getLoopVar();
  llvm::Type *Ty = getLLVMType(LoopVar.getType());
  auto *LoopVarAlloca =
      createEntryBlockAlloca(CurrentFunction, LoopVar.getId(), Ty);
  NamedValues[&LoopVar] = LoopVarAlloca;

  // Get range bounds
  if (auto *RL = llvm::dyn_cast<RangeLiteral>(&S->getRange())) {
    llvm::Value *Start = codegenExpr(&RL->getStart());
    llvm::Value *End = codegenExpr(&RL->getEnd());
    Builder.CreateStore(Start, LoopVarAlloca);
    Builder.CreateBr(CondBB);

    // Condition
    Builder.SetInsertPoint(CondBB);
    llvm::Value *Current = Builder.CreateLoad(Ty, LoopVarAlloca);
    llvm::Value *Cond = Builder.CreateICmpSLT(Current, End, "forcond");
    Builder.CreateCondBr(Cond, BodyBB, AfterBB);

    // Body
    Builder.SetInsertPoint(BodyBB);
    LoopStack.emplace_back(IncBB, AfterBB);
    codegenBlock(&S->getBody());
    LoopStack.pop_back();
    if (!hasTerminator())
      Builder.CreateBr(IncBB);

    // Increment
    Builder.SetInsertPoint(IncBB);
    Current = Builder.CreateLoad(Ty, LoopVarAlloca);
    llvm::Value *Next = Builder.CreateAdd(Current, llvm::ConstantInt::get(Ty, 1), "inc");
    Builder.CreateStore(Next, LoopVarAlloca);
    Builder.CreateBr(CondBB);
  } else {
    // Fallback: just execute body once
    Builder.CreateBr(CondBB);
    Builder.SetInsertPoint(CondBB);
    Builder.CreateBr(BodyBB);
    Builder.SetInsertPoint(BodyBB);
    codegenBlock(&S->getBody());
    if (!hasTerminator())
      Builder.CreateBr(AfterBB);
  }

  Builder.SetInsertPoint(AfterBB);
}

void CodeGen::codegenBreakStmt(BreakStmt * /*S*/) {
  if (!LoopStack.empty()) {
    Builder.CreateBr(LoopStack.back().AfterBB);
  }
}

void CodeGen::codegenContinueStmt(ContinueStmt * /*S*/) {
  if (!LoopStack.empty()) {
    Builder.CreateBr(LoopStack.back().CondBB);
  }
}

void CodeGen::codegenExprStmt(ExprStmt *S) { codegenExpr(&S->getExpr()); }
