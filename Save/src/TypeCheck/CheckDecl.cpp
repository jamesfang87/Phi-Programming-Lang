#include "Sema/TypeChecker.hpp"

#include <cassert>
#include <format>

#include "AST/Nodes/Decl.hpp"
#include "AST/Nodes/Stmt.hpp"
#include "AST/TypeSystem/Type.hpp"
#include "Diagnostics/DiagnosticBuilder.hpp"

using namespace phi;

bool TypeChecker::visit(Decl &D) { return D.accept(*this); }

bool TypeChecker::visit(FunDecl &D) {
  CurrentFun = &D;
  bool Success = true;
  for (auto &Param : D.getParams()) {
    Success = visit(*Param) && Success;
  }

  return visit(D.getBody()) && Success;
}

bool TypeChecker::visit(ParamDecl &D) {
  // TODO: Fix in case later changes come
  return true;
}

bool TypeChecker::visit(StructDecl &D) {
  bool Success = true;
  for (auto &Field : D.getFields()) {
    Success = visit(Field) && Success;
  }

  for (auto &Method : D.getMethods()) {
    Success = visit(Method) && Success;
  }

  return Success;
}

bool TypeChecker::visit(FieldDecl &D) {
  if (D.hasInit()) {
    bool Success = visit(D.getInit());

    Type InitType = D.getInit().getType();
    if (InitType != D.getType()) {
      error(
          std::format("Init of type `{}` cannot be assigned to variable `{}`, "
                      "which has type `{}",
                      InitType.toString(), D.getId(), D.getType().toString()))
          .with_primary_label(
              D.getLocation(),
              std::format("For field `{}` declared here ", D.getId()))
          .emit(*Diag);
      return false;
    }

    return Success;
  }

  return true;
}

bool TypeChecker::visit(MethodDecl &D) {
  CurrentFun = &D;
  bool Success = true;
  for (auto &Param : D.getParams()) {
    Success = visit(*Param) && Success;
  }

  return visit(D.getBody()) && Success;
}

bool TypeChecker::visit(VarDecl &D) {
  if (D.hasInit()) {
    bool Success = visit(D.getInit());

    Type InitType = D.getInit().getType();
    if (InitType != D.getType()) {
      error(
          std::format("Init of type `{}` cannot be assigned to variable `{}`, "
                      "which has type `{}",
                      InitType.toString(), D.getId(), D.getType().toString()))
          .with_primary_label(
              D.getLocation(),
              std::format("For variable `{}` declared here ", D.getId()))
          .emit(*Diag);
      return false;
    }

    return Success;
  }

  return true;
}

bool TypeChecker::visit(EnumDecl &D) {
  bool Success = true;
  for (auto &Method : D.getMethods()) {
    Success = visit(Method) && Success;
  }
  return Success;
}

bool TypeChecker::visit(VariantDecl &D) { return true; }
