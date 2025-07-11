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
        auto* cur_fun = dynamic_cast<ResolvedFunDecl*>(res_it->get());
        (*ast_it)->info_dump();
        active_scopes.emplace_back(); // create a new scope for the function

        // insert all parameters into the scope
        for (auto& param : cur_fun->get_params()) {
            std::println("inserting param {}", param->get_id());
            if (!insert_decl(param.get())) {
                // throw error
                std::println("while resolving function {}", cur_fun->get_id());
                std::println("failed to insert param {}, probably redefinition",
                             param->get_id());

                return {};
            }
        }

        // resolve the block
        resolve_block((*ast_it)->get_block());

        active_scopes.pop_back(); // scope should be popped when moving to the
                                  // next function
    }
    active_scopes.pop_back();

    return resolved_ast;
}

std::unique_ptr<ResolvedBlock> Sema::resolve_block(const Block* block) {
    // TODO: implement block resolution
    (void)block; // suppress unused parameter warning
    return nullptr;
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
