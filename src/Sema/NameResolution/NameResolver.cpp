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
  }
  if (auto *Fun = llvm::dyn_cast<FunDecl>(&D)) {
    return resolveHeader(*Fun);
  }

  if (auto *Enum = llvm::dyn_cast<EnumDecl>(&D)) {
    return resolveHeader(*Enum);
  }
  return true;
}

bool NameResolver::resolveBodies(Decl &D) {
  if (auto *Struct = llvm::dyn_cast<StructDecl>(&D)) {
    return visit(Struct);
  }
  if (auto *Fun = llvm::dyn_cast<FunDecl>(&D)) {
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
