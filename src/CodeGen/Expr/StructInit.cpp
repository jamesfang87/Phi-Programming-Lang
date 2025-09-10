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
    const FieldDecl *FD = FI.getDecl();
    unsigned Idx = FieldIndexMap.at(FD);

    llvm::Value *FieldPtr =
        Builder.CreateStructGEP(ST, Alloca, Idx, FD->getId());
    llvm::Value *Val = FI.getValue()->accept(*this);
    llvm::Type *FieldTy = FD->getType().toLLVM(Context);

    if (FieldTy->isAggregateType()) {
      if (Val->getType()->isPointerTy()) {
        // Val is pointer to an aggregate (e.g. an alloca)
        llvm::Value *Loaded = Builder.CreateLoad(FieldTy, Val, "agg.load");
        Builder.CreateStore(Loaded, FieldPtr);
      } else {
        // Val is already an aggregate value
        Builder.CreateStore(Val, FieldPtr);
      }
      continue;
    }

    // Non-aggregate fields
    if (Val->getType()->isPointerTy() && !FieldTy->isPointerTy()) {
      llvm::Value *Loaded = Builder.CreateLoad(FieldTy, Val, "scalar.load");
      Builder.CreateStore(Loaded, FieldPtr);
    } else if (FieldTy->isPointerTy() && Val->getType()->isPointerTy()) {
      llvm::PointerType *FieldPtrTy = llvm::PointerType::getUnqual(FieldTy);
      if (Val->getType() != FieldPtrTy) {
        Val = Builder.CreateBitCast(Val, FieldTy);
      }
      Builder.CreateStore(Val, FieldPtr);
    } else {
      Builder.CreateStore(Val, FieldPtr);
    }
  }

  return Alloca;
}

llvm::Value *CodeGen::visit(FieldInitExpr &E) {
  return E.getValue()->accept(*this);
}
