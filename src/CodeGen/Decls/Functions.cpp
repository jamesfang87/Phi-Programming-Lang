#include "CodeGen/CodeGen.hpp"

#include <cassert>
#include <llvm/IR/DerivedTypes.h>
#include <llvm/Support/Casting.h>

using namespace phi;

void CodeGen::declareHeader(FunDecl &D) {
  // Get parameter types
  std::vector<llvm::Type *> ParamTypes;
  for (const auto &Param : D.getParams()) {
    ParamTypes.push_back(Param->getType().toLLVM(Context));
  }

  // Get return type
  llvm::Type *RetType = D.getReturnTy().toLLVM(Context);

  // Create function type
  auto *FunType = llvm::FunctionType::get(RetType, ParamTypes, false);

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
    auto *Alloca = stackAlloca(*P);
    DeclMap[P.get()] = Alloca;

    if (ArgIt != Fun->arg_end()) {
      Builder.CreateStore(ArgIt, Alloca);
      ++ArgIt;
    }
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
    if (D.getReturnTy().isNullType()) {
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
