#include "sema.hpp"
#include <cassert>
#include <memory>
#include <optional>
#include <print>
#include <string>
#include <unordered_map>
#include <vector>

std::pair<bool, std::vector<std::unique_ptr<FunDecl>>> Sema::resolve_ast() {
    // first create the global scope
    active_scopes.emplace_back();

    // TODO: first resolve structs so that functions
    // may use user-defined types

    // then we resolve all function declarations first
    // to allow for declarations in any order
    for (auto& fun_decl : ast) {
        bool success = resolve_fun_decl(fun_decl.get());
        if (!success) {
            // throw error
            std::println("failed to resolve function");
            return {false, {}};
        }

        if (!insert_decl(fun_decl.get())) {
            // throw error
            std::println("failed to insert function, probably redefinition");
            return {false, {}};
        }
    }

    // now we 'truly' resolve all functions
    for (auto ast_it = ast.begin(); ast_it != ast.end(); ++ast_it) {
        cur_fun = ast_it->get();
        active_scopes.emplace_back(); // create a new scope for the function

        // insert all parameters into the scope
        for (const std::unique_ptr<ParamDecl>& param : cur_fun->get_params()) {
            if (!insert_decl(param.get())) {
                // throw error
                std::println("while resolving function {}", cur_fun->get_id());
                std::println("failed to insert param {}, probably redefinition", param->get_id());

                return {false, {}};
            }
        }

        // resolve the block
        bool success = resolve_block((*ast_it)->get_block());
        if (!success) {
            std::println("while resolving function {}", cur_fun->get_id());
            std::println("failed to resolve block");

            return {false, {}};
        }

        active_scopes.pop_back(); // scope should be popped when moving to the
                                  // next function
    }
    active_scopes.pop_back();

    return {true, std::move(ast)};
}

bool Sema::resolve_block(Block* block) {
    for (const auto& stmt : block->get_stmts()) {
        bool success = resolve_stmt(stmt.get());
        if (!success) {
            // throw error
            std::println("failed to resolve statement");
            return false;
        }
    }
    return true;
}

std::optional<Type> Sema::resolve_type(Type type) {
    if (type.is_primitive()) {
        return type;
    }

    // TODO: support user-defined types
    return std::nullopt;
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

bool Sema::resolve_fun_decl(FunDecl* fun) {
    // first resolve the return type in the funciton decl
    std::optional<Type> resolved_return_type = resolve_type(fun->get_return_type());
    if (!resolved_return_type) {
        // throw error
        std::println("invalid type for return");
        return false;
    }

    // enforce rules for main
    if (fun->get_id() == "main") {
        if (!fun->get_return_type().is_primitive()) {
            std::println("main cannot return a custom type");
            return false;
        }
        if (fun->get_return_type().primitive_type() != Type::Primitive::null) {
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

Decl* Sema::lookup_decl(const std::string& name) {
    for (int i = active_scopes.size() - 1; i >= 0; --i) {
        auto it = active_scopes[i].find(name);
        if (it != active_scopes[i].end()) {
            return it->second;
        }
    }
    return nullptr;
}

bool Sema::insert_decl(Decl* decl) {
    if (lookup_decl(decl->get_id()) != nullptr) {
        // throw error
        std::println("redefinition error: {}", decl->get_id());
        return false;
    }

    // push into unordered_map representing most inner scope
    active_scopes.back()[decl->get_id()] = decl;
    return true;
}

bool Sema::resolve_decl_ref(DeclRefExpr* declref, bool function_call) {
    Decl* decl = lookup_decl(declref->get_id());

    // if the declaration is not found
    if (!decl) {
        // throw error
        std::println("undeclared error: {}", declref->get_id());
        return false;
    }

    // if we are not trying to call a function and the declaration is a function
    if (!function_call && dynamic_cast<FunDecl*>(decl)) {
        std::println("Did you mean to call a function?");
        return false;
    }

    declref->set_decl(decl);
    declref->set_type(decl->get_type()); // debug

    if (function_call) {
        std::println("decl: {}", decl->get_id());
        std::println("type: {}", decl->get_type().to_string());
    }

    return true;
}

bool Sema::resolve_function_call(FunCallExpr* call) {
    auto declref = dynamic_cast<DeclRefExpr*>(call->get_callee());
    bool success = resolve_decl_ref(declref, true);

    if (!success) {
        std::println("You cannot call an expression");
        return false;
    }

    // Get the resolved function declaration from the decl_ref
    auto resolved_fun = dynamic_cast<FunDecl*>(declref->get_decl());
    assert(resolved_fun);
    // check parma list length is the same
    const std::vector<std::unique_ptr<ParamDecl>>& params = resolved_fun->get_params();
    if (call->get_args().size() != params.size()) {
        std::println("Parameter list length mismatch");
        return false;
    }

    // Resolve the arguments and check param types are compatible
    for (int i = 0; i < (int)call->get_args().size(); i++) {
        Expr* arg = call->get_args()[i].get();
        bool success_expr = resolve_expr(arg);
        if (!success_expr) {
            std::println("Could not resolve argument");
            return false;
        }

        ParamDecl* param = params.at(i).get();
        // TODO: Implement type checking for function calls
        if (arg->get_type() != param->get_type()) {
            std::println("Argument type mismatch");
            return false;
        }
    }
    call->set_type(resolved_fun->get_return_type());

    return true;
}

bool Sema::resolve_expr(Expr* expr) {
    if (auto* int_lit = dynamic_cast<IntLiteral*>(expr)) {
        expr->set_type(Type(Type::Primitive::i64));
        return true;
    }

    if (auto* float_lit = dynamic_cast<FloatLiteral*>(expr)) {
        expr->set_type(Type(Type::Primitive::f64));
        return true;
    }

    if (auto* declref = dynamic_cast<DeclRefExpr*>(expr)) {
        return resolve_decl_ref(declref, false);
    }

    if (auto* call = dynamic_cast<FunCallExpr*>(expr)) {
        return resolve_function_call(call);
    }

    std::println("unsupported expr");
    return false;
}

bool Sema::resolve_return_stmt(ReturnStmt* stmt) {
    // case that the function is void
    if (!stmt->get_expr()) {
        if (cur_fun->get_return_type() != Type(Type::Primitive::null)) {
            std::println("error: function '{}' should return a value", cur_fun->get_id());
            return false;
        }
        return true;
    }

    // now handle the case that the function is not void
    bool success = resolve_expr(stmt->get_expr());
    if (!success) {
        return false;
    }

    // compare to the current function
    std::println("stmt: {}, in fn: {}", stmt->get_expr()->is_resolved(), cur_fun->get_id());
    if (stmt->get_expr()->get_type() != cur_fun->get_return_type()) {
        // throw error
        std::println("type mismatch error: {}", cur_fun->get_id());
        std::println("return stmt type: {}", stmt->get_expr()->get_type().to_string());
        std::println("expected type: {}", cur_fun->get_return_type().to_string());
        return false;
    }

    return true;
}

bool Sema::resolve_stmt(Stmt* stmt) {
    if (auto* expr = dynamic_cast<Expr*>(stmt)) {
        return resolve_expr(expr);
    }

    if (auto* return_stmt = dynamic_cast<ReturnStmt*>(stmt)) {
        return resolve_return_stmt(return_stmt);
    }

    std::println("unsupported stmt");
    return false;
}
