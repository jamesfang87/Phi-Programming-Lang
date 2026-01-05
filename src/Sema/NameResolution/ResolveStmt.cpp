#include "Sema/NameResolution/NameResolver.hpp"

#include <optional>

#include <llvm/ADT/TypeSwitch.h>

#include "AST/Nodes/Decl.hpp"
#include "AST/Nodes/Expr.hpp"
#include "AST/Nodes/Stmt.hpp"
#include "Sema/NameResolution/SymbolTable.hpp"

namespace phi {

bool NameResolver::visit(Stmt &S) {
  return llvm::TypeSwitch<Stmt *, bool>(&S)
      .Case<ReturnStmt>([&](ReturnStmt *X) { return visit(*X); })
      .Case<DeferStmt>([&](DeferStmt *X) { return visit(*X); })
      .Case<IfStmt>([&](IfStmt *X) { return visit(*X); })
      .Case<WhileStmt>([&](WhileStmt *X) { return visit(*X); })
      .Case<ForStmt>([&](ForStmt *X) { return visit(*X); })
      .Case<DeclStmt>([&](DeclStmt *X) { return visit(*X); })
      .Case<ContinueStmt>([&](ContinueStmt *X) { return visit(*X); })
      .Case<BreakStmt>([&](BreakStmt *X) { return visit(*X); })
      .Case<ExprStmt>([&](ExprStmt *X) { return visit(*X); })
      .Default([&](Stmt *) {
        std::unreachable();
        return false;
      });
}

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
  bool Success = visit(S.getCond());

  // Resolve loop body
  Success = visit(S.getBody()) && Success;
  return Success;
}

bool NameResolver::visit(ForStmt &S) {
  // Resolve range expression
  bool Success = visit(S.getRange());

  // Create scope for loop variable
  SymbolTable::ScopeGuard BlockScope(SymbolTab);
  if (!SymbolTab.insert(&S.getLoopVar())) {
    emitRedefinitionError("variable", SymbolTab.lookup(S.getLoopVar()),
                          &S.getLoopVar());
    Success = false;
  }

  // Resolve loop body (scope already created)
  Success = visit(S.getBody(), true) && Success;
  return Success;
}

bool NameResolver::visit(DeclStmt &S) {
  VarDecl &Var = S.getDecl();
  bool Success = true;

  // Resolve initializer if present
  if (Var.hasInit()) {
    Success = visit(Var.getInit()) && Success;
  }

  if (Var.hasType()) {
    Success = visit(Var.getType()) && Success;
  }

  // Add to symbol table
  if (!SymbolTab.insert(&Var)) {
    emitRedefinitionError("variable", SymbolTab.lookup(Var), &Var);
    Success = false;
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
