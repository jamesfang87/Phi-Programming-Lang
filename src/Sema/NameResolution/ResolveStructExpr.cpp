#include "AST/Expr.hpp"
#include "Sema/NameResolver.hpp"

#include <llvm/Support/Casting.h>

#include <cassert>
#include <print>
#include <string>
#include <unordered_set>

namespace phi {

bool NameResolver::visit(StructInitExpr &Expression) {
  auto *Found = SymbolTab.lookup(Expression.getStructId());
  if (!Found) {
    emitNotFoundError(NotFoundErrorKind::Struct, Expression.getStructId(),
                      Expression.getLocation());
    return false;
  }
  Expression.setStructDecl(Found);

  std::unordered_set<std::string> AccountedFor;
  for (auto &Field : Found->getFields()) {
    if (Field.hasInit())
      continue;

    AccountedFor.insert(Field.getId());
  }

  bool Success = true;
  for (auto &FieldInit : Expression.getFields()) {
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
    std::println("No matching construtor for struct {} found",
                 Expression.getStructId());
    return false;
  }

  return Success;
}

bool NameResolver::visit(FieldInitExpr &Expression) {
  assert(Expression.getValue() != nullptr);

  return visit(*Expression.getValue());
}

bool NameResolver::visit(MemberAccessExpr &Expression) {
  return visit(*Expression.getBase());
}

bool NameResolver::visit(MemberFunCallExpr &Expression) {
  bool Success = visit(*Expression.getBase());

  for (const auto &Args : Expression.getCall().getArgs()) {
    Success = visit(*Args) && Success;
  }

  return Success;
}

} // namespace phi
