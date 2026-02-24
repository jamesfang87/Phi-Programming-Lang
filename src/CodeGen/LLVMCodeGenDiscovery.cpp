#include "CodeGen/LLVMCodeGen.hpp"

#include <cassert>

#include <llvm/ADT/TypeSwitch.h>
#include <llvm/IR/Constants.h>
#include <llvm/IR/DerivedTypes.h>
#include <llvm/IR/Verifier.h>

using namespace phi;

//===----------------------------------------------------------------------===//
// Phase 1: Discovery
//===----------------------------------------------------------------------===//

void CodeGen::discoverInstantiations() {
  for (auto *M : Ast) {
    discoverInModule(M);
  }
}

void CodeGen::discoverInModule(ModuleDecl *M) {
  for (auto &Item : M->getItems()) {
    if (auto *F = llvm::dyn_cast<FunDecl>(Item.get())) {
      discoverInFunction(F);
    }

    if (auto *A = llvm::dyn_cast<AdtDecl>(Item.get())) {
      for (auto &Method : A->getMethods()) {
        discoverInMethod(Method.get());
      }
    }
  }
}

void CodeGen::discoverInFunction(FunDecl *F) { discoverInBlock(&F->getBody()); }

void CodeGen::discoverInMethod(MethodDecl *M) {
  discoverInBlock(&M->getBody());
}

void CodeGen::discoverInBlock(Block *B) {
  for (auto &S : B->getStmts()) {
    discoverInStmt(S.get());
  }
}

void CodeGen::discoverInStmt(Stmt *S) {
  if (auto *DS = llvm::dyn_cast<DeclStmt>(S)) {
    if (DS->getDecl().hasType()) {
      discoverInType(DS->getDecl().getType());
    }

    if (DS->getDecl().hasInit()) {
      discoverInExpr(&DS->getDecl().getInit());
    }
  }

  if (auto *RS = llvm::dyn_cast<ReturnStmt>(S)) {
    if (RS->hasExpr()) {
      discoverInExpr(&RS->getExpr());
    }
  }

  if (auto *IS = llvm::dyn_cast<IfStmt>(S)) {
    discoverInExpr(&IS->getCond());
    discoverInBlock(&IS->getThen());
    if (IS->hasElse()) {
      discoverInBlock(&IS->getElse());
    }
  }

  if (auto *WS = llvm::dyn_cast<WhileStmt>(S)) {
    discoverInExpr(&WS->getCond());
    discoverInBlock(&WS->getBody());
  }

  if (auto *FS = llvm::dyn_cast<ForStmt>(S)) {
    discoverInExpr(&FS->getRange());
    discoverInBlock(&FS->getBody());
  }

  if (auto *ES = llvm::dyn_cast<ExprStmt>(S)) {
    discoverInExpr(&ES->getExpr());
  }
}

void CodeGen::discoverInExpr(Expr *E) {
  if (!E)
    return;

  if (auto *X = llvm::dyn_cast<AdtInit>(E)) {
    assert(X->getDecl());
    recordInstantiation(X->getDecl(), X->getTypeArgs());

    for (auto &Init : X->getInits()) {
      discoverInExpr(Init->getInitValue());
    }
  }

  if (auto *X = llvm::dyn_cast<FunCallExpr>(E)) {
    assert(X->getDecl());
    recordInstantiation(X->getDecl(), X->getTypeArgs());

    for (auto &Arg : X->getArgs()) {
      discoverInExpr(Arg.get());
    }
  }

  if (auto *X = llvm::dyn_cast<MethodCallExpr>(E)) {
    discoverInExpr(X->getBase());
    assert(X->getMethodPtr());
    recordInstantiation(X->getMethodPtr(), X->getTypeArgs());

    for (auto &Arg : X->getArgs()) {
      discoverInExpr(Arg.get());
    }
  }

  if (auto *X = llvm::dyn_cast<BinaryOp>(E)) {
    discoverInExpr(&X->getLhs());
    discoverInExpr(&X->getRhs());
  }

  if (auto *X = llvm::dyn_cast<UnaryOp>(E)) {
    discoverInExpr(&X->getOperand());
  }

  if (auto *X = llvm::dyn_cast<FieldAccessExpr>(E)) {
    discoverInExpr(X->getBase());
  }

  if (auto *X = llvm::dyn_cast<MatchExpr>(E)) {
    discoverInExpr(X->getScrutinee());
    for (auto &Arm : X->getArms()) {
      discoverInBlock(Arm.Body.get());
    }
  }

  if (auto *X = llvm::dyn_cast<TupleIndex>(E)) {
    discoverInExpr(X->getBase());
    discoverInExpr(X->getIndex());
  }

  if (auto *X = llvm::dyn_cast<TupleLiteral>(E)) {
    for (auto &Elem : X->getElements()) {
      discoverInExpr(Elem.get());
    }
  }

  if (auto *X = llvm::dyn_cast<ArrayIndex>(E)) {
    discoverInExpr(X->getBase());
    discoverInExpr(X->getIndex());
  }

  if (auto *X = llvm::dyn_cast<ArrayLiteral>(E)) {
    for (auto &Elem : X->getElements()) {
      discoverInExpr(Elem.get());
    }
  }
}

void CodeGen::recordInstantiation(const NamedDecl *Decl,
                                  const std::vector<TypeRef> &TypeArgs) {
  assert(Decl);

  // Items and Methods can have instantiations
  bool HasTypeArgs = false;
  if (auto *Item = llvm::dyn_cast<ItemDecl>(Decl)) {
    HasTypeArgs = Item->hasTypeArgs();
  } else if (auto *Method = llvm::dyn_cast<MethodDecl>(Decl)) {
    HasTypeArgs = Method->hasTypeArgs();
  }

  // Return if there are none
  if (!HasTypeArgs)
    return;

  // Do not record instantiation if any type argument depends on a generic type
  // parameter.
  for (const auto &Arg : TypeArgs) {
    if (hasGenericType(Arg))
      return;
  }

  TypeInstantiation Inst{Decl, TypeArgs};
  if (!Instantiations.contains(Inst)) {
    Instantiations.insert(Inst);
  }
}

void CodeGen::discoverInType(TypeRef T) {
  if (auto *App = llvm::dyn_cast<AppliedTy>(T.getPtr())) {
    for (const auto &Arg : App->getArgs()) {
      discoverInType(Arg);
    }

    assert(App->getBase().isAdt());
    auto *Adt = llvm::dyn_cast<AdtTy>(App->getBase().getPtr());

    assert(Adt->getDecl());
    recordInstantiation(Adt->getDecl(), App->getArgs());
  }

  if (auto *Ptr = llvm::dyn_cast<PtrTy>(T.getPtr())) {
    discoverInType(Ptr->getPointee());
  }

  if (auto *Ref = llvm::dyn_cast<RefTy>(T.getPtr())) {
    discoverInType(Ref->getPointee());
  }

  if (auto *Arr = llvm::dyn_cast<ArrayTy>(T.getPtr())) {
    discoverInType(Arr->getContainedTy());
  }

  if (auto *Tup = llvm::dyn_cast<TupleTy>(T.getPtr())) {
    for (const auto &Elem : Tup->getElementTys()) {
      discoverInType(Elem);
    }
  }
}
