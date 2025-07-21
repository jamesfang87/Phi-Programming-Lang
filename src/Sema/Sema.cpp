#include "Sema/Sema.hpp"
#include "AST/Decl.hpp"
#include <cassert>
#include <memory>
#include <print>
#include <string>
#include <unordered_map>
#include <vector>

std::pair<bool, std::vector<std::unique_ptr<FunDecl>>> Sema::resolve_ast() {
    // Create global scope with RAII
    ScopeGuard global_scope(active_scopes);

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
    for (auto& fun_decl : ast) {
        cur_fun = fun_decl.get();

        // Create function scope with RAII - parameters and body share this scope
        ScopeGuard function_scope(active_scopes);

        // insert all parameters into the function scope
        for (const std::unique_ptr<ParamDecl>& param : cur_fun->get_params()) {
            if (!insert_decl(param.get())) {
                // throw error
                std::println("while resolving function {}", cur_fun->get_id());
                std::println("failed to insert param {}, probably redefinition", param->get_id());
                return {false, {}};
            }
        }

        // Mark the next block as a function body (shouldn't create its own scope)
        is_function_body_block = true;

        // resolve the block (function body shares scope with parameters)
        bool success = fun_decl->get_block()->accept(*this);
        if (!success) {
            std::println("while resolving function {}", cur_fun->get_id());
            std::println("failed to resolve block");
            return {false, {}};
        }

        // Function scope will be automatically popped by ScopeGuard destructor
    }

    // Global scope will be automatically popped by ScopeGuard destructor
    return {true, std::move(ast)};
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
