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
bool Sema::resolve_fun_decl(FunDecl* fun) {
    // Resolve return type
    if (!resolve_type(fun->get_return_type())) {
        std::println("invalid type for return in function: {}", fun->get_id());
        return false;
    }

    // Special handling for main function
    if (fun->get_id() == "main") {
        if (fun->get_return_type() != Type(Type::Primitive::null)) {
            std::println("main cannot return a non-null value");
            return false;
        }
        if (!fun->get_params().empty()) {
            std::println("main cannot have parameters");
            return false;
        }
    }

    // Resolve parameters
    for (const auto& param : fun->get_params()) {
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
bool Sema::resolve_param_decl(ParamDecl* param) {
    // Resolve parameter type
    if (!resolve_type(param->get_type())) {
        std::println("invalid type for parameter: {}", param->get_id());
        return false;
    }

    // Parameters can't be null type
    const Type& t = param->get_type();
    if (t.is_primitive() && t.primitive_type() == Type::Primitive::null) {
        std::println("param type cannot be null for: {}", param->get_id());
        return false;
    }
    return true;
}

} // namespace phi
