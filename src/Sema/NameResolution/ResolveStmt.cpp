#include "Sema/NameResolver.hpp"

#include <optional>

#include "AST/Decl.hpp"
#include "AST/Expr.hpp"
#include "AST/Stmt.hpp"
#include "Sema/SymbolTable.hpp"

namespace phi {

bool NameResolver::visit(Block &Block, bool ScopeCreated = false) {
  // Create new scope unless parent already created one
  std::optional<SymbolTable::ScopeGuard> BlockScope;
  if (!ScopeCreated) {
    BlockScope.emplace(SymbolTab);
  }

  // Resolve all statements in the block
  bool Success = true;
  for (const auto &StmtPtr : Block.getStmts()) {
    Success = visit(*StmtPtr) && Success;
  }
  return Success;
}

bool NameResolver::visit(Stmt &S) { return S.accept(*this); }

bool NameResolver::visit(ReturnStmt &S) {
  // Void function return
  if (!S.hasExpr()) {
    return true;
  }

  // Resolve return expression
  return visit(S.getExpr());
}

bool NameResolver::visit(DeferStmt &S) { return visit(S.getDeferred()); }

bool NameResolver::visit(IfStmt &S) {
  bool Success = true;
  // Resolve cond
  Success = visit(S.getCond()) && Success;

  // Resolve then and else blocks
  Success = visit(S.getThen()) && Success;
  if (S.hasElse()) {
    Success = visit(S.getElse()) && Success;
  }

  return Success;
}

bool NameResolver::visit(WhileStmt &S) {
  bool Success = S.getCond().accept(*this);

  // Resolve loop body
  Success = visit(S.getBody()) && Success;
  return Success;
}

bool NameResolver::visit(ForStmt &S) {
  // Resolve range expression
  bool Success = S.getRange().accept(*this);

  // Create scope for loop variable
  SymbolTable::ScopeGuard BlockScope(SymbolTab);
  if (!SymbolTab.insert(&S.getLoopVar())) {
    emitRedefinitionError("variable", SymbolTab.lookup(S.getLoopVar()),
                          &S.getLoopVar());
    return false;
  }

  // Resolve loop body (scope already created)
  Success = visit(S.getBody(), true) && Success;
  return Success;
}

bool NameResolver::visit(DeclStmt &S) {
  VarDecl &Var = S.getDecl();
  bool Success = true;

  // Resolve variable type
  if (Var.hasType()) {
    Success = visit(Var.getType()) && Success;
  }

  // Resolve initializer if present
  if (Var.hasInit()) {
    Success = visit(Var.getInit()) && Success;
  }

  // Add to symbol table
  SymbolTab.insert(&Var);
  if (!SymbolTab.insert(&Var)) {
    emitRedefinitionError("variable", SymbolTab.lookup(Var), &Var);
    return false;
  }

  return Success;
}

bool NameResolver::visit(BreakStmt &S) {
  (void)S;
  return true;
}

bool NameResolver::visit(ContinueStmt &S) {
  (void)S;
  return true;
}

bool NameResolver::visit(ExprStmt &S) { return visit(S.getExpr()); }

} // namespace phi
