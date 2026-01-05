#include "Sema/NameResolution/NameResolver.hpp"

#include <cassert>
#include <memory>
#include <vector>

#include "llvm/Support/Casting.h"

#include "AST/Nodes/Stmt.hpp"

namespace phi {

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
