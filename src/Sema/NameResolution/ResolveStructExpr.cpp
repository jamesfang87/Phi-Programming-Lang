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
    std::println("Could not find struct {}", Expression.getStructId());
    return false;
  }
  Expression.setStructDecl(Found);

  std::unordered_set<std::string> AccountedFor;
  for (auto &Field : Found->getFields()) {
    if (Field.hasInit())
      continue;

    AccountedFor.insert(Field.getId());
  }

  for (auto &FieldInit : Expression.getFields()) {
    if (Found->getField(FieldInit->getFieldId()) == nullptr) {
      std::println("Could not find field {} in struct {}",
                   FieldInit->getFieldId(), Expression.getStructId());
      return false;
    }

    FieldInit->setFieldDecl(Found->getField(FieldInit->getFieldId()));
    assert(FieldInit->getDecl() != nullptr);

    if (!FieldInit->accept(*this)) {
      return false;
    }

    AccountedFor.erase(FieldInit->getFieldId());
  }

  if (!AccountedFor.empty()) {
    std::println("No matching construtor for struct {} found",
                 Expression.getStructId());
    return false;
  }

  return true;
}

bool NameResolver::visit(FieldInitExpr &Expression) {
  assert(Expression.getValue() != nullptr);
  assert(Expression.getDecl() != nullptr);

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
