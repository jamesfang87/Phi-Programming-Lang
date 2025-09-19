#include "Sema/NameResolver.hpp"

#include <cassert>

#include <llvm/Support/Casting.h>

#include "AST/Decl.hpp"

namespace phi {

bool NameResolver::visit(StructDecl *D) {
  SymbolTable::ScopeGuard StructScope(SymbolTab);
  assert(D);

  bool Success = true;

  for (auto &Field : D->getFields()) {
    Success = visit(Field.get()) && Success;
    SymbolTab.insert(Field.get());
  }

  for (auto &Method : D->getMethods()) {
    Success = visit(&Method) && Success;
    SymbolTab.insert(&Method);
  }

  for (auto &Method : D->getMethods()) {
    CurrentFun = llvm::dyn_cast<FunDecl>(&Method);

    // Create function scope
    SymbolTable::ScopeGuard FunctionScope(SymbolTab);

    // Add parameters to function scope
    for (const std::unique_ptr<ParamDecl> &param : Method.getParams()) {
      if (!SymbolTab.insert(param.get())) {
        emitRedefinitionError("Parameter", SymbolTab.lookup(*param),
                              param.get());
      }
    }

    Success = visit(Method.getBody(), true) && Success;
  }

  return Success;
}

bool NameResolver::visit(FieldDecl *D) {
  bool Success = visit(D->getType());

  // Handle initializer if present
  if (D->hasInit()) {
    Success = visit(D->getInit()) && Success;
  }

  return Success;
}

} // namespace phi
