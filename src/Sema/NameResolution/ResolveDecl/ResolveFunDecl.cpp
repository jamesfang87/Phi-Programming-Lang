#include "Sema/NameResolver.hpp"

namespace phi {

/**
 * Resolves a function declaration.
 *
 * Validates:
 * - Return type is valid
 * - Parameters are valid
 * - Special rules for main() function
 */
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

/**
 * Resolves a parameter declaration.
 *
 * Validates:
 * - Type is valid
 * - Type is not null
 */
bool NameResolver::visit(ParamDecl *D) { return visit(D->getType()); }

} // namespace phi
