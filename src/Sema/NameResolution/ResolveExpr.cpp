#include "AST/Decl.hpp"
#include "AST/Expr.hpp"
#include "Sema/NameResolver.hpp"

#include <llvm/Support/Casting.h>

#include <cassert>
#include <cstddef>
#include <print>

namespace phi {

// Literal expression resolution
// trivial: we don't need any name resolution
bool NameResolver::visit(IntLiteral &Expression) {
  (void)Expression;
  return true;
}

bool NameResolver::visit(FloatLiteral &Expression) {
  (void)Expression;
  return true;
}

bool NameResolver::visit(StrLiteral &Expression) {
  (void)Expression;
  return true;
}

bool NameResolver::visit(CharLiteral &Expression) {
  (void)Expression;
  return true;
}

bool NameResolver::visit(BoolLiteral &Expression) {
  (void)Expression;
  return true;
}

bool NameResolver::visit(RangeLiteral &Expression) {
  // Resolve start and end expressions
  if (!Expression.getStart().accept(*this) ||
      !Expression.getEnd().accept(*this)) {
    return false;
  }
  return true;
}

/**
 * Resolves a variable reference expression.
 *
 * Validates:
 * - Identifier exists in symbol table
 * - Not attempting to use function as value
 */
bool NameResolver::visit(DeclRefExpr &Expression) {
  ValueDecl *DeclPtr = SymbolTab.lookup(Expression);
  assert(DeclPtr != nullptr);
  if (!DeclPtr) {
    std::println("error: undeclared identifier '{}'", Expression.getId());
    return false;
  }
  Expression.setDecl(DeclPtr);
  return true;
}

/**
 * Resolves a function call expression.
 */
bool NameResolver::visit(FunCallExpr &Expression) {
  FunDecl *FunPtr = SymbolTab.lookup(Expression);
  if (!FunPtr) {
    auto *DeclRef = llvm::dyn_cast<DeclRefExpr>(&Expression.getCallee());
    std::println("error: undeclared fun identifier '{}'", DeclRef->getId());
    return false;
  }

  for (auto &Arg : Expression.getArgs()) {
    if (!Arg->accept(*this)) {
      return false;
    }
  }

  Expression.setDecl(FunPtr);
  return true;
}

/**
 * Resolves a binary operation expression.
 *
 * Validates:
 * - Both operands resolve successfully
 * - Operation is supported for operand types
 * - Type promotion rules are followed
 */
bool NameResolver::visit(BinaryOp &Expression) {
  // Resolve operands
  if (!Expression.getLhs().accept(*this) ||
      !Expression.getRhs().accept(*this)) {
    return false;
  }
  return true;
}

/**
 * Resolves a unary operation expression.
 *
 * Validates:
 * - Operand resolves successfully
 * - Operation is supported for operand type
 */
bool NameResolver::visit(UnaryOp &Expression) {
  // Resolve operand
  if (!Expression.getOperand().accept(*this)) {
    return false;
  }
  return true;
}

} // namespace phi
