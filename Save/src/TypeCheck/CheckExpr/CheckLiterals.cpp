#include "Sema/TypeChecker.hpp"

#include <cassert>

#include "AST/Nodes/Expr.hpp"
#include "AST/TypeSystem/Type.hpp"

using namespace phi;

bool TypeChecker::visit(Expr &E) { return E.accept(*this); }

bool TypeChecker::visit(IntLiteral &E) {
  assert(E.hasType());
  return E.getType().isInteger();
}

bool TypeChecker::visit(FloatLiteral &E) {
  assert(E.hasType());
  return E.getType().isFloat();
}

bool TypeChecker::visit(StrLiteral &E) {
  assert(E.hasType());
  assert(E.getType().isPrimitive());
  return E.getType().asPrimitive() == BuiltinKind::String;
}

bool TypeChecker::visit(CharLiteral &E) {
  assert(E.hasType());
  assert(E.getType().isPrimitive());
  return E.getType().asPrimitive() == BuiltinKind::Char;
}

bool TypeChecker::visit(BoolLiteral &E) {
  assert(E.hasType());
  assert(E.getType().isPrimitive());
  return E.getType().asPrimitive() == BuiltinKind::Bool;
}

bool TypeChecker::visit(RangeLiteral &E) {
  assert(E.hasType());
  assert(E.getType().isPrimitive());

  bool Success = visit(E.getStart());
  Success = visit(E.getEnd()) && Success;
  Success = (E.getStart().getType() == E.getEnd().getType()) && Success;
  Success = E.getType().asPrimitive() == BuiltinKind::Range && Success;
  return Success;
}

bool TypeChecker::visit(TupleLiteral &E) {
  assert(E.hasType());

  bool Success = true;
  for (auto &Element : E.getElements()) {
    Success = visit(*Element) && Success;
  }
  Success = E.getType().isTuple() && Success;
  return Success;
}
