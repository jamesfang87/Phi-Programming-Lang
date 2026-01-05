#include "Sema/NameResolution/NameResolver.hpp"

#include <cassert>
#include <string>
#include <unordered_map>

namespace phi {

bool NameResolver::resolveHeader(Decl &D) {
  if (auto *Struct = llvm::dyn_cast<StructDecl>(&D)) {
    return resolveHeader(*Struct);
  }

  if (auto *Enum = llvm::dyn_cast<EnumDecl>(&D)) {
    return resolveHeader(*Enum);
  }

  if (auto *Fun = llvm::dyn_cast<FunDecl>(&D)) {
    return resolveHeader(*Fun);
  }

  return true;
}

bool NameResolver::resolveBodies(Decl &D) {
  if (auto *Struct = llvm::dyn_cast<StructDecl>(&D)) {
    return visit(Struct);
  }

  if (auto *Enum = llvm::dyn_cast<EnumDecl>(&D)) {
    return visit(Enum);
  }

  if (auto *Fun = llvm::dyn_cast<FunDecl>(&D)) {
    return visit(Fun);
  }

  return true;
}

bool NameResolver::resolveHeader(FunDecl &D) {
  bool Success = visit(D.getReturnTy()); // we visit param types later

  // Resolve parameters
  for (const auto &Param : D.getParams()) {
    Success = visit(Param.get()) && Success;
  }

  if (!SymbolTab.insert(&D)) {
    emitRedefinitionError("Function", SymbolTab.lookup(D), &D);
  }

  return Success;
}

bool NameResolver::resolveHeader(StructDecl &D) {
  if (!SymbolTab.insert(&D)) {
    emitRedefinitionError("Struct", SymbolTab.lookup(D), &D);
    return false;
  }
  assert(SymbolTab.lookup(D));

  return visit(D.getType());
}

bool NameResolver::resolveHeader(EnumDecl &D) {
  if (!SymbolTab.insert(&D)) {
    emitRedefinitionError("Enum", SymbolTab.lookup(D), &D);
    return false;
  }
  assert(SymbolTab.lookup(D));

  return visit(D.getType());
}

bool NameResolver::visit(FunDecl *D) {
  CurrentFun = D;
  if (!CurrentFun) {
    return false;
  }

  // Create function scope
  SymbolTable::ScopeGuard FunctionScope(SymbolTab);

  // Add parameters to function scope
  bool Success = true;
  for (const auto &Param : CurrentFun->getParams()) {
    if (!SymbolTab.insert(Param.get())) {
      emitRedefinitionError("Parameter", SymbolTab.lookup(*Param), Param.get());
      Success = false;
    }
  }

  // Resolve function body
  return visit(CurrentFun->getBody(), true) && Success;
}

bool NameResolver::visit(ParamDecl *D) { return visit(D->getType()); }

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
