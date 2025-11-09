#include "CodeGen/CodeGen.hpp"

#include "llvm/IR/DerivedTypes.h"
#include "llvm/Support/Casting.h"
#include <cassert>

using namespace phi;

void CodeGen::declareHeader(FunDecl &D) {
  // Create function type
  auto *FunType =
      llvm::dyn_cast<llvm::FunctionType>(D.getFunType().toLLVM(Context));

  llvm::Function::Create(FunType, llvm::Function::ExternalLinkage, D.getId(),
                         Module);
}

void CodeGen::visit(FunDecl &D) {
  auto *Fun = Module.getFunction(D.getId());

  // Create the entry block and push a placeholder value that
  // does nothing so that stack variables (params) can be added after
  auto *EntryBB = llvm::BasicBlock::Create(Context, "entry", Fun);
  Builder.SetInsertPoint(EntryBB);

  llvm::Value *Placeholder = llvm::UndefValue::get(Builder.getInt32Ty());
  AllocaInsertPoint = new llvm::BitCastInst(Placeholder, Placeholder->getType(),
                                            "alloca.placeholder", EntryBB);
  // allocate and store parameters
  auto ArgIt = Fun->arg_begin();
  for (auto &P : D.getParams()) {
    assert(ArgIt != Fun->arg_end() && "function param/arg mismatch");

    if (P->getType().toLLVM(Context)->isPointerTy()) {
      llvm::Argument *A = &*ArgIt;
      DeclMap[P.get()] = A;
    } else {
      auto *Alloca = stackAlloca(*P);
      DeclMap[P.get()] = Alloca;
      Builder.CreateStore(ArgIt, Alloca);
    }
    ++ArgIt;
  }

  // Set current function for statement generation
  CurrentFun = Fun;

  // emit body statements
  visit(D.getBody());

  // Handle function exit with deferred statements
  if (Builder.GetInsertBlock() && !Builder.GetInsertBlock()->getTerminator()) {
    // Execute all deferred statements before function exit
    executeDefers();

    // Create the appropriate return instruction
    if (D.getReturnTy().isNull()) {
      Builder.CreateRetVoid();
    } else {
      // This should not happen for well-formed code since non-void functions
      // must have explicit returns, but we handle it for robustness
      Builder.CreateRet(llvm::UndefValue::get(D.getReturnTy().toLLVM(Context)));
    }
  }

  // Clear deferred statements and clean up
  clearDefers();
  AllocaInsertPoint->eraseFromParent();
  AllocaInsertPoint = nullptr;
  CurrentFun = nullptr;
}
