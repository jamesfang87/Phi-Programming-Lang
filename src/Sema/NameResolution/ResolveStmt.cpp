#include "AST/Expr.hpp"
#include "AST/Stmt.hpp"
#include "Sema/NameResolver.hpp"

#include <algorithm>
#include <optional>
#include <print>

#include "AST/Decl.hpp"
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
  return std::all_of(
      Block.getStmts().begin(), Block.getStmts().end(),
      [this](const auto &StmtPtr) { return StmtPtr->accept(*this); });
}

/**
 * Resolves a return statement.
 *
 * Validates:
 * - Void functions don't return values
 * - Non-void functions return correct type
 * - Return expression resolves successfully
 */
bool NameResolver::visit(ReturnStmt &Statement) {
  // Void function return
  if (!Statement.hasExpr()) {
    return true;
  }

  // Resolve return expression
  bool Success = Statement.getExpr().accept(*this);
  if (!Success)
    return false;
  return true;
}

/**
 * Resolves an if statement.
 *
 * Validates:
 * - cond is boolean type
 * - Then/else blocks resolve successfully
 */
bool NameResolver::visit(IfStmt &Statement) {
  // Resolve cond
  bool Success = Statement.getCond().accept(*this);
  if (!Success)
    return false;

  // Resolve then and else blocks
  if (!resolveBlock(Statement.getThen()))
    return false;
  if (Statement.hasElse() && !resolveBlock(Statement.getElse()))
    return false;

  return true;
}

/**
 * Resolves a while loop statement.
 *
 * Validates:
 * - cond is boolean type
 * - Loop body resolves successfully
 */
bool NameResolver::visit(WhileStmt &Statement) {
  bool Success = Statement.getCond().accept(*this);
  if (!Success)
    return false;

  // Resolve loop body
  if (!resolveBlock(Statement.getBody()))
    return false;

  return true;
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
bool NameResolver::visit(ForStmt &Statement) {
  // Resolve range expression
  bool Success = Statement.getRange().accept(*this);
  if (!Success)
    return false;

  // Create scope for loop variable
  SymbolTable::ScopeGuard BlockScope(SymbolTab);
  SymbolTab.insert(&Statement.getLoopVar());

  // Resolve loop body (scope already created)
  if (!resolveBlock(Statement.getBody(), true))
    return false;

  return true;
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
bool NameResolver::visit(DeclStmt &Statement) {
  VarDecl &Var = Statement.getDecl();

  // Resolve variable type
  if (Var.hasType() && !resolveType(Var.getType())) {
    return false;
  }

  // Resolve initializer if present
  if (Var.hasInit() && !Var.getInit().accept(*this)) {
    std::println("failed to resolve variable initializer");
    return false;
  }

  // Add to symbol table
  SymbolTab.insert(&Var);

  return true;
}

bool NameResolver::visit(BreakStmt &Statement) {
  (void)Statement;
  return true;
}

bool NameResolver::visit(ContinueStmt &Statement) {
  (void)Statement;
  return true;
}

bool NameResolver::visit(Expr &Statement) { return Statement.accept(*this); }

} // namespace phi
