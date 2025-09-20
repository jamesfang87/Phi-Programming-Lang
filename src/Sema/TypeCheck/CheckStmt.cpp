#include "Sema/TypeChecker.hpp"

#include <cassert>
#include <format>

#include "AST/Stmt.hpp"
#include "AST/Type.hpp"
#include "Diagnostics/DiagnosticBuilder.hpp"

using namespace phi;

bool TypeChecker::visit(Stmt &S) { return S.accept(*this); }

bool TypeChecker::visit(ReturnStmt &S) {
  assert(CurrentFun);

  auto RetType = CurrentFun->getReturnTy();
  bool Success = true;

  if (!S.hasExpr()) {
    // Function returns non-void, but no expression provided
    if (!RetType.isPrimitive() ||
        RetType.asPrimitive() != PrimitiveKind::Null) {
      error("Function with non-void return type must return a value")
          .with_primary_label(S.getLocation(),
                              std::format("Expected an expr of type `{}` here",
                                          RetType.toString()))
          .emit(*Diag);
      Success = false;
    }
    return Success;
  }

  // Visit the return expression
  Success = visit(S.getExpr()) && Success;

  Type ExprType = S.getExpr().getType();

  // Check type compatibility
  if (ExprType != RetType) {
    error("Return type mismatch")
        .with_primary_label(
            S.getExpr().getLocation(),
            std::format("Return expression has type `{}`", ExprType.toString()))
        .with_secondary_label(
            S.getLocation(),
            std::format("Function expects type `{}`", RetType.toString()))
        .emit(*Diag);
    Success = false;
  }

  return Success;
}

bool TypeChecker::visit(DeferStmt &S) {
  (void)S;
  return true;
}

bool TypeChecker::visit(IfStmt &S) {
  assert(S.getCond().hasType());

  visit(S.getCond());
  visit(S.getThen());
  if (S.hasElse()) {
    visit(S.getElse());
  }

  if (!S.getCond().getType().isPrimitive()) {
    return false;
  }

  return S.getCond().getType().asPrimitive() == PrimitiveKind::Bool;
}

bool TypeChecker::visit(WhileStmt &S) {
  assert(S.getCond().hasType());

  visit(S.getCond());
  visit(S.getBody());

  if (!S.getCond().getType().isPrimitive()) {
    return false;
  }

  return S.getCond().getType().asPrimitive() == PrimitiveKind::Bool;
}

bool TypeChecker::visit(ForStmt &S) {
  // TODO: Implement ConstExprEval to catch overflows at compile time
  return visit(S.getBody());
}

bool TypeChecker::visit(DeclStmt &S) { return visit(S.getDecl()); }

bool TypeChecker::visit(BreakStmt &S) {
  (void)S;
  return true;
}

bool TypeChecker::visit(ContinueStmt &S) {
  (void)S;
  return true;
}

bool TypeChecker::visit(ExprStmt &S) { return visit(S.getExpr()); }

bool TypeChecker::visit(Block &B) {
  // Resolve all statements in the block
  bool Success = true;
  for (const auto &StmtPtr : B.getStmts()) {
    Success = visit(*StmtPtr) && Success;
  }
  return Success;
}
