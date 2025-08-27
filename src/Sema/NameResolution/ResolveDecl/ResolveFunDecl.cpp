#include "Sema/NameResolver.hpp"

#include <print>

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
    std::println("invalid type for return in function: {}", Fun->getId());
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
    std::println("invalid type for parameter: {}", Param->getId());
    return false;
  }

  return true;
}

} // namespace phi
