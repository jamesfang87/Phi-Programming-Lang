#include "CodeGen/CodeGen.hpp"

void phi::CodeGen::visit(phi::DeclRefExpr &expr) {
  // Look up the declaration in our declarations map
  auto it = decls.find(expr.getDecl());
  if (it != decls.end()) {
    llvm::Value *val = it->second;

    // Check if this is a local variable (AllocaInst) that needs to be loaded
    if (auto *alloca = llvm::dyn_cast<llvm::AllocaInst>(val)) {
      // Load the value from the allocated memory
      curValue =
          builder.CreateLoad(alloca->getAllocatedType(), alloca, expr.getID());
    } else {
      // This is a function parameter or other value - use directly
      curValue = val;
    }
  } else {
    // Declaration not found - this might be a global or external symbol
    curValue = nullptr;
  }
}

void phi::CodeGen::visit(phi::FunCallExpr &expr) {
  auto *decl_ref = dynamic_cast<phi::DeclRefExpr *>(&expr.getCallee());

  // Temp linking to printf
  if (decl_ref->getID() == "println") {
    generatePrintlnCall(expr);
    return;
  }

  // Generate arguments
  std::vector<llvm::Value *> args;
  for (auto &arg : expr.getArgs()) {
    arg->accept(*this);
    args.push_back(curValue);
  }

  // Find the function to call
  llvm::Function *func = module.getFunction(decl_ref->getID());
  if (func) {
    curValue = builder.CreateCall(func, args);
  }
}
