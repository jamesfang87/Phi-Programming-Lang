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
std::pair<bool, std::vector<std::unique_ptr<FunDecl>>> Sema::resolveAST() {
  // Create global scope
  SymbolTable::ScopeGuard global_scope(symbolTable);

  // TODO: Add struct resolution here

  // Phase 1: Resolve function signatures
  for (auto &funDecl : ast) {
    if (!resolveFunDecl(funDecl.get())) {
      std::println("failed to resolve function signature: {}",
                   funDecl->getID());
      return {false, {}};
    }

    if (!symbolTable.insert(funDecl.get())) {
      std::println("function redefinition: {}", funDecl->getID());
      return {false, {}};
    }
  }

  // Phase 2: Resolve function bodies
  for (auto &funDecl : ast) {
    curFun = funDecl.get();

    // Create function scope
    SymbolTable::ScopeGuard function_scope(symbolTable);

    // Add parameters to function scope
    for (const std::unique_ptr<ParamDecl> &param : curFun->getParams()) {
      if (!symbolTable.insert(param.get())) {
        std::println("parameter redefinition in {}: {}", curFun->getID(),
                     param->getID());
        return {false, {}};
      }
    }

    // Resolve function body
    if (!resolveBlock(funDecl->getBlock(), true)) {
      std::println("failed to resolve body of: {}", curFun->getID());
      return {false, {}};
    }
  }

  return {true, std::move(ast)};
}

} // namespace phi
