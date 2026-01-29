#include "Sema/NameResolution/NameResolver.hpp"

#include <llvm/Support/Casting.h>

#include <cassert>
#include <variant>

#include "AST/Nodes/Decl.hpp"
#include "Diagnostics/DiagnosticBuilder.hpp"

namespace phi {

bool NameResolver::resolveHeader(ItemDecl &D) {
  if (auto *Adt = llvm::dyn_cast<AdtDecl>(&D)) {
    return resolveHeader(*Adt);
  }

  if (auto *Fun = llvm::dyn_cast<FunDecl>(&D)) {
    return resolveHeader(*Fun);
  }

  static_assert("ModuleDecl not yet supported");
  return false;
}

bool NameResolver::resolveBodies(ItemDecl &D) {
  if (auto *Struct = llvm::dyn_cast<StructDecl>(&D)) {
    return visit(Struct);
  }

  if (auto *Enum = llvm::dyn_cast<EnumDecl>(&D)) {
    return visit(Enum);
  }

  if (auto *Fun = llvm::dyn_cast<FunDecl>(&D)) {
    return visit(Fun);
  }

  static_assert("ModuleDecl not yet supported");
  return true;
}

bool NameResolver::resolveHeader(FunDecl &D) {
  if (!SymbolTab.insert(&D)) {
    emitRedefinitionError("Function", SymbolTab.lookup(D), &D);
    return false;
  }

  return true;
}

bool NameResolver::resolveHeader(AdtDecl &D) {
  if (!SymbolTab.insert(&D)) {
    emitRedefinitionError("Custom type", SymbolTab.lookup(D), &D);
    return false;
  }
  assert(SymbolTab.lookup(D));

  visit(D.getType());
  return true;
}

bool NameResolver::resolveHeader(MethodDecl &D) {
  if (!SymbolTab.insert(&D)) {
    emitRedefinitionError("Method", SymbolTab.lookup(D), &D);
    return false;
  }

  return true;
}

bool NameResolver::visit(FunDecl *D) {
  assert(D);
  CurrentFun = D;
  if (std::holds_alternative<std::monostate>(CurrentFun)) {
    return false;
  }

  // Create function scope
  SymbolTable::ScopeGuard FunctionScope(SymbolTab);

  bool Success = visit(D->getReturnType());

  for (const auto &TypeArg : D->getTypeArgs()) {
    SymbolTab.insert(TypeArg.get());
  }

  for (const auto &Param : D->getParams()) {
    Success = visit(Param.get()) && Success;

    if (!SymbolTab.insert(Param.get())) {
      emitRedefinitionError("Parameter", SymbolTab.lookup(*Param), Param.get());
      Success = false;
    }
  }

  // Resolve function body
  return visit(D->getBody(), true) && Success;
}

bool NameResolver::visit(ParamDecl *D) { return visit(D->getType()); }

bool NameResolver::visit(MethodDecl *D) {
  assert(D);
  CurrentFun = D;
  if (std::holds_alternative<std::monostate>(CurrentFun)) {
    return false;
  }

  // Create function scope
  SymbolTable::ScopeGuard FunctionScope(SymbolTab);

  bool Success = visit(D->getReturnType());

  for (const auto &TypeArg : D->getTypeArgs()) {
    SymbolTab.insert(TypeArg.get());
  }

  for (const auto &Param : D->getParams()) {
    Success = visit(Param.get()) && Success;

    if (!SymbolTab.insert(Param.get())) {
      emitRedefinitionError("Parameter", SymbolTab.lookup(*Param), Param.get());
      Success = false;
    }
  }

  // Resolve function body
  return visit(D->getBody(), true) && Success;
}

bool NameResolver::visit(StructDecl *D) {
  assert(D);

  SymbolTable::ScopeGuard StructScope(SymbolTab);

  bool Success = visit(D->getType());

  for (const auto &TypeArg : D->getTypeArgs()) {
    if (!SymbolTab.insert(TypeArg.get())) {
      error(std::format("Redefinition of type argument `{}`", TypeArg->getId()))
          .with_primary_label(TypeArg->getSpan(), "here")
          .emit(*Diags);
      Success = false;
    }
  }

  for (auto &Field : D->getFields()) {
    if (!SymbolTab.insert(Field.get())) {
      emitRedefinitionError("Field", SymbolTab.lookup(*Field), Field.get());
      Success = false;
    }

    Success = visit(Field.get()) && Success;
  }

  for (auto &Method : D->getMethods()) {
    Success = resolveHeader(*Method) && Success;
  }

  for (auto &Method : D->getMethods()) {
    Success = visit(Method.get()) && Success;
  }

  return Success;
}

bool NameResolver::visit(FieldDecl *D) {
  bool Success = true;

  // Handle initializer if present
  if (D->hasInit()) {
    Success = visit(D->getInit()) && Success;
  }

  Success = visit(D->getType()) && Success;

  return Success;
}

bool NameResolver::visit(EnumDecl *D) {
  assert(D);

  SymbolTable::ScopeGuard EnumScope(SymbolTab);

  bool Success = visit(D->getType());

  for (const auto &TypeArg : D->getTypeArgs()) {
    if (!SymbolTab.insert(TypeArg.get())) {
      error(std::format("Redefinition of type argument `{}`", TypeArg->getId()))
          .with_primary_label(TypeArg->getSpan(), "here")
          .emit(*Diags);
      Success = false;
    }
  }

  for (auto &Variant : D->getVariants()) {
    if (!SymbolTab.insert(Variant.get())) {
      emitRedefinitionError("Variant", SymbolTab.lookup(*Variant),
                            Variant.get());
      Success = false;
    }

    Success = visit(Variant.get()) && Success;
  }

  for (auto &Method : D->getMethods()) {
    Success = resolveHeader(*Method) && Success;
  }

  for (auto &Method : D->getMethods()) {
    Success = visit(Method.get()) && Success;
  }

  return Success;
}

bool NameResolver::visit(VariantDecl *D) {
  if (D->hasPayload()) {
    return visit(D->getPayloadType());
  }
  return true;
}

} // namespace phi
