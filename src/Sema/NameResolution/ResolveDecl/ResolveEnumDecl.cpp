#include "Sema/NameResolution/NameResolver.hpp"

#include <cassert>
#include <string>
#include <unordered_map>

#include "AST/Nodes/Stmt.hpp"

namespace phi {

bool NameResolver::resolveHeader(EnumDecl &D) {
  if (!SymbolTab.insert(&D)) {
    emitRedefinitionError("Enum", SymbolTab.lookup(D), &D);
    return false;
  }
  assert(SymbolTab.lookup(D));

  // should just be true...
  bool Success = visit(D.getType());
  return Success;
}

bool NameResolver::visit(EnumDecl *D) {
  bool Success = true;
  std::unordered_map<std::string, VariantDecl *> SeenVariants;
  for (auto &Variant : D->getVariants()) {
    if (SeenVariants.contains(Variant.getId())) {
      assert(SeenVariants[Variant.getId()]);
      emitRedefinitionError("Variant", SeenVariants[Variant.getId()], &Variant);
      Success = false;
    }
    SeenVariants[Variant.getId()] = &Variant;

    Success = visit(&Variant) && Success;
  }

  for (auto &Method : D->getMethods()) {
    SymbolTab.insert(&Method);
  }

  for (auto &Method : D->getMethods()) {
    Success = visit(&Method) && Success;
  }

  return Success;
}

bool NameResolver::visit(VariantDecl *D) {
  if (D->hasType()) {
    return visit(D->getType());
  }
  return true;
}

} // namespace phi
