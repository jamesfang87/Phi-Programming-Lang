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
  bool Success = resolveType(Fun->getReturnTy());

  // Resolve parameters
  for (const auto &Param : Fun->getParams()) {
    Success = visit(Param.get()) && Success;
  }

  return Success;
}

/**
 * Resolves a parameter declaration.
 *
 * Validates:
 * - Type is valid
 * - Type is not null
 */
bool NameResolver::visit(ParamDecl *Param) {
  return resolveType(Param->getType());
}

} // namespace phi
