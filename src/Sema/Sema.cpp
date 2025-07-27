#include "Sema/Sema.hpp"

#include <cassert>
#include <memory>
#include <print>
#include <vector>

#include "AST/Decl.hpp"

namespace phi {

/**
 * Main entry point for semantic analysis.
 *
 * @return Pair of success status and resolved AST
 *
 * Performs in two phases:
 * 1. Resolve all function signatures (allows mutual recursion)
 * 2. Resolve function bodies with proper scoping
 */
std::pair<bool, std::vector<std::unique_ptr<FunDecl>>> Sema::resolve_ast() {
    // Create global scope
    SymbolTable::ScopeGuard global_scope(symbol_table);

    // TODO: Add struct resolution here

    // Phase 1: Resolve function signatures
    for (auto& fun_decl : ast) {
        if (!resolve_fun_decl(fun_decl.get())) {
            std::println("failed to resolve function signature: {}", fun_decl->get_id());
            return {false, {}};
        }

        if (!symbol_table.insert_decl(fun_decl.get())) {
            std::println("function redefinition: {}", fun_decl->get_id());
            return {false, {}};
        }
    }

    // Phase 2: Resolve function bodies
    for (auto& fun_decl : ast) {
        cur_fun = fun_decl.get();

        // Create function scope
        SymbolTable::ScopeGuard function_scope(symbol_table);

        // Add parameters to function scope
        for (const std::unique_ptr<ParamDecl>& param : cur_fun->get_params()) {
            if (!symbol_table.insert_decl(param.get())) {
                std::println("parameter redefinition in {}: {}",
                             cur_fun->get_id(),
                             param->get_id());
                return {false, {}};
            }
        }

        // Resolve function body
        if (!resolve_block(fun_decl->get_block(), true)) {
            std::println("failed to resolve body of: {}", cur_fun->get_id());
            return {false, {}};
        }
    }

    return {true, std::move(ast)};
}

} // namespace phi
