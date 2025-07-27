#include "Sema/Sema.hpp"

#include <print>

namespace phi {

bool Sema::resolve_fun_decl(FunDecl* fun) {
    // first resolve the return type in the funciton decl
    bool resolved_return_type = resolve_type(fun->get_return_type());
    if (!resolved_return_type) {
        std::println("invalid type for return");
        std::println("for function: {}", fun->get_id());
        return false;
    }

    // enforce rules for main
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

    // make sure that params are correctly resolved
    for (const auto& param : fun->get_params()) {
        if (!resolve_param_decl(param.get())) {
            return false;
        }
    }

    return true;
}

bool Sema::resolve_param_decl(ParamDecl* param) {
    const bool resolved_param_type = resolve_type(param->get_type());
    if (!resolved_param_type) {
        // throw error
        std::println("invalid type for param");
        return false;
    }

    const Type& t = param->get_type();
    // param cannot be null
    if (t.is_primitive() && t.primitive_type() == Type::Primitive::null) {
        // throw error;
        std::println("param type cannot be null");
        return false;
    }
    return true;
}

} // namespace phi
