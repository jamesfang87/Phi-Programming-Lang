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
bool Sema::resolveBlock(Block &Block, bool ScopeCreated = false) {
  // Create new scope unless parent already created one
  std::optional<SymbolTable::ScopeGuard> BlockScope;
  if (!ScopeCreated) {
    BlockScope.emplace(SymbolTab);
  }

  // Resolve all statements in the block
  for (const auto &StmtPtr : Block.getStmts()) {
    if (!StmtPtr->accept(*this))
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
bool Sema::visit(ReturnStmt &Statement) {
  // Void function return
  if (!Statement.hasExpr()) {
    if (CurFun->getReturnTy() != Type(Type::PrimitiveKind::NullKind)) {
      std::println("error: function '{}' should return a value",
                   CurFun->getId());
      return false;
    }
    return true;
  }

  // Resolve return expression
  bool Success = Statement.getExpr().accept(*this);
  if (!Success)
    return false;

  // Validate return type matches function signature
  if (Statement.getExpr().getType() != CurFun->getReturnTy()) {
    std::println("type mismatch error: {}", CurFun->getId());
    std::println("return stmt type: {}",
                 Statement.getExpr().getType().toString());
    std::println("expected type: {}", CurFun->getReturnTy().toString());
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
bool Sema::visit(IfStmt &Statement) {
  // Resolve cond
  bool Success = Statement.getCond().accept(*this);
  if (!Success)
    return false;

  // Validate cond is boolean
  if (Statement.getCond().getType() != Type(Type::PrimitiveKind::BoolKind)) {
    std::println("error: cond in if statement must have type bool");
    return false;
  }

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
bool Sema::visit(WhileStmt &Statement) {
  LoopContextRAII LoopContext(LoopDepth);

  bool Success = Statement.getCond().accept(*this);
  if (!Success)
    return false;

  // Validate cond is boolean
  if (Statement.getCond().getType() != Type(Type::PrimitiveKind::BoolKind)) {
    std::println("error: cond in while statement must have type bool");
    return false;
  }

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
bool Sema::visit(ForStmt &Statement) {
  LoopContextRAII LoopContext(LoopDepth);
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
bool Sema::visit(DeclStmt &Statement) {
  VarDecl &Var = Statement.getDecl();

  // Resolve variable type
  const bool Success = resolveTy(Var.getOptionalType());
  if (!Success) {
    std::println("invalid type for variable");
    return false;
  }

  // Handle initializer if present
  if (Var.hasInit()) {
    Expr &Init = Var.getInit();
    if (!Init.accept(*this)) {
      std::println("failed to resolve variable initializer");
      return false;
    }

    // Check type compatibility
    auto VarType = Var.getOptionalType();
    if (!VarType.has_value()) {
      std::println("variable '{}' has unresolved type", Var.getId());
      return false;
    }
    if (Init.getType() != VarType.value()) {
      std::println("variable initializer type mismatch");
      std::println("variable type: {}", VarType.value().toString());
      std::println("initializer type: {}", Init.getType().toString());
      return false;
    }
  } else if (Var.isConst()) {
    // Constants require initializers
    std::println("constant variable '{}' must have an initializer",
                 Var.getId());
    return false;
  }

  // Add to symbol table
  SymbolTab.insert(&Var);

  return true;
}

bool Sema::visit(BreakStmt &Statement) {
  (void)Statement;
  return LoopDepth > 0;
}

bool Sema::visit(ContinueStmt &Statement) {
  (void)Statement;
  return LoopDepth > 0;
}

bool Sema::visit(Expr &Statement) { return Statement.accept(*this); }

} // namespace phi
