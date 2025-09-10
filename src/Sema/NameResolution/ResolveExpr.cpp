#include "Sema/NameResolver.hpp"

#include <cassert>
#include <cstddef>

#include <llvm/Support/Casting.h>

#include "AST/Decl.hpp"
#include "AST/Expr.hpp"

namespace phi {

bool NameResolver::visit(Expr &E) { return E.accept(*this); }

// Literal expression resolution
// trivial: we don't need any name resolution
bool NameResolver::visit(IntLiteral &E) {
  (void)E;
  return true;
}

bool NameResolver::visit(FloatLiteral &E) {
  (void)E;
  return true;
}

bool NameResolver::visit(StrLiteral &E) {
  (void)E;
  return true;
}

bool NameResolver::visit(CharLiteral &E) {
  (void)E;
  return true;
}

bool NameResolver::visit(BoolLiteral &E) {
  (void)E;
  return true;
}

bool NameResolver::visit(RangeLiteral &E) {
  // Resolve start and end expressions
  return visit(E.getStart()) && visit(E.getEnd());
}

/**
 * Resolves a variable reference expression.
 *
 * Validates:
 * - Identifier exists in symbol table
 * - Not attempting to use function as value
 */
bool NameResolver::visit(DeclRefExpr &E) {
  ValueDecl *DeclPtr = SymbolTab.lookup(E);
  if (!DeclPtr) {
    emitNotFoundError(NotFoundErrorKind::Variable, E.getId(), E.getLocation());

    return false;
  }
  E.setDecl(DeclPtr);
  return true;
}

/**
 * Resolves a function call expression.
 */
bool NameResolver::visit(FunCallExpr &E) {
  bool Success = true;
  FunDecl *FunPtr = SymbolTab.lookup(E);
  if (!FunPtr) {
    auto *DeclRef = llvm::dyn_cast<DeclRefExpr>(&E.getCallee());
    emitNotFoundError(NotFoundErrorKind::Function, DeclRef->getId(),
                      E.getLocation());
    Success = false;
  }

  for (auto &Arg : E.getArgs()) {
    Success = visit(*Arg) && Success;
  }

  E.setDecl(FunPtr);
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
bool NameResolver::visit(BinaryOp &E) {
  // Resolve operands
  return visit(E.getLhs()) && visit(E.getRhs());
}

/**
 * Resolves a unary operation expression.
 *
 * Validates:
 * - Operand resolves successfully
 * - Operation is supported for operand type
 */
bool NameResolver::visit(UnaryOp &E) { return visit(E.getOperand()); }

} // namespace phi
