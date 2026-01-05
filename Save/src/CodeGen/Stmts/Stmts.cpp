#include "CodeGen/CodeGen.hpp"

#include <cassert>
#include <ranges>

using namespace phi;

void CodeGen::visit(Block &B) {
  for (auto &Stmt : B.getStmts()) {
    visit(*Stmt);
  }
}

void CodeGen::visit(Stmt &S) { S.accept(*this); }

void CodeGen::visit(DeclStmt &S) { visit(S.getDecl()); }

void CodeGen::visit(ExprStmt &S) { S.getExpr().accept(*this); }

void CodeGen::visit(ReturnStmt &S) {
  // Execute all deferred statements before returning
  executeDefers();

  if (S.hasExpr()) {
    // Return with value
    assert(visit(S.getExpr()));
    llvm::Value *RetVal = load(visit(S.getExpr()), S.getExpr().getType());
    Builder.CreateRet(RetVal);
  } else {
    // Void return
    Builder.CreateRetVoid();
  }

  // Note: Control flow ends here, so no need to set insert point
}

void CodeGen::visit(DeferStmt &S) {
  // Add the deferred expression to the defer stack
  // It will be executed in reverse order when the function returns
  DeferStack.emplace_back(S.getDeferred());
}

//===----------------------------------------------------------------------===//
// Defer Statement Management
//===----------------------------------------------------------------------===//

void CodeGen::executeDefers() {
  // Execute deferred statements in reverse order (LIFO)
  for (auto &Deferred : DeferStack | std::views::reverse) {
    load(visit(Deferred.get()), Deferred.get().getType());
  }
}

void CodeGen::clearDefers() { DeferStack.clear(); }
