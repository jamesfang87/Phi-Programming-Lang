#include "Sema/NameResolver.hpp"

#include <cassert>
#include <format>
#include <string>
#include <unordered_set>

#include <llvm/Support/Casting.h>

#include "AST/Expr.hpp"
#include "Diagnostics/DiagnosticBuilder.hpp"

namespace phi {

bool NameResolver::visit(StructLiteral &E) {
  auto *Found = SymbolTab.lookupStruct(E.getStructId());
  if (!Found) {
    emitNotFoundError(NotFoundErrorKind::Struct, E.getStructId(),
                      E.getLocation());
    return false;
  }
  E.setDecl(Found);

  std::unordered_set<std::string> AccountedFor;
  for (auto &Field : Found->getFields()) {
    if (Field->hasInit())
      continue;

    AccountedFor.insert(Field->getId());
  }

  bool Success = true;
  for (auto &FieldInit : E.getFields()) {
    if (Found->getField(FieldInit->getFieldId()) == nullptr) {
      emitNotFoundError(NotFoundErrorKind::Field, FieldInit->getFieldId(),
                        FieldInit->getLocation());
    } else {
      FieldInit->setDecl(Found->getField(FieldInit->getFieldId()));
      assert(FieldInit->getDecl() != nullptr);
      AccountedFor.erase(FieldInit->getFieldId());
    }

    Success = visit(*FieldInit) && Success;
  }

  if (!AccountedFor.empty()) {
    std::string Error =
        std::format("Struct `{}` is missing inits for fields ", Found->getId());
    for (const std::string &Field : AccountedFor) {
      Error += Field + ", ";
    }
    Error = Error.substr(0, Error.size() - 2); //  remove trailing comma
    emitError(error(Error)
                  .with_primary_label(E.getLocation(), "For this init")
                  .build());

    return false;
  }

  return Success;
}

bool NameResolver::visit(FieldInitExpr &E) {
  assert(E.getInitValue() != nullptr);

  return visit(*E.getInitValue());
}

bool NameResolver::visit(FieldAccessExpr &E) { return visit(*E.getBase()); }

bool NameResolver::visit(MethodCallExpr &E) {
  bool Success = visit(*E.getBase());

  for (const auto &Args : E.getArgs()) {
    Success = visit(*Args) && Success;
  }

  return Success;
}

} // namespace phi
