#include "Sema/NameResolver.hpp"

#include <cassert>

#include <llvm/Support/Casting.h>

#include "AST/Decl.hpp"

namespace phi {

bool NameResolver::visit(StructDecl *S) {
  SymbolTable::ScopeGuard StructScope(SymbolTab);
  assert(S);

  bool Success = true;

  for (auto &Field : S->getFields()) {
    Success = visit(Field.get()) && Success;
    SymbolTab.insert(Field.get());
  }

  for (auto &Method : S->getMethods()) {
    Success = visit(&Method) && Success;
    SymbolTab.insert(&Method);
  }

  for (auto &Method : S->getMethods()) {
    CurrentFun = llvm::dyn_cast<FunDecl>(&Method);

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

bool NameResolver::visit(FieldDecl *F) {
  bool Success = resolveType(F->getType());

  // Handle initializer if present
  if (F->hasInit()) {
    Success = visit(F->getInit()) && Success;
  }

  return Success;
}

} // namespace phi
