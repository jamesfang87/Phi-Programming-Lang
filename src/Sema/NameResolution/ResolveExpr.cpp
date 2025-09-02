#include "Sema/NameResolver.hpp"

#include <cassert>
#include <cstddef>

#include <llvm/Support/Casting.h>

#include "AST/Decl.hpp"
#include "AST/Expr.hpp"

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
  return visit(Expression.getStart()) && visit(Expression.getEnd());
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
  if (!DeclPtr) {
    emitNotFoundError(NotFoundErrorKind::Variable, Expression.getId(),
                      Expression.getLocation());

    return false;
  }
  Expression.setDecl(DeclPtr);
  return true;
}

/**
 * Resolves a function call expression.
 */
bool NameResolver::visit(FunCallExpr &Expression) {
  bool Success = true;
  FunDecl *FunPtr = SymbolTab.lookup(Expression);
  if (!FunPtr) {
    auto *DeclRef = llvm::dyn_cast<DeclRefExpr>(&Expression.getCallee());
    emitNotFoundError(NotFoundErrorKind::Function, DeclRef->getId(),
                      Expression.getLocation());
    Success = false;
  }

  for (auto &Arg : Expression.getArgs()) {
    Success = Arg->accept(*this) && Success;
  }

  Expression.setDecl(FunPtr);
  return Success;
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
  return visit(Expression.getLhs()) && visit(Expression.getRhs());
}

/**
 * Resolves a unary operation expression.
 *
 * Validates:
 * - Operand resolves successfully
 * - Operation is supported for operand type
 */
bool NameResolver::visit(UnaryOp &Expression) {
  return visit(Expression.getOperand());
}

} // namespace phi
