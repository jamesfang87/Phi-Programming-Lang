#include "Sema/NameResolver.hpp"

#include "AST/Decl.hpp"
#include "llvm/Support/Casting.h"
#include <cassert>
#include <print>

namespace phi {

bool NameResolver::visit(StructDecl *Struct) {
  SymbolTable::ScopeGuard StructScope(SymbolTab);
  assert(Struct);

  for (auto &Field : Struct->getFields()) {
    if (!visit(&Field)) {
      return false;
    }

    SymbolTab.insert(&Field);
  }

  for (auto &Method : Struct->getMethods()) {
    if (!visit(&Method)) {
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
        emitRedefinitionError("Parameter", SymbolTab.lookup(*param),
                              param.get());
      }
    }

    if (!resolveBlock(Method.getBody(), true)) {
      return false;
    }
  }

  return true;
}

bool NameResolver::visit(FieldDecl *Field) {
  if (!resolveType(Field->getType())) {
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
