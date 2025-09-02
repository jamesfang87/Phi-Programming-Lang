#include "Sema/NameResolver.hpp"

#include <cassert>
#include <format>
#include <string>
#include <unordered_set>

#include <llvm/Support/Casting.h>

#include "AST/Expr.hpp"
#include "Diagnostics/DiagnosticBuilder.hpp"

namespace phi {

bool NameResolver::visit(StructInitExpr &E) {
  auto *Found = SymbolTab.lookup(E.getStructId());
  if (!Found) {
    emitNotFoundError(NotFoundErrorKind::Struct, E.getStructId(),
                      E.getLocation());
    return false;
  }
  E.setStructDecl(Found);

  std::unordered_set<std::string> AccountedFor;
  for (auto &Field : Found->getFields()) {
    if (Field.hasInit())
      continue;

    AccountedFor.insert(Field.getId());
  }

  bool Success = true;
  for (auto &FieldInit : E.getFields()) {
    if (Found->getField(FieldInit->getFieldId()) == nullptr) {
      emitNotFoundError(NotFoundErrorKind::Field, FieldInit->getFieldId(),
                        FieldInit->getLocation());
    } else {
      FieldInit->setFieldDecl(Found->getField(FieldInit->getFieldId()));
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
  assert(E.getValue() != nullptr);

  return visit(*E.getValue());
}

bool NameResolver::visit(MemberAccessExpr &E) { return visit(*E.getBase()); }

bool NameResolver::visit(MemberFunCallExpr &E) {
  bool Success = visit(*E.getBase());

  for (const auto &Args : E.getCall().getArgs()) {
    Success = visit(*Args) && Success;
  }

  return Success;
}

} // namespace phi
