#include "CodeGen/CodeGen.hpp"

#include <llvm/Support/Casting.h>

#include "AST/Expr.hpp"

using namespace phi;

llvm::Value *CodeGen::visit(DeclRefExpr &E) {
  Decl *D = E.getDecl();

  // Check if this is a field access within a method
  if (auto *Field = llvm::dyn_cast<FieldDecl>(D)) {
    // Find the self parameter (should be mapped in DeclMap)
    auto It = DeclMap.find(D);
    if (It != DeclMap.end()) {
      llvm::Value *SelfPtr = It->second;
      // Generate member access through self
      llvm::Value *FieldPtr = computeMemberPointer(SelfPtr, Field);
      llvm::Type *FieldType = Field->getType().toLLVM(Context);
      return Builder.CreateLoad(FieldType, FieldPtr, E.getId());
    }
  }

  // Normal variable access
  auto It = DeclMap.find(D);
  if (It == DeclMap.end())
    return nullptr;
  llvm::Value *Val = It->second;
  if (auto *Alloca = llvm::dyn_cast<llvm::AllocaInst>(Val))
    return Builder.CreateLoad(Alloca->getAllocatedType(), Alloca, E.getId());
  return Val;
}

llvm::Value *CodeGen::visit(FunCallExpr &E) {
  if (auto *DR = llvm::dyn_cast<DeclRefExpr>(&E.getCallee()))
    if (DR->getId() == "println") {
      // println bridge returns the call value (or nullptr)
      return generatePrintlnBridge(E);
    }

  std::vector<llvm::Value *> Args;
  for (auto &A : E.getArgs()) {
    llvm::Value *V = A->accept(*this);
    if (!V)
      V = llvm::Constant::getNullValue(Builder.getInt64Ty());
    Args.push_back(V);
  }

  auto *CalleeRef = llvm::dyn_cast<DeclRefExpr>(&E.getCallee());
  if (!CalleeRef) {
    throw std::runtime_error("unsupported callee type");
  }
  std::string CalleeName = CalleeRef->getId();
  llvm::Function *Fn = Module.getFunction(CalleeName);
  if (!Fn) {
    // callee is expression that computes a pointer
    llvm::Value *FnPtr = E.getCallee().accept(*this);
    if (!FnPtr)
      return nullptr;
    // For function pointers, we need the function type
    // Get the function declaration to extract the type
    FunDecl *FunD = E.getDecl();
    if (!FunD) {
      throw std::runtime_error("function call without resolved declaration");
    }
    // Construct function type from return type and parameter types
    llvm::Type *RetType = FunD->getReturnTy().toLLVM(Context);
    std::vector<llvm::Type *> ParamTypes;
    for (const auto &Param : FunD->getParams()) {
      ParamTypes.push_back(Param->getType().toLLVM(Context));
    }
    llvm::FunctionType *FnType =
        llvm::FunctionType::get(RetType, ParamTypes, false);
    return Builder.CreateCall(FnType, FnPtr, Args);
  }
  return Builder.CreateCall(Fn, Args);
}
