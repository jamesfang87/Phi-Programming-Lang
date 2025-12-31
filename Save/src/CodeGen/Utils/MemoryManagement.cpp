#include "CodeGen/CodeGen.hpp"

namespace phi {

llvm::AllocaInst *CodeGen::stackAlloca(Decl &D) {
  std::string_view Id = D.getId();
  Type T = D.getType();

  llvm::IRBuilder<> TempBuilder(Context);
  TempBuilder.SetInsertPoint(AllocaInsertPoint);

  return TempBuilder.CreateAlloca(T.toLLVM(Context), nullptr, Id);
}

llvm::AllocaInst *CodeGen::stackAlloca(std::string_view Id, const Type &T) {
  llvm::IRBuilder<> TempBuilder(Context);
  TempBuilder.SetInsertPoint(AllocaInsertPoint);

  return TempBuilder.CreateAlloca(T.toLLVM(Context), nullptr, Id);
}

llvm::Value *CodeGen::store(llvm::Value *Val, llvm::Value *Destination,
                            const Type &T) {
  // Non-aggregate: simple store
  if (!T.isStruct())
    return Builder.CreateStore(Val, Destination);

  // Aggregate (struct) store:
  // - If Val is an aggregate value, store it directly.
  // - If Val is a pointer to an aggregate, copy field-by-field.
  llvm::Type *LLTy = T.toLLVM(Context);
  if (!Val->getType()->isPointerTy()) {
    return Builder.CreateStore(Val, Destination);
  }

  // Pointer -> memory copy per-field (avoid memcpy to keep element types exact)
  auto *StructTy = static_cast<llvm::StructType *>(LLTy);
  unsigned NumFields = StructTy->getNumElements();
  for (unsigned i = 0; i < NumFields; ++i) {
    llvm::Value *DstGEP = Builder.CreateStructGEP(StructTy, Destination, i);
    llvm::Value *SrcGEP = Builder.CreateStructGEP(StructTy, Val, i);
    llvm::Type *ElemTy = StructTy->getElementType(i);
    llvm::Value *Loaded = Builder.CreateLoad(ElemTy, SrcGEP);
    Builder.CreateStore(Loaded, DstGEP);
  }
  return Destination;
}

llvm::Value *CodeGen::load(llvm::Value *Val, const Type &T) {
  // If this is already an SSA value, return as-is.
  if (!llvm::isa<llvm::AllocaInst>(Val) &&
      !llvm::isa<llvm::GetElementPtrInst>(Val))
    return Val;

  // Primitive/scalar types: standard load
  return Builder.CreateLoad(T.toLLVM(Context), Val);
}

} // namespace phi
