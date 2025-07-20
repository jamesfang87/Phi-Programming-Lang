#include "Sema/Sema.hpp"
#include <cassert>
#include <memory>
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
        bool success = (*ast_it)->get_block()->accept(*this);
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
