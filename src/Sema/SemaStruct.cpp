#include "AST/Expr.hpp"
#include "Sema/Sema.hpp"

#include <llvm/Support/Casting.h>

#include <cassert>
#include <print>
#include <string>
#include <unordered_set>

namespace phi {

bool Sema::visit(StructInitExpr &Expression) {
  auto *StructPtr = SymbolTab.lookup(Expression.getStructId());
  if (!StructPtr) {
    std::println("Could not find struct {}", Expression.getStructId());
    return false;
  }
  Expression.setStructDecl(StructPtr);

  std::unordered_set<std::string> AccountedFor;
  for (auto &Field : StructPtr->getFields()) {
    std::println("Processing field {}", Field.getId());
    if (Field.hasInit() || Field.isConst())
      continue;

    AccountedFor.insert(Field.getId());
  }

  for (auto &FieldInit : Expression.getFields()) {
    if (StructPtr->getField(FieldInit->getFieldId()) == nullptr) {
      std::println("Could not find field {} in struct {}",
                   FieldInit->getFieldId(), Expression.getStructId());
      return false;
    }

    FieldInit->setFieldDecl(StructPtr->getField(FieldInit->getFieldId()));
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

  Expression.setType(StructPtr->getType());

  return true;
}

bool Sema::visit(FieldInitExpr &Expression) {
  assert(Expression.getValue() != nullptr);

  if (!Expression.getValue()->accept(*this)) {
    return false;
  }

  if (Expression.getValue()->getType() != Expression.getDecl()->getType()) {
    std::println("Type mismatch for field {}", Expression.getDecl()->getId());
    return false;
  }

  Expression.setType(Expression.getDecl()->getType());

  return true;
}

bool Sema::visit(MemberAccessExpr &Expression) {
  const auto Base = Expression.getBase();

  if (!Base->accept(*this)) {
    std::println("Base expression failed semantic analysis");
    return false;
  }

  StructDecl *Struct = SymbolTab.lookup(Base->getType().getCustomTypeName());
  if (Struct == nullptr) {
    std::println("Could not find struct {} in symbol table",
                 Base->getType().getCustomTypeName());
    return false;
  }

  if (Struct->getField(Expression.getMemberId()) == nullptr) {
    std::println("Could not find field {} in struct {}",
                 Expression.getMemberId(), Base->getType().getCustomTypeName());
    return false;
  }

  Expression.setType(Struct->getField(Expression.getMemberId())->getType());

  return true;
}

bool Sema::visit(MemberFunCallExpr &Expression) {
  const auto Base = llvm::dyn_cast<DeclRefExpr>(Expression.getBase());
  if (!Base) {
    std::println("this");
    return false;
  }

  if (!Base->accept(*this)) {
    std::println("Base expression failed semantic analysis");
    return false;
  }

  StructDecl *Struct =
      SymbolTab.lookup(Base->getDecl()->getType().getCustomTypeName());
  if (Struct == nullptr) {
    std::println("Could not find struct {} in symbol table",
                 Base->getDecl()->getId());
    return false;
  }

  auto DeclRef = llvm::dyn_cast<DeclRefExpr>(&Expression.getCall().getCallee());
  auto Fun = Struct->getMethod(DeclRef->getId());
  if (Fun == nullptr) {
    std::println("Could not find method {} in struct {}", DeclRef->getId(),
                 Base->getDecl()->getId());
    return false;
  }

  // Validate argument count
  if (Expression.getCall().getArgs().size() != Fun->getParams().size()) {
    std::println("error: parameter list length mismatch");
    return false;
  }

  // Resolve arguments and validate types
  for (size_t i = 0; i < Expression.getCall().getArgs().size(); i++) {
    Expr *Arg = Expression.getCall().getArgs()[i].get();
    if (!Arg->accept(*this)) {
      std::println("error: could not resolve argument");
      return false;
    }

    ParamDecl *Param = Fun->getParams().at(i).get();
    if (Arg->getType() != Param->getType()) {
      std::println("error: argument type mismatch");
      return false;
    }
  }

  Expression.setType(Fun->getReturnTy());

  return true;
}

} // namespace phi
