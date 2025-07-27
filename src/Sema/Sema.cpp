#include "Sema/Sema.hpp"

#include <cassert>
#include <memory>
#include <print>
#include <vector>

#include "AST/Decl.hpp"

std::pair<bool, std::vector<std::unique_ptr<FunDecl>>> Sema::resolve_ast() {
    // Create global scope with RAII
    SymbolTable::ScopeGuard global_scope(symbol_table);

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

        if (!symbol_table.insert_decl(fun_decl.get())) {
            // throw error
            std::println("failed to insert function, probably redefinition");
            return {false, {}};
        }
    }

    // now we 'truly' resolve all functions
    for (auto& fun_decl : ast) {
        cur_fun = fun_decl.get();

        // Create function scope with RAII - parameters and body share this scope
        SymbolTable::ScopeGuard function_scope(symbol_table);

        // insert all parameters into the function scope
        for (const std::unique_ptr<ParamDecl>& param : cur_fun->get_params()) {
            if (!symbol_table.insert_decl(param.get())) {
                // throw error
                std::println("while resolving function {}", cur_fun->get_id());
                std::println("failed to insert param {}, probably redefinition", param->get_id());
                return {false, {}};
            }
        }

        // resolve the block (function body shares scope with parameters)
        bool success = resolve_block(fun_decl->get_block(), true);
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
