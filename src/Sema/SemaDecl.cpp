#include "Sema/Sema.hpp"

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
bool Sema::resolveFunDecl(FunDecl *fun) {
  // Resolve return type
  if (!resolveTy(fun->getReturnTy())) {
    std::println("invalid type for return in function: {}", fun->getId());
    return false;
  }

  // Special handling for main function
  if (fun->getId() == "main") {
    if (fun->getReturnTy() != Type(Type::Primitive::null)) {
      std::println("main cannot return a non-null value");
      return false;
    }
    if (!fun->getParams().empty()) {
      std::println("main cannot have parameters");
      return false;
    }
  }

  // Resolve parameters
  for (const auto &param : fun->getParams()) {
    if (!resolveParamDecl(param.get())) {
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
bool Sema::resolveParamDecl(ParamDecl *param) {
  // Resolve parameter type
  if (!resolveTy(param->getType())) {
    std::println("invalid type for parameter: {}", param->getId());
    return false;
  }

  // Parameters can't be null type
  const Type &t = param->getType();
  if (t.isPrimitive() && t.primitive_type() == Type::Primitive::null) {
    std::println("param type cannot be null for: {}", param->getId());
    return false;
  }
  return true;
}

} // namespace phi
