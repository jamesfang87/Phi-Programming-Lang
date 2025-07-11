#include "ast.hpp"
#include <memory>
#include <optional>
#include <print>
#include <sema.hpp>
#include <string>
#include <unordered_map>
#include <vector>

std::vector<std::unique_ptr<ResolvedDecl>> Sema::resolve_ast() {
    std::vector<std::unique_ptr<ResolvedDecl>> resolved_ast;
    // first create the global scope
    active_scopes.emplace_back();

    // TODO: first resolve structs so that functions
    // may use user-defined types

    // then we resolve all function declarations first
    // to allow for declarations in any order
    for (auto& fun : ast) {
        auto resolved_fun = resolve_fun_decl(fun.get());
        if (!resolved_fun) {
            // throw error
            std::println("failed to resolve function");
            return {};
        }

        if (!insert_decl(resolved_fun.get())) {
            // throw error
            std::println("failed to insert function, probably redefinition");
            return {};
        }

        // insert into scope and resolved_ast
        resolved_ast.push_back(std::move(resolved_fun));
    }

    // now we 'truly' resolve all functions
    auto ast_it = ast.begin();
    auto res_it = resolved_ast.begin();
    for (; ast_it != ast.end() && res_it != resolved_ast.end();
         ++ast_it, ++res_it) {
        cur_fun = dynamic_cast<ResolvedFunDecl*>(res_it->get());
        (*ast_it)->info_dump();
        active_scopes.emplace_back(); // create a new scope for the function

        // insert all parameters into the scope
        for (auto& param : cur_fun->get_params()) {
            if (!insert_decl(param.get())) {
                // throw error
                std::println("while resolving function {}", cur_fun->get_id());
                std::println("failed to insert param {}, probably redefinition",
                             param->get_id());

                return {};
            }
        }

        // resolve the block
        auto body = resolve_block((*ast_it)->get_block());
        if (!body) {
            std::println("while resolving function {}", cur_fun->get_id());
            std::println("failed to resolve block");

            return {};
        }

        cur_fun->set_block(std::move(body));

        active_scopes.pop_back(); // scope should be popped when moving to the
                                  // next function
    }
    active_scopes.pop_back();

    return resolved_ast;
}

std::unique_ptr<ResolvedBlock> Sema::resolve_block(const Block* block) {
    // TODO: implement block resolution

    std::vector<std::unique_ptr<ResolvedStmt>> resolved_stmts;
    for (const auto& stmt : block->get_stmts()) {
        std::unique_ptr<ResolvedStmt> r_stmt = resolve_stmt(stmt.get());
        if (!r_stmt) {
            // throw error
            std::println("failed to resolve statement");
            return nullptr;
        }
        resolved_stmts.push_back(std::move(r_stmt));
    }
    return std::make_unique<ResolvedBlock>(std::move(resolved_stmts));
}

std::optional<Type> Sema::resolve_type(Type type) {
    if (type.is_primitive()) {
        return type;
    }

    // TODO: support user-defined types
    return std::nullopt;
}

std::unique_ptr<ResolvedParamDecl>
Sema::resolve_param_decl(const ParamDecl* param) {
    auto resolved_param_type = resolve_type(param->get_type());
    if (!resolved_param_type) {
        // throw error
        std::println("invalid type for param");
        return nullptr;
    }

    Type t = resolved_param_type.value();
    // param cannot be null
    if (t.is_primitive() && t.primitive_type() == Type::Primitive::null) {
        // throw error;
        std::println("param type cannot be null");
        return nullptr;
    }

    return std::make_unique<ResolvedParamDecl>(
        param->get_location(),
        param->get_id(),
        std::move(resolved_param_type.value()));
}

std::unique_ptr<ResolvedFunDecl>
Sema::resolve_fun_decl(const FunctionDecl* fun) {
    // first resolve the return type in the funciton decl
    auto resolved_return_type = resolve_type(fun->get_return_type());
    if (!resolved_return_type) {
        // throw error
        std::println("invalid type for return");
        return nullptr;
    }

    // enforce rules for main
    if (fun->get_id() == "main") {
        if (!fun->get_return_type().is_primitive()) {
            std::println("main cannot return a custom type");
            return nullptr;
        }
        if (fun->get_return_type().primitive_type() != Type::Primitive::null) {
            std::println("main cannot return a non-null value");
            return nullptr;
        }
        if (!fun->get_params()->empty()) {
            std::println("main cannot have parameters");
            return nullptr;
        }
    }

    // make sure that params are correctly resolved
    std::vector<std::unique_ptr<ResolvedParamDecl>> resolved_params;
    for (auto& param : *fun->get_params()) {
        resolved_params.push_back(resolve_param_decl(param.get()));
    }

    return std::make_unique<ResolvedFunDecl>(
        fun->get_location(),
        fun->get_id(),
        std::move(resolved_return_type.value()),
        std::move(resolved_params),
        nullptr);
}

std::pair<ResolvedDecl*, int> Sema::lookup_decl(const std::string& name) {
    for (int i = active_scopes.size() - 1; i >= 0; --i) {
        auto it = active_scopes[i].find(name);
        if (it != active_scopes[i].end()) {
            return {it->second, i};
        }
    }
    return {nullptr, -1};
}

bool Sema::insert_decl(ResolvedDecl* decl) {
    if (lookup_decl(decl->get_id()).first != nullptr) {
        // throw error
        std::println("redefinition error: {}", decl->get_id());
        for (auto& decl : active_scopes[lookup_decl(decl->get_id()).second]) {
            std::println("{}", decl.first);
        }
        return false;
    }

    // push into unordered_map representing most inner scope
    active_scopes.back()[decl->get_id()] = decl;
    return true;
}

std::unique_ptr<ResolvedDeclRef> Sema::resolve_decl_ref(const DeclRef* declref,
                                                        bool function_call) {
    auto [decl, depth] = lookup_decl(declref->get_id());

    // if the declaration is not found
    if (!decl) {
        // throw error
        std::println("undeclared error: {}", declref->get_id());
        return nullptr;
    }

    // if we are not trying to call a function and the declaration is a function
    if (!function_call && dynamic_cast<ResolvedFunDecl*>(decl)) {
        std::println("Did you mean to call a function?");
        return nullptr;
    }

    return std::make_unique<ResolvedDeclRef>(declref->get_location(),
                                             decl->get_type(),
                                             declref->get_id(),
                                             decl);
}

std::unique_ptr<ResolvedFunctionCall>
Sema::resolve_function_call(const FunctionCall* call) {
    auto declref = dynamic_cast<const DeclRef*>(call->get_callee());
    auto resolved_decl_ref = resolve_decl_ref(declref, true);

    if (!resolved_decl_ref) {
        std::println("You cannot call an expression");
        return nullptr;
    }

    // Get the resolved function declaration from the decl_ref
    auto resolved_fun =
        dynamic_cast<const ResolvedFunDecl*>(resolved_decl_ref->get_decl());

    // check parma list length is the same
    const auto& params = resolved_fun->get_params();
    if (call->get_args().size() != params.size()) {
        std::println("Parameter list length mismatch");
        return nullptr;
    }

    // Resolve the arguments and check param types are compatible
    std::vector<std::unique_ptr<ResolvedExpr>> resolved_args;
    for (int i = 0; i < (int)call->get_args().size(); i++) {
        auto resolved_arg = resolve_expr(call->get_args()[i].get());
        if (!resolved_arg) {
            std::println("Could not resolve argument");
            return nullptr;
        }

        auto param = params.at(i).get();
        // TODO: Implement type checking for function calls
        if (resolved_arg->get_type() != param->get_type()) {
            std::println("Argument type mismatch");
            return nullptr;
        }

        resolved_args.push_back(std::move(resolved_arg));
    }

    return std::make_unique<ResolvedFunctionCall>(
        call->get_location(),
        resolved_fun->get_type(),
        const_cast<ResolvedFunDecl*>(resolved_fun),
        std::move(resolved_args));
    /*

    // check param list length is the same
    if (call->get_params().size() != call->get_fun()->get_params().size()) {
        std::println("Parameter list length mismatch");
        return nullptr;
    }

    // check param types are compatible

    return std::make_unique<ResolvedFunctionCall>(call->get_location(),
                                                  fun->get_type(),
                                                  fun->get_id(),
                                                  fun);
                                                  */
}

std::unique_ptr<ResolvedExpr> Sema::resolve_expr(const Expr* expr) {
    if (const auto* int_lit = dynamic_cast<const IntLiteral*>(expr)) {
        return std::make_unique<ResolvedIntLiteral>(int_lit->get_location(),
                                                    int_lit->get_value());
    }

    if (const auto* float_lit = dynamic_cast<const FloatLiteral*>(expr)) {
        return std::make_unique<ResolvedFloatLiteral>(float_lit->get_location(),
                                                      float_lit->get_value());
    }

    if (const auto* declref = dynamic_cast<const DeclRef*>(expr)) {
        return resolve_decl_ref(declref, false);
    }

    if (const auto* call = dynamic_cast<const FunctionCall*>(expr)) {
        return resolve_function_call(call);
    }

    return nullptr;
}

std::unique_ptr<ResolvedReturnStmt>
Sema::resolve_return_stmt(const ReturnStmt* stmt) {
    auto expr = resolve_expr(stmt->get_expr());
    if (!expr) {
        return nullptr;
    }

    // compare to the current function
    if (expr->get_type() != cur_fun->get_type()) {
        // throw error
        std::println("type mismatch error: {}", cur_fun->get_id());
        std::println("return stmt type: {}", expr->get_type().to_string());
        std::println("expected type: {}", cur_fun->get_type().to_string());
        return nullptr;
    }

    return std::make_unique<ResolvedReturnStmt>(stmt->get_location(),
                                                std::move(expr));
}

std::unique_ptr<ResolvedStmt> Sema::resolve_stmt(const Stmt* stmt) {
    if (auto* expr = dynamic_cast<const Expr*>(stmt)) {
        return resolve_expr(expr);
    }

    if (auto* return_stmt = dynamic_cast<const ReturnStmt*>(stmt)) {
        return resolve_return_stmt(return_stmt);
    }
    return nullptr;
}
