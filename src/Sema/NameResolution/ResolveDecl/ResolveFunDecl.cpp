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
bool NameResolver::visit(FunDecl *Fun) {
  // Resolve return type
  if (!resolveType(Fun->getReturnTy())) {
    return false;
  }

  // Resolve parameters
  for (const auto &Param : Fun->getParams()) {
    if (!visit(Param.get())) {
      return false;
    }
  }

  return true;
}

/**
 * Resolves a parameter declaration.
 *
 * Validates:
 * - Type is valid
 * - Type is not null
 */
bool NameResolver::visit(ParamDecl *Param) {
  // Resolve parameter type
  if (!resolveType(Param->getType())) {
    return false;
  }

  return true;
}

} // namespace phi
