#include "CodeGen/CodeGen.hpp"

#include <llvm/Support/Casting.h>

using namespace phi;

llvm::Value *CodeGen::visit(StructInitExpr &E) {
  StructDecl &SD = *E.getStructDecl();
  if (!StructTypeMap.count(&SD))
    createStructLayout(&SD);
  llvm::StructType *ST = StructTypeMap.at(&SD);
  llvm::AllocaInst *Alloca = Builder.CreateAlloca(ST, nullptr, "struct.tmp");
  for (auto &Fptr : E.getFields()) {
    FieldInitExpr &FI = *Fptr;
    auto FD = FI.getDecl();
    unsigned Idx = FieldIndexMap.at(FD);
    llvm::Value *Ptr = Builder.CreateStructGEP(ST, Alloca, Idx, FD->getId());
    llvm::Value *Val = FI.getValue()->accept(*this);
    Builder.CreateStore(Val, Ptr);
  }
  return Alloca;
}

llvm::Value *CodeGen::visit(FieldInitExpr &E) {
  return E.getValue()->accept(*this);
}
