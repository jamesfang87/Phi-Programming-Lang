#include "AST/Decl.hpp"
#include "Sema/NameResolver.hpp"
#include <cassert>
#include <string>
#include <unordered_map>

namespace phi {

bool NameResolver::resolveHeader(EnumDecl &D) {
  if (!SymbolTab.insert(&D)) {
    emitRedefinitionError("Enum", SymbolTab.lookup(D), &D);
    return false;
  }
  assert(SymbolTab.lookup(D));
  return true;
}

bool NameResolver::visit(EnumDecl *D) {
  bool Success = true;
  std::unordered_map<std::string, VariantDecl *> Seen;
  for (auto &Variant : D->getVariants()) {
    if (Seen.contains(Variant.getId())) {
      assert(Seen[Variant.getId()]);
      emitRedefinitionError("Variant", Seen[Variant.getId()], &Variant);
      Success = false;
    }
    Seen[Variant.getId()] = &Variant;

    Success = visit(&Variant) && Success;
  }

  return Success;
}

bool NameResolver::visit(VariantDecl *D) {
  // typeless variants
  if (!D->hasType()) {
    return true;
  }

  return visit(D->getType());
}

} // namespace phi
