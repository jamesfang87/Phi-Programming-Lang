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
bool Sema::resolveFunDecl(FunDecl *Fun) {
  // Resolve return type
  if (!resolveTy(Fun->getReturnTy())) {
    std::println("invalid type for return in function: {}", Fun->getId());
    return false;
  }

  // Special handling for main function
  if (Fun->getId() == "main") {
    if (Fun->getReturnTy() != Type(Type::PrimitiveKind::NullKind)) {
      std::println("main cannot return a non-null value");
      return false;
    }
    if (!Fun->getParams().empty()) {
      std::println("main cannot have parameters");
      return false;
    }
  }

  // Resolve parameters
  for (const auto &Param : Fun->getParams()) {
    if (!resolveParamDecl(Param.get())) {
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
bool Sema::resolveParamDecl(ParamDecl *Param) {
  // Resolve parameter type
  if (!resolveTy(Param->getType())) {
    std::println("invalid type for parameter: {}", Param->getId());
    return false;
  }

  // Parameters can't be null type
  const Type &T = Param->getType();
  if (T.isPrimitive() &&
      T.getPrimitiveType() == Type::PrimitiveKind::NullKind) {
    std::println("param type cannot be null for: {}", Param->getId());
    return false;
  }
  return true;
}

} // namespace phi
