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

  // If it's an alloca (local variable/parameter), load the value using our
  // helper
  if (auto *Alloca = llvm::dyn_cast<llvm::AllocaInst>(Val)) {
    return load(Alloca, D->getType());
  }

  // For other values (like function references), return directly
  return Val;
}

llvm::Value *CodeGen::visit(FunCallExpr &E) {
  // Special case for println - use our built-in bridge
  if (auto *DR = llvm::dyn_cast<DeclRefExpr>(&E.getCallee())) {
    if (DR->getId() == "println") {
      return generatePrintlnBridge(E);
    }
  }

  std::vector<llvm::Value *> Args;
  Args.reserve(E.getArgs().size());
  for (auto &Arg : E.getArgs()) {
    llvm::Value *ArgVal = visit(*Arg);
    assert(ArgVal);
    Args.push_back(ArgVal);
  }

  // Get the callee function
  auto *CalleeRef = llvm::dyn_cast<DeclRefExpr>(&E.getCallee());
  assert(CalleeRef);

  llvm::Function *Fun = Module.getFunction(CalleeRef->getId());
  llvm::Value *CallResult = Builder.CreateCall(Fun, Args);

  // For void functions, return null; otherwise return the result
  return Fun->getReturnType()->isVoidTy() ? nullptr : CallResult;
}
