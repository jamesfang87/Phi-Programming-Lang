#include "AST/Expr.hpp"
#include "AST/Stmt.hpp"
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
bool Sema::resolveBlock(Block &block, bool scopeCreated = false) {
  // Create new scope unless parent already created one
  std::optional<SymbolTable::ScopeGuard> blockScope;
  if (!scopeCreated) {
    blockScope.emplace(symbolTable);
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
  if (stmt.getExpr().getType() != curFun->getReturnTy()) {
    std::println("type mismatch error: {}", curFun->getID());
    std::println("return stmt type: {}", stmt.getExpr().getType().toString());
    std::println("expected type: {}", curFun->getReturnTy().toString());
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
  if (stmt.getCond().getType() != Type(Type::Primitive::boolean)) {
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
  LoopContextRAII LoopContext(LoopDepth);

  bool res = stmt.getCond().accept(*this);
  if (!res)
    return false;

  // Validate cond is boolean
  if (stmt.getCond().getType() != Type(Type::Primitive::boolean)) {
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
  LoopContextRAII LoopContext(LoopDepth);
  // Resolve range expression
  bool res = stmt.getRange().accept(*this);
  if (!res)
    return false;

  // Create scope for loop variable
  SymbolTable::ScopeGuard blockScope(symbolTable);
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
  const bool type = resolveTy(var.getType());
  if (!type) {
    std::println("invalid type for variable");
    return false;
  }

  // Handle initializer if present
  if (var.hasInit()) {
    Expr &init = var.getInit();
    if (!init.accept(*this)) {
      std::println("failed to resolve variable initializer");
      return false;
    }

    // Check type compatibility
    if (init.getType() != var.getType()) {
      std::println("variable initializer type mismatch");
      std::println("variable type: {}", var.getType().toString());
      std::println("initializer type: {}", init.getType().toString());
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

bool Sema::visit(BreakStmt &stmt) { return LoopDepth > 0; }
bool Sema::visit(ContinueStmt &stmt) { return LoopDepth > 0; }

bool Sema::visit(Expr &stmt) { return stmt.accept(*this); }

} // namespace phi
