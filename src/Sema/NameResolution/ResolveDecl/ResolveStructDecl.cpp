#include "Sema/NameResolver.hpp"

#include "AST/Decl.hpp"
#include "llvm/Support/Casting.h"
#include <print>

namespace phi {

bool NameResolver::visit(StructDecl *Struct) {
  SymbolTable::ScopeGuard StructScope(SymbolTab);
  if (!Struct) {
    std::println("failed to resolve declaration: {}", Struct->getId());
    return false;
  }

  for (auto &Field : Struct->getFields()) {
    if (!visit(&Field)) {
      std::println("failed to resolve field: {}", Field.getId());
      return false;
    }

    SymbolTab.insert(&Field);
  }

  for (auto &Method : Struct->getMethods()) {
    if (!visit(&Method)) {
      std::println("failed to resolve method: {}", Method.getId());
      return false;
    }

    SymbolTab.insert(&Method);
  }

  for (auto &Method : Struct->getMethods()) {
    CurFun = llvm::dyn_cast<FunDecl>(&Method);

    // Create function scope
    SymbolTable::ScopeGuard FunctionScope(SymbolTab);

    // Add parameters to function scope
    for (const std::unique_ptr<ParamDecl> &param : Method.getParams()) {
      if (!SymbolTab.insert(param.get())) {
        std::println("parameter redefinition in {}: {}", Method.getId(),
                     param->getId());
        return false;
      }
    }

    if (!resolveBlock(Method.getBody(), true)) {
      std::println("method body failed: {}", Method.getId());
      return false;
    }
  }

  return true;
}

bool NameResolver::visit(FieldDecl *Field) {
  if (!resolveType(Field->getType())) {
    std::println("Undefined type for parameter: {}", Field->getId());
    return false;
  }

  // Handle initializer if present
  if (Field->hasInit() && !Field->getInit().accept(*this)) {
    std::println("failed to resolve field initializer");
    return false;
  }

  return true;
}

} // namespace phi
