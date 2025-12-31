#include "Sema/NameResolution/NameResolver.hpp"

#include <cassert>

#include <llvm/Support/Casting.h>

#include "AST/Nodes/Stmt.hpp"

namespace phi {

bool NameResolver::resolveHeader(StructDecl &D) {
  if (!SymbolTab.insert(&D)) {
    emitRedefinitionError("Struct", SymbolTab.lookup(D), &D);
    return false;
  }
  assert(SymbolTab.lookup(D));

  return visit(D.getType());
}

bool NameResolver::visit(StructDecl *D) {
  SymbolTable::ScopeGuard StructScope(SymbolTab);
  assert(D);

  bool Success = true;

  for (auto &Field : D->getFields()) {
    Success = visit(&Field) && Success;
    SymbolTab.insert(&Field);
  }

  for (auto &Method : D->getMethods()) {
    SymbolTab.insert(&Method);
  }

  for (auto &Method : D->getMethods()) {
    Success = visit(&Method) && Success;
  }

  return Success;
}

bool NameResolver::visit(FieldDecl *D) {
  bool Success = true;

  // Handle initializer if present
  if (D->hasInit()) {
    Success = visit(D->getInit()) && Success;
  }

  if (D->hasType()) {
    Success = visit(D->getType()) && Success;
  }

  return Success;
}

} // namespace phi
