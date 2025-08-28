#include "Sema/NameResolver.hpp"

#include <cassert>
#include <memory>
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
      emitRedefinitionError("Function", SymbolTab.lookup(*Struct), Struct);
    }
  }

  // Phase 1: Resolve function signatures
  for (auto &DeclPtr : Ast) {
    auto Fun = llvm::dyn_cast<FunDecl>(DeclPtr.get());
    if (!Fun) {
      continue;
    }

    if (!visit(Fun)) {
      return {false, {}};
    }

    if (!SymbolTab.insert(Fun)) {
      emitRedefinitionError("Function", SymbolTab.lookup(*Fun), Fun);
    }
  }

  // Phase 2: Resolve function bodies
  for (auto &Decl : Ast) {
    CurFun = llvm::dyn_cast<FunDecl>(Decl.get());
    if (CurFun) {
      // Create function scope
      SymbolTable::ScopeGuard FunctionScope(SymbolTab);

      // Add parameters to function scope
      for (const std::unique_ptr<ParamDecl> &Param : CurFun->getParams()) {
        if (!SymbolTab.insert(Param.get())) {
          emitRedefinitionError("Parameter", SymbolTab.lookup(*Param),
                                Param.get());
        }
      }

      // Resolve function body
      if (!resolveBlock(CurFun->getBody(), true)) {
        return {false, {}};
      }
    }

    auto Struct = llvm::dyn_cast<StructDecl>(Decl.get());
    if (Struct) {
      if (!visit(Struct)) {
        return {false, {}};
      }
    }
  }

  return {true, std::move(Ast)};
}

} // namespace phi
