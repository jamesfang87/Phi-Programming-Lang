#include "CodeGen/CodeGen.hpp"
#include "llvm/IR/DerivedTypes.h"

#include <llvm/Support/Casting.h>

using namespace phi;

void CodeGen::visit(Decl &D) { D.accept(*this); }

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

  // Clean up alloca insert point
  AllocaInsertPoint->eraseFromParent();
  AllocaInsertPoint = nullptr;

  // Add return if the function doesn't already have a terminator
  if (Builder.GetInsertBlock() && !Builder.GetInsertBlock()->getTerminator()) {
    if (D.getReturnTy().isPrimitive() &&
        D.getReturnTy().asPrimitive() == PrimitiveKind::Null) {
      Builder.CreateRetVoid();
    } else {
      // For now, return a default value - this should be improved
      // to handle explicit returns properly
      llvm::Value *DefaultRet =
          llvm::Constant::getNullValue(D.getReturnTy().toLLVM(Context));
      Builder.CreateRet(DefaultRet);
    }
  }

  // Reset current function
  CurrentFun = nullptr;
}

void CodeGen::visit(ParamDecl &D) { (void)D; }

void CodeGen::declareHeader(StructDecl &D) {}
void CodeGen::visit(StructDecl &D) { (void)D; }
void CodeGen::visit(FieldDecl &D) { (void)D; }
void CodeGen::visit(VarDecl &D) { (void)D; }
