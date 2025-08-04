#include "AST/Expr.hpp"
#include "Sema/Sema.hpp"

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
bool Sema::resolveBlock(Block &block, bool scope_created = false) {
  // Create new scope unless parent already created one
  std::optional<SymbolTable::ScopeGuard> block_scope;
  if (!scope_created) {
    block_scope.emplace(symbolTable);
  }

  // Resolve all statements in the block
  for (const auto &stmt : block.getStmts()) {
    if (!stmt->accept(*this))
      return false;
  }

  return true;
}

/**
 * Resolves a return statement.
 *
 * Validates:
 * - Void functions don't return values
 * - Non-void functions return correct type
 * - Return expression resolves successfully
 */
bool Sema::visit(ReturnStmt &stmt) {
  // Void function return
  if (!stmt.hasExpr()) {
    if (curFun->getReturnTy() != Type(Type::Primitive::null)) {
      std::println("error: function '{}' should return a value",
                   curFun->getID());
      return false;
    }
    return true;
  }

  // Resolve return expression
  bool success = stmt.getExpr().accept(*this);
  if (!success)
    return false;

  // Validate return type matches function signature
  if (stmt.getExpr().getTy() != curFun->getReturnTy()) {
    std::println("type mismatch error: {}", curFun->getID());
    std::println("return stmt type: {}", stmt.getExpr().getTy().to_string());
    std::println("expected type: {}", curFun->getReturnTy().to_string());
    return false;
  }

  return true;
}

/**
 * Resolves an if statement.
 *
 * Validates:
 * - cond is boolean type
 * - Then/else blocks resolve successfully
 */
bool Sema::visit(IfStmt &stmt) {
  // Resolve cond
  bool res = stmt.getCond().accept(*this);
  if (!res)
    return false;

  // Validate cond is boolean
  if (stmt.getCond().getTy() != Type(Type::Primitive::boolean)) {
    std::println("error: cond in if statement must have type bool");
    return false;
  }

  // Resolve then and else blocks
  if (!resolveBlock(stmt.getThen()))
    return false;
  if (stmt.hasElse() && !resolveBlock(stmt.getElse()))
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
bool Sema::visit(WhileStmt &stmt) {
  bool res = stmt.getCond().accept(*this);
  if (!res)
    return false;

  // Validate cond is boolean
  if (stmt.getCond().getTy() != Type(Type::Primitive::boolean)) {
    std::println("error: cond in while statement must have type bool");
    return false;
  }

  // Resolve loop body
  if (!resolveBlock(stmt.getBody()))
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
bool Sema::visit(ForStmt &stmt) {
  // Resolve range expression
  bool res = stmt.getRange().accept(*this);
  if (!res)
    return false;

  // Create scope for loop variable
  SymbolTable::ScopeGuard block_scope(symbolTable);
  symbolTable.insert(&stmt.getLoopVar());

  // Resolve loop body (scope already created)
  if (!resolveBlock(stmt.getBody(), true))
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
bool Sema::visit(LetStmt &stmt) {
  VarDecl &var = stmt.getDecl();

  // Resolve variable type
  const bool type = resolveTy(var.getTy());
  if (!type) {
    std::println("invalid type for variable");
    return false;
  }

  // Handle initializer if present
  if (var.hasInit()) {
    Expr &initializer = var.getInit();
    if (!initializer.accept(*this)) {
      std::println("failed to resolve variable initializer");
      return false;
    }

    // Check type compatibility
    if (initializer.getTy() != var.getTy()) {
      std::println("variable initializer type mismatch");
      std::println("variable type: {}", var.getTy().to_string());
      std::println("initializer type: {}", initializer.getTy().to_string());
      return false;
    }
  } else if (var.isConst()) {
    // Constants require initializers
    std::println("constant variable '{}' must have an initializer",
                 var.getID());
    return false;
  }

  // Add to symbol table
  symbolTable.insert(&var);

  return true;
}

bool Sema::visit(Expr &stmt) { return stmt.accept(*this); }

} // namespace phi
