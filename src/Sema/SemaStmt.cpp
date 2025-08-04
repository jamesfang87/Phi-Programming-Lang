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
bool Sema::resolve_block(Block &block, bool scope_created = false) {
  // Create new scope unless parent already created one
  std::optional<SymbolTable::ScopeGuard> block_scope;
  if (!scope_created) {
    block_scope.emplace(symbol_table);
  }

  // Resolve all statements in the block
  for (const auto &stmt : block.get_stmts()) {
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
  if (!stmt.has_expr()) {
    if (cur_fun->getReturnTy() != Type(Type::Primitive::null)) {
      std::println("error: function '{}' should return a value",
                   cur_fun->getID());
      return false;
    }
    return true;
  }

  // Resolve return expression
  bool success = stmt.get_expr().accept(*this);
  if (!success)
    return false;

  // Validate return type matches function signature
  if (stmt.get_expr().getTy() != cur_fun->getReturnTy()) {
    std::println("type mismatch error: {}", cur_fun->getID());
    std::println("return stmt type: {}", stmt.get_expr().getTy().to_string());
    std::println("expected type: {}", cur_fun->getReturnTy().to_string());
    return false;
  }

  return true;
}

/**
 * Resolves an if statement.
 *
 * Validates:
 * - Condition is boolean type
 * - Then/else blocks resolve successfully
 */
bool Sema::visit(IfStmt &stmt) {
  // Resolve condition
  bool res = stmt.get_condition().accept(*this);
  if (!res)
    return false;

  // Validate condition is boolean
  if (stmt.get_condition().getTy() != Type(Type::Primitive::boolean)) {
    std::println("error: condition in if statement must have type bool");
    return false;
  }

  // Resolve then and else blocks
  if (!resolve_block(stmt.get_then()))
    return false;
  if (stmt.has_else() && !resolve_block(stmt.get_else()))
    return false;

  return true;
}

/**
 * Resolves a while loop statement.
 *
 * Validates:
 * - Condition is boolean type
 * - Loop body resolves successfully
 */
bool Sema::visit(WhileStmt &stmt) {
  bool res = stmt.get_condition().accept(*this);
  if (!res)
    return false;

  // Validate condition is boolean
  if (stmt.get_condition().getTy() != Type(Type::Primitive::boolean)) {
    std::println("error: condition in while statement must have type bool");
    return false;
  }

  // Resolve loop body
  if (!resolve_block(stmt.get_body()))
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
  bool res = stmt.get_range().accept(*this);
  if (!res)
    return false;

  // Create scope for loop variable
  SymbolTable::ScopeGuard block_scope(symbol_table);
  symbol_table.insert(&stmt.get_loop_var());

  // Resolve loop body (scope already created)
  if (!resolve_block(stmt.get_body(), true))
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
  symbol_table.insert(&var);

  return true;
}

bool Sema::visit(Expr &stmt) { return stmt.accept(*this); }

} // namespace phi
