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
  if (!T.isStruct())
    return Builder.CreateStore(Val, Destination);

  // T.toLLVM(Context) should return the llvm::StructType* for the struct
  auto *StructTy = static_cast<llvm::StructType *>(T.toLLVM(Context));
  unsigned NumFields = StructTy->getNumElements();

  // We expect Val and Destination to be pointers to the struct (allocas).
  // For each field: load from src.field and store into dst.field.
  for (unsigned i = 0; i < NumFields; ++i) {
    // GEP to the i-th element for src and dst
    llvm::Value *DstGEP = Builder.CreateStructGEP(StructTy, Destination, i);
    llvm::Value *SrcGEP = Builder.CreateStructGEP(StructTy, Val, i);

    // Load the element from source and store into destination
    llvm::Type *ElemTy = StructTy->getElementType(i);
    llvm::Value *Loaded = Builder.CreateLoad(ElemTy, SrcGEP);
    Builder.CreateStore(Loaded, DstGEP);
  }

  // Return the destination pointer (consistent with your previous API)
  return Destination;
}

llvm::Value *CodeGen::load(llvm::Value *Val, const Type &T) {
  // If it's already a value (not an alloca), return it directly
  if (!llvm::isa<llvm::AllocaInst>(Val) &&
      !llvm::isa<llvm::GetElementPtrInst>(Val)) {
    return Val;
  }

  // For custom types (structs), we want the pointer, don't load the entire
  // struct
  if (T.isStruct()) {
    return Val;
  }

  // For primitive types, perform the actual load
  return Builder.CreateLoad(T.toLLVM(Context), Val);
}

} // namespace phi
