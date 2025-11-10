#include "Sema/TypeChecker.hpp"

#include <cassert>

#include "Diagnostics/DiagnosticBuilder.hpp"

using namespace phi;

bool TypeChecker::visit(CustomTypeCtor &E) {
  bool Success = true;
  for (auto &&FieldInit : E.getInits()) {
    Success = visit(*FieldInit) && Success;
  }
  return Success;
}

bool TypeChecker::visit(MemberInitExpr &E) {
  assert(E.getInitValue()->hasType());
  assert(E.getDecl());
  assert(E.getDecl()->hasType());

  bool Success = visit(*E.getInitValue());

  Type InitType = E.getInitValue()->getType();
  if (InitType != E.getDecl()->getType()) {
    error(std::format("Init of type `{}` cannot be assigned to field `{}`, "
                      "which has type `{}",
                      InitType.toString(), E.getDecl()->getId(),
                      E.getDecl()->getType().toString()))
        .with_code_snippet(
            E.getDecl()->getLocation(),
            std::format("For field `{}` declared here: ", E.getDecl()->getId()))
        .emit(*Diag);
    return false;
  }

  return Success;
}
