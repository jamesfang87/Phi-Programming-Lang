#include "Sema/NameResolver.hpp"

#include <cassert>
#include <print>
#include <string>
#include <unordered_map>

#include "AST/Decl.hpp"

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

  std::unordered_map<std::string, MethodDecl *> SeenMethods;
  for (auto &Method : D->getMethods()) {
    if (SeenMethods.contains(Method.getId())) {
      assert(SeenMethods[Method.getId()]);
      emitRedefinitionError("Method", SeenMethods[Method.getId()], &Method);
      Success = false;
    }
    SeenMethods[Method.getId()] = &Method;

    Success = visit(&Method) && Success;
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
