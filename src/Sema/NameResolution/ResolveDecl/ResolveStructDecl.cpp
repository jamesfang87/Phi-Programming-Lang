#include "Sema/NameResolver.hpp"

#include <cassert>

#include <llvm/Support/Casting.h>

#include "AST/Decl.hpp"

namespace phi {

bool NameResolver::resolveHeader(StructDecl &D) {
  if (!SymbolTab.insert(&D)) {
    emitRedefinitionError("Struct", SymbolTab.lookup(D), &D);
    return false;
  }
  return true;
}

bool NameResolver::visit(StructDecl *D) {
  SymbolTable::ScopeGuard StructScope(SymbolTab);
  assert(D);

  bool Success = true;

  for (auto &Field : D->getFields()) {
    Success = visit(Field.get()) && Success;
    SymbolTab.insert(Field.get());
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
  bool Success = visit(D->getType());

  // Handle initializer if present
  if (D->hasInit()) {
    Success = visit(D->getInit()) && Success;
  }

  return Success;
}

} // namespace phi
