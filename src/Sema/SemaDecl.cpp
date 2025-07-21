#include "Sema/Sema.hpp"
#include <print>

bool Sema::resolve_fun_decl(FunDecl* fun) {
    // first resolve the return type in the funciton decl
    std::optional<Type> resolved_return_type = resolve_type(fun->get_return_type());
    if (!resolved_return_type) {
        std::println("invalid type for return");
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
    std::optional<Type> resolved_param_type = resolve_type(param->get_type());
    if (!resolved_param_type) {
        // throw error
        std::println("invalid type for param");
        return false;
    }

    const Type& t = resolved_param_type.value();
    // param cannot be null
    if (t.is_primitive() && t.primitive_type() == Type::Primitive::null) {
        // throw error;
        std::println("param type cannot be null");
        return false;
    }
    return true;
}

bool Sema::resolve_var_decl(VarDecl* var) {
    // First resolve the variable's declared type
    std::optional<Type> resolved_var_type = resolve_type(var->get_type());
    if (!resolved_var_type) {
        std::println("invalid type for variable");
        return false;
    }

    // If there's an initializer, resolve it and check type compatibility
    if (var->has_initializer()) {
        Expr* initializer = var->get_initializer();
        if (!initializer->accept(*this)) {
            std::println("failed to resolve variable initializer");
            return false;
        }

        Type initializer_type = initializer->get_type();
        Type var_type = resolved_var_type.value();

        // Check if initializer type matches variable type
        if (initializer_type != var_type) {
            std::println("variable initializer type mismatch");
            std::println("variable type: {}", var_type.to_string());
            std::println("initializer type: {}", initializer_type.to_string());
            return false;
        }
    } else if (var->is_constant()) {
        // Constant variables must have an initializer
        std::println("constant variable '{}' must have an initializer", var->get_id());
        return false;
    }

    return true;
}
