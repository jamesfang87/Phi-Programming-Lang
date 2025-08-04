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
bool Sema::resolve_fun_decl(FunDecl *fun) {
  // Resolve return type
  if (!resolveTy(fun->getReturnTy())) {
    std::println("invalid type for return in function: {}", fun->getID());
    return false;
  }

  // Special handling for main function
  if (fun->getID() == "main") {
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
    if (!resolve_param_decl(param.get())) {
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
bool Sema::resolve_param_decl(ParamDecl *param) {
  // Resolve parameter type
  if (!resolveTy(param->getTy())) {
    std::println("invalid type for parameter: {}", param->getID());
    return false;
  }

  // Parameters can't be null type
  const Type &t = param->getTy();
  if (t.is_primitive() && t.primitive_type() == Type::Primitive::null) {
    std::println("param type cannot be null for: {}", param->getID());
    return false;
  }
  return true;
}

} // namespace phi
