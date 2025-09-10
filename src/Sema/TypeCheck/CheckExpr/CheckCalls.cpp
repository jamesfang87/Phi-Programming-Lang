#include "Sema/TypeChecker.hpp"

#include <cassert>
#include <cstddef>

using namespace phi;

bool TypeChecker::visit(DeclRefExpr &E) {
  assert(E.hasType());
  assert(E.getDecl());
  assert(E.getDecl()->hasType());

  return E.getType() == E.getDecl()->getType();
}

bool TypeChecker::visit(FunCallExpr &E) {
  assert(E.getDecl());

  const auto &Args = E.getArgs();
  const auto &Params = E.getDecl()->getParams();

  bool Success = Args.size() == Params.size();
  for (size_t I = 0; I < Args.size(); I++) {
    assert(Args[I]->hasType());
    assert(Params[I]->hasType());

    Success = visit(*Args[I]) && Success;
    Success = (Args[I]->getType() == Params[I]->getType()) && Success;
  }

  return Success;
}

bool TypeChecker::visit(FieldAccessExpr &E) {
  assert(E.hasType());
  assert(E.getField());
  assert(E.getField()->hasType());

  return E.getType() == E.getField()->getType();
}

bool TypeChecker::visit(MethodCallExpr &E) {
  // TODO: Finish
  return true;
}
