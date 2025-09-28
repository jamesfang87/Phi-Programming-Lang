#include "CodeGen/CodeGen.hpp"
#include "llvm/IR/DerivedTypes.h"
#include "llvm/Support/Casting.h"

using namespace phi;

void CodeGen::declareHeader(StructDecl &D) {
  llvm::StructType::create(Context, D.getId());

  for (auto &Method : D.getMethods()) {
    declareHeader(Method, D.getId() + "." + Method.getId());
  }
}

void CodeGen::visit(StructDecl &D) {
  auto *Type = static_cast<llvm::StructType *>(D.getType().toLLVM(Context));

  std::vector<llvm::Type *> FieldTypes;
  for (auto &&Field : D.getFields()) {
    llvm::Type *T = Field->getType().toLLVM(Context);
    FieldTypes.emplace_back(T);
  }
  Type->setBody(FieldTypes);

  for (auto &&Method : D.getMethods()) {
    visit(Method);
  }
}

void CodeGen::declareHeader(MethodDecl &D, const std::string &MangledName) {
  auto *FunType =
      llvm::dyn_cast<llvm::FunctionType>(D.getFunType().toLLVM(Context));
  D.setMangledId(MangledName);

  llvm::Function::Create(FunType, llvm::Function::ExternalLinkage, MangledName,
                         Module);
}

void CodeGen::visit(MethodDecl &D) {
  // Methods are handled the same way as functions
  // The mangled name should already be set from the declaration phase
  std::string MangledId =
      D.getMangledId(); // This should already be StructName.methodName

  auto *Fun = Module.getFunction(MangledId);
  if (!Fun) {
    throw std::runtime_error("Method function not found: " + MangledId);
  }

  // Create the entry block
  auto *EntryBB = llvm::BasicBlock::Create(Context, "entry", Fun);
  Builder.SetInsertPoint(EntryBB);

  llvm::Value *Placeholder = llvm::UndefValue::get(Builder.getInt32Ty());
  AllocaInsertPoint = new llvm::BitCastInst(Placeholder, Placeholder->getType(),
                                            "alloca.placeholder", EntryBB);

  // Allocate and store parameters
  //
  //
  auto ArgIt = Fun->arg_begin();
  for (auto &P : D.getParams()) {
    auto T = P->getType().toLLVM(Context);
    if (T->isPointerTy()) {
      assert(ArgIt != Fun->arg_end() && "function param/arg mismatch");
      llvm::Argument *A = &*ArgIt;
      // Map the parameter declaration to the argument value (no alloca)
      DeclMap[P.get()] = A;
    } else {
      auto *Alloca = stackAlloca(*P);
      DeclMap[P.get()] = Alloca;

      assert(ArgIt != Fun->arg_end());
      Builder.CreateStore(ArgIt, Alloca);
    }
    ++ArgIt;
  }

  // Set current function for statement generation
  CurrentFun = Fun;

  // Emit body statements
  visit(D.getBody());

  // Handle function exit with deferred statements
  if (Builder.GetInsertBlock() && !Builder.GetInsertBlock()->getTerminator()) {
    executeDefers();

    if (D.getReturnTy().isNullType()) {
      Builder.CreateRetVoid();
    } else {
      Builder.CreateRet(llvm::UndefValue::get(D.getReturnTy().toLLVM(Context)));
    }
  }

  // Clean up
  clearDefers();
  AllocaInsertPoint->eraseFromParent();
  AllocaInsertPoint = nullptr;
  CurrentFun = nullptr;
}
