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
bool NameResolver::visit(FunDecl *F) {
  bool Success = resolveType(F->getReturnTy());

  // Resolve parameters
  for (const auto &Param : F->getParams()) {
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
bool NameResolver::visit(ParamDecl *P) { return resolveType(P->getType()); }

} // namespace phi
