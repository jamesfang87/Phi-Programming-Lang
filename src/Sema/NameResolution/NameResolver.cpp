#include "Sema/NameResolver.hpp"

#include <cassert>
#include <memory>
#include <vector>

#include <llvm/Support/Casting.h>

#include "AST/Decl.hpp"

namespace phi {

bool NameResolver::resolveHeader(Decl &D) {
  if (auto *Struct = llvm::dyn_cast<StructDecl>(&D)) {
    return resolveHeader(*Struct);
  } else if (auto *Fun = llvm::dyn_cast<FunDecl>(&D)) {
    return resolveHeader(*Fun);
  }
  return true;
}

bool NameResolver::resolveHeader(StructDecl &D) {
  if (!SymbolTab.insert(&D)) {
    emitRedefinitionError("Struct", SymbolTab.lookup(D), &D);
    return false;
  }
  return true;
}

bool NameResolver::resolveHeader(FunDecl &D) {
  bool Success = visit(D.getReturnTy());

  // Resolve parameters
  for (const auto &Param : D.getParams()) {
    Success = visit(Param.get()) && Success;
  }

  if (!SymbolTab.insert(&D)) {
    emitRedefinitionError("Function", SymbolTab.lookup(D), &D);
  }

  return Success;
}

bool NameResolver::resolveBodies(Decl &D) {
  if (auto *Struct = llvm::dyn_cast<StructDecl>(&D)) {
    return visit(Struct);
  } else if (auto *Fun = llvm::dyn_cast<FunDecl>(&D)) {
    return visit(Fun);
  }
  return true;
}

std::pair<bool, std::vector<std::unique_ptr<Decl>>>
NameResolver::resolveNames() {
  // Create global scope
  SymbolTable::ScopeGuard GlobalScope(SymbolTab);

  // Decl with bodies are handled in 2 phases.
  // Phase 1: Resolve signatures
  // Phase 2: Resolve bodies

  for (auto &DeclPtr : Ast) {
    resolveHeader(*DeclPtr);
  }

  // Phase 2: Resolve function bodies
  for (auto &Decl : Ast) {
    resolveBodies(*Decl);
  }

  return {!Diags->has_errors(), std::move(Ast)};
}

} // namespace phi
