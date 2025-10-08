#include "CodeGen/CodeGen.hpp"

#include <cassert>
#include <llvm/Support/Casting.h>

#include "AST/Expr.hpp"

using namespace phi;

llvm::Value *CodeGen::visit(DeclRefExpr &E) {
  Decl *D = E.getDecl();

  // Look up the declaration in our map
  auto It = DeclMap.find(D);
  if (It == DeclMap.end())
    return nullptr;

  llvm::Value *Val = It->second;

  return Val;
}

llvm::Value *CodeGen::visit(FunCallExpr &E) {
  // Special case for println - use our built-in bridge
  if (auto *DeclRef = llvm::dyn_cast<DeclRefExpr>(&E.getCallee())) {
    if (DeclRef->getId() == "println") {
      return generatePrintlnBridge(E);
    }
  }

  // Get the callee function
  auto *Called = llvm::dyn_cast<DeclRefExpr>(&E.getCallee());
  assert(Called);
  llvm::Function *Fun = Module.getFunction(Called->getId());
  assert(Fun && "Called function not found");

  // Prepare args taking the callee's parameter types into account.
  std::vector<llvm::Value *> Args;
  Args.reserve(E.getArgs().size());

  unsigned ArgIndex = 0;
  for (auto &Arg : E.getArgs()) {
    llvm::Value *Raw = visit(*Arg);
    assert(Raw && "argument visit returned null");

    // If callee expects a pointer for this param, pass the pointer.
    // Otherwise load the value (primitive or aggregate) and pass it.
    llvm::Type *ParamTy = nullptr;
    if (ArgIndex < Fun->getFunctionType()->getNumParams()) {
      ParamTy = Fun->getFunctionType()->getParamType(ArgIndex);
    }

    llvm::Value *ArgToPass = nullptr;
    if (ParamTy && ParamTy->isPointerTy()) {
      ArgToPass = Raw; // pass pointer
    } else {
      // pass value: for primitives this loads a scalar; for structs this
      // loads the whole aggregate value
      if (Arg->getType().isCustom()) {
        // Raw should be a pointer to the alloca/GEP to the struct: load
        // aggregate
        ArgToPass = Builder.CreateLoad(Arg->getType().toLLVM(Context), Raw);
      } else {
        ArgToPass = load(Raw, Arg->getType());
      }
    }

    assert(ArgToPass && "Could not form call argument");
    Args.push_back(ArgToPass);
    ++ArgIndex;
  }
  llvm::Value *CallResult = Builder.CreateCall(Fun, Args);

  // For void functions, return null; otherwise return the result
  return Fun->getReturnType()->isVoidTy() ? nullptr : CallResult;
}
