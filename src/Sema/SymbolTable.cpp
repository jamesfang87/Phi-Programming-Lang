#include "Sema/SymbolTable.hpp"
#include "AST/Decl.hpp"
#include "AST/Expr.hpp"
#include "llvm/Support/Casting.h"

namespace phi {

/**
 * Enters a new scope in the symbol table.
 *
 * Scopes are implemented as stack of hash maps.
 * Each scope corresponds to a lexical block (function, if, for, etc.).
 */
void SymbolTable::enterScope() { scopes.emplace_back(); }

/**
 * Exits the current scope, discarding all declarations within it.
 *
 * Automatically removes all symbols defined in the current scope.
 */
void SymbolTable::exitScope() { scopes.pop_back(); }

bool SymbolTable::insert(FunDecl *fun) {
  scopes.back().funs[fun->getId()] = fun;
  return true;
}

bool SymbolTable::insert(Decl *decl) {
  // not implemented
  return false;
}

bool SymbolTable::insert(VarDecl *decl) {
  scopes.back().vars[decl->getId()] = decl;
  return true;
}

bool SymbolTable::insert(ParamDecl *decl) {
  scopes.back().vars[decl->getId()] = decl;
  return true;
}

Decl *SymbolTable::lookup(DeclRefExpr &declref) {
  for (int i = scopes.size() - 1; i >= 0; --i) {
    if (auto it = scopes[i].vars.find(declref.getID());
        it != scopes[i].vars.end()) {
      return it->second;
    }
  }
  return nullptr;
}

FunDecl *SymbolTable::lookup(FunCallExpr &fun) {
  auto declref = llvm::dyn_cast<DeclRefExpr>(&fun.getCallee());
  for (int i = scopes.size() - 1; i >= 0; --i) {
    if (auto it = scopes[i].funs.find(declref->getID());
        it != scopes[i].funs.end()) {
      return it->second;
    }
  }
  return nullptr;
}

} // namespace phi
