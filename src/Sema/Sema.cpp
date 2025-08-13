#include "Sema/Sema.hpp"

#include <cassert>
#include <memory>
#include <print>
#include <vector>

#include "AST/Decl.hpp"
#include "llvm/Support/Casting.h"

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
std::pair<bool, std::vector<std::unique_ptr<Decl>>> Sema::resolveAST() {
  // Create global scope
  SymbolTable::ScopeGuard GlobalScope(SymbolTab);

  // Decl with bodies are handled in 2 phases.
  // Phase 1: Resolve signatures
  // Phase 2: Resolve bodies

  for (auto &DeclPtr : Ast) {
    auto Struct = llvm::dyn_cast<StructDecl>(DeclPtr.get());
    if (!Struct) {
      // Skip if this is not a struct declaration
      continue;
    }

    // now insert into symbol table
    if (!SymbolTab.insert(Struct)) {
      std::println("struct redefinition: {}", DeclPtr->getId());
      return {false, {}};
    }
  }

  // Phase 1: Resolve function signatures
  for (auto &DeclPtr : Ast) {
    auto Fun = llvm::dyn_cast<FunDecl>(DeclPtr.get());
    if (!Fun) {
      continue;
    }

    if (!resolveFunDecl(Fun)) {
      std::println("failed to resolve declaration: {}", DeclPtr->getId());
      return {false, {}};
    }

    if (!SymbolTab.insert(Fun)) {
      std::println("function redefinition: {}", DeclPtr->getId());
      return {false, {}};
    }
  }

  // Phase 2: Resolve function bodies
  for (auto &decl : Ast) {
    CurFun = llvm::dyn_cast<FunDecl>(decl.get());
    if (CurFun) {
      // Create function scope
      SymbolTable::ScopeGuard FunctionScope(SymbolTab);

      // Add parameters to function scope
      for (const std::unique_ptr<ParamDecl> &param : CurFun->getParams()) {
        if (!SymbolTab.insert(param.get())) {
          std::println("parameter redefinition in {}: {}", CurFun->getId(),
                       param->getId());
          return {false, {}};
        }
      }

      // Resolve function body
      if (!resolveBlock(CurFun->getBody(), true)) {
        std::println("failed to resolve body of: {}", CurFun->getId());
        return {false, {}};
      }
    }

    auto Struct = llvm::dyn_cast<StructDecl>(decl.get());
    if (Struct) {
      if (!resolveStructDecl(Struct)) {
        std::println("failed to resolve declaration: {}", decl->getId());
        return {false, {}};
      }
    }
  }

  return {true, std::move(Ast)};
}

} // namespace phi
