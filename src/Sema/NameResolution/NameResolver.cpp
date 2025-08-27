#include "Sema/NameResolver.hpp"

#include <cassert>
#include <memory>
#include <print>
#include <vector>

#include "AST/Decl.hpp"
#include "llvm/Support/Casting.h"

namespace phi {

std::pair<bool, std::vector<std::unique_ptr<Decl>>>
NameResolver::resolveNames() {
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

    if (!visit(Fun)) {
      std::println("failed to resolve declaration: {}", DeclPtr->getId());
      return {false, {}};
    }

    if (!SymbolTab.insert(Fun)) {
      std::println("function redefinition: {}", DeclPtr->getId());
      return {false, {}};
    }
  }

  // Phase 2: Resolve function bodies
  for (auto &Decl : Ast) {
    CurFun = llvm::dyn_cast<FunDecl>(Decl.get());
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

    auto Struct = llvm::dyn_cast<StructDecl>(Decl.get());
    if (Struct) {
      if (!visit(Struct)) {
        std::println("failed to resolve declaration: {}", Decl->getId());
        return {false, {}};
      }
    }
  }

  return {true, std::move(Ast)};
}

} // namespace phi
