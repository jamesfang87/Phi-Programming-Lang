#include "Sema/NameResolver.hpp"

namespace phi {

bool NameResolver::resolveHeader(FunDecl &D) {
  bool Success = visit(D.getReturnTy());

  // Resolve parameters
  for (const auto &Param : D.getParams()) {
    Success = visit(Param.get()) && Success;
  }

  if (!SymbolTab.insert(&D)) {
    emitRedefinitionError("Function", SymbolTab.lookup(D), &D);
  }

  return Success;
}

bool NameResolver::visit(FunDecl *D) {
  CurrentFun = D;
  if (!CurrentFun) {
    return false;
  }

  // Create function scope
  SymbolTable::ScopeGuard FunctionScope(SymbolTab);

  // Add parameters to function scope
  bool Success = true;
  for (const auto &Param : CurrentFun->getParams()) {
    if (!SymbolTab.insert(Param.get())) {
      emitRedefinitionError("Parameter", SymbolTab.lookup(*Param), Param.get());
      Success = false;
    }
  }

  // Resolve function body
  return visit(CurrentFun->getBody(), true) && Success;
}

bool NameResolver::visit(ParamDecl *D) { return visit(D->getType()); }

} // namespace phi
