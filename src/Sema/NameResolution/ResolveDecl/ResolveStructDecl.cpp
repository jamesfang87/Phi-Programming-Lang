#include "Sema/NameResolver.hpp"

#include <cassert>

#include <llvm/Support/Casting.h>

#include "AST/Decl.hpp"

namespace phi {

bool NameResolver::visit(StructDecl *Struct) {
  SymbolTable::ScopeGuard StructScope(SymbolTab);
  assert(Struct);

  bool Success = true;

  for (auto &Field : Struct->getFields()) {
    Success = visit(&Field) && Success;
    SymbolTab.insert(&Field);
  }

  for (auto &Method : Struct->getMethods()) {
    Success = visit(&Method) && Success;
    SymbolTab.insert(&Method);
  }

  for (auto &Method : Struct->getMethods()) {
    CurFun = llvm::dyn_cast<FunDecl>(&Method);

    // Create function scope
    SymbolTable::ScopeGuard FunctionScope(SymbolTab);

    // Add parameters to function scope
    for (const std::unique_ptr<ParamDecl> &param : Method.getParams()) {
      if (!SymbolTab.insert(param.get())) {
        emitRedefinitionError("Parameter", SymbolTab.lookup(*param),
                              param.get());
      }
    }

    Success = resolveBlock(Method.getBody(), true) && Success;
  }

  return Success;
}

bool NameResolver::visit(FieldDecl *Field) {
  bool Success = resolveType(Field->getType());

  // Handle initializer if present
  if (Field->hasInit()) {
    Success = visit(Field->getInit()) && Success;
  }

  return Success;
}

} // namespace phi
