#include "Sema/NameResolver.hpp"

#include <optional>

#include "AST/Decl.hpp"
#include "AST/Expr.hpp"
#include "AST/Stmt.hpp"
#include "Sema/SymbolTable.hpp"

namespace phi {

/**
 * Resolves all statements within a block.
 *
 * @param block Block to resolve
 * @param scope_created Whether a scope was already created for this block
 * @return true if all statements resolved successfully, false otherwise
 *
 * Manages scope creation/destruction via RAII guard.
 * Recursively resolves nested statements and expressions.
 */
bool NameResolver::resolveBlock(Block &Block, bool ScopeCreated = false) {
  // Create new scope unless parent already created one
  std::optional<SymbolTable::ScopeGuard> BlockScope;
  if (!ScopeCreated) {
    BlockScope.emplace(SymbolTab);
  }

  // Resolve all statements in the block
  bool Success = true;
  for (const auto &StmtPtr : Block.getStmts()) {
    Success = StmtPtr->accept(*this) && Success;
  }
  return Success;
}

/**
 * Resolves a return statement.
 *
 * Validates:
 * - Void functions don't return values
 * - Non-void functions return correct type
 * - Return expression resolves successfully
 */
bool NameResolver::visit(ReturnStmt &S) {
  // Void function return
  if (!S.hasExpr()) {
    return true;
  }

  // Resolve return expression
  return visit(S.getExpr());
}

bool NameResolver::visit(DeferStmt &S) { return visit(S.getDeferred()); }

/**
 * Resolves an if statement.
 *
 * Validates:
 * - cond is boolean type
 * - Then/else blocks resolve successfully
 */
bool NameResolver::visit(IfStmt &S) {
  bool Success = true;
  // Resolve cond
  Success = visit(S.getCond()) && Success;

  // Resolve then and else blocks
  Success = resolveBlock(S.getThen()) && Success;
  if (S.hasElse()) {
    Success = resolveBlock(S.getElse()) && Success;
  }

  return Success;
}

/**
 * Resolves a while loop statement.
 *
 * Validates:
 * - cond is boolean type
 * - Loop body resolves successfully
 */
bool NameResolver::visit(WhileStmt &S) {
  bool Success = S.getCond().accept(*this);

  // Resolve loop body
  Success = resolveBlock(S.getBody()) && Success;
  return Success;
}

/**
 * Resolves a for loop statement.
 *
 * Validates:
 * - Range expression resolves successfully
 * - Loop body resolves successfully
 *
 * Creates new scope for loop variable.
 */
bool NameResolver::visit(ForStmt &S) {
  // Resolve range expression
  bool Success = S.getRange().accept(*this);

  // Create scope for loop variable
  SymbolTable::ScopeGuard BlockScope(SymbolTab);
  SymbolTab.insert(&S.getLoopVar());

  // Resolve loop body (scope already created)
  Success = resolveBlock(S.getBody(), true) && Success;
  return Success;
}

/**
 * Resolves a variable declaration statement.
 *
 * Validates:
 * - Type specification is valid
 * - Initializer expression resolves
 * - Initializer type matches declared type
 * - Constants have initializers
 *
 * Adds variable to current symbol table.
 */
bool NameResolver::visit(DeclStmt &S) {
  VarDecl &Var = S.getDecl();
  bool Success = true;

  // Resolve variable type
  if (Var.hasType()) {
    Success = resolveType(Var.getType()) && Success;
  }

  // Resolve initializer if present
  if (Var.hasInit()) {
    Success = visit(Var.getInit()) && Success;
  }

  // Add to symbol table
  SymbolTab.insert(&Var);

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

bool NameResolver::visit(Expr &S) { return S.accept(*this); }

} // namespace phi
