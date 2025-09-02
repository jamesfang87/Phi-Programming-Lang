#include "CodeGen/CodeGen.hpp"

void phi::CodeGen::visit(phi::DeclRefExpr &E) {
  // Look up the declaration in our declarations map
  auto It = Decls.find(E.getDecl());
  if (It == Decls.end()) {
    CurValue = nullptr;
  }

  llvm::Value *Val = It->second;
  // Check if this is a local variable (AllocaInst) that needs to be loaded
  if (auto *Alloca = llvm::dyn_cast<llvm::AllocaInst>(Val)) {
    // Load the value from the allocated memory
    CurValue =
        Builder.CreateLoad(Alloca->getAllocatedType(), Alloca, E.getId());
  } else {
    // This is a function parameter or other value - use directly
    CurValue = Val;
  }
}

void phi::CodeGen::visit(phi::FunCallExpr &E) {
  auto DeclRef = llvm::dyn_cast<phi::DeclRefExpr>(&E.getCallee());

  // Temp linking to printf
  if (DeclRef->getId() == "println") {
    generatePrintlnCall(E);
    return;
  }

  // Generate arguments
  std::vector<llvm::Value *> Args;
  for (auto &Arg : E.getArgs()) {
    Arg->accept(*this);
    Args.push_back(CurValue);
  }

  // Find the function to call
  llvm::Function *Fun = Module.getFunction(DeclRef->getId());
  if (Fun) {
    CurValue = Builder.CreateCall(Fun, Args);
  }
}
