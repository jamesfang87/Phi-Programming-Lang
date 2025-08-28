#include "Sema/SymbolTable.hpp"
#include "AST/Decl.hpp"
#include "AST/Expr.hpp"
#include "llvm/Support/Casting.h"
#include <string>

namespace phi {

/**
 * Enters a new scope in the symbol table.
 *
 * Scopes are implemented as stack of hash maps.
 * Each scope corresponds to a lexical block (function, if, for, etc.).
 */
void SymbolTable::enterScope() { Scopes.emplace_back(); }

/**
 * Exits the current scope, discarding all declarations within it.
 *
 * Automatically removes all symbols defined in the current scope.
 */
void SymbolTable::exitScope() { Scopes.pop_back(); }

bool SymbolTable::insert(FunDecl *Fun) {
  for (auto &scope : Scopes) {
    if (scope.Funs.find(Fun->getId()) != scope.Funs.end())
      return false;
  }
  Scopes.back().Funs[Fun->getId()] = Fun;
  return true;
}

bool SymbolTable::insert(StructDecl *Struct) {
  for (auto &scope : Scopes) {
    if (scope.Structs.find(Struct->getId()) != scope.Structs.end())
      return false;
  }
  Scopes.back().Structs[Struct->getId()] = Struct;
  return true;
}

bool SymbolTable::insert(VarDecl *Var) {
  for (auto &scope : Scopes) {
    if (scope.Vars.find(Var->getId()) != scope.Vars.end())
      return false;
  }
  Scopes.back().Vars[Var->getId()] = Var;
  return true;
}

bool SymbolTable::insert(ParamDecl *Param) {
  for (auto &scope : Scopes) {
    if (scope.Vars.find(Param->getId()) != scope.Vars.end())
      return false;
  }
  Scopes.back().Vars[Param->getId()] = Param;
  return true;
}

bool SymbolTable::insert(FieldDecl *Field) {
  for (auto &scope : Scopes) {
    if (scope.Vars.find(Field->getId()) != scope.Vars.end())
      return false;
  }
  Scopes.back().Vars[Field->getId()] = Field;
  return true;
}

ValueDecl *SymbolTable::lookup(DeclRefExpr &Var) {
  for (int i = Scopes.size() - 1; i >= 0; --i) {
    if (auto it = Scopes[i].Vars.find(Var.getId());
        it != Scopes[i].Vars.end()) {
      return it->second;
    }
  }
  return nullptr;
}

FunDecl *SymbolTable::lookup(FunCallExpr &Fun) {
  auto DeclRef = llvm::dyn_cast<DeclRefExpr>(&Fun.getCallee());
  for (int i = Scopes.size() - 1; i >= 0; --i) {
    if (auto it = Scopes[i].Funs.find(DeclRef->getId());
        it != Scopes[i].Funs.end()) {
      return it->second;
    }
  }
  return nullptr;
}

StructDecl *SymbolTable::lookup(const std::string &Id) {
  for (int i = Scopes.size() - 1; i >= 0; --i) {
    if (auto it = Scopes[i].Structs.find(Id); it != Scopes[i].Structs.end()) {
      return it->second;
    }
  }
  return nullptr;
}

// === Lookup overloads by declaration type ===

FunDecl *SymbolTable::lookup(FunDecl &Fun) {
  for (int i = Scopes.size() - 1; i >= 0; --i) {
    if (auto it = Scopes[i].Funs.find(Fun.getId());
        it != Scopes[i].Funs.end()) {
      return it->second;
    }
  }
  return nullptr;
}

StructDecl *SymbolTable::lookup(StructDecl &Struct) {
  for (int i = Scopes.size() - 1; i >= 0; --i) {
    if (auto it = Scopes[i].Structs.find(Struct.getId());
        it != Scopes[i].Structs.end()) {
      return it->second;
    }
  }
  return nullptr;
}

VarDecl *SymbolTable::lookup(VarDecl &Var) {
  for (int i = Scopes.size() - 1; i >= 0; --i) {
    if (auto it = Scopes[i].Vars.find(Var.getId());
        it != Scopes[i].Vars.end()) {
      return llvm::dyn_cast<VarDecl>(it->second);
    }
  }
  return nullptr;
}

ParamDecl *SymbolTable::lookup(ParamDecl &Param) {
  for (int i = Scopes.size() - 1; i >= 0; --i) {
    if (auto it = Scopes[i].Vars.find(Param.getId());
        it != Scopes[i].Vars.end()) {
      return llvm::dyn_cast<ParamDecl>(it->second);
    }
  }
  return nullptr;
}

FieldDecl *SymbolTable::lookup(FieldDecl &Field) {
  for (int i = Scopes.size() - 1; i >= 0; --i) {
    if (auto it = Scopes[i].Vars.find(Field.getId());
        it != Scopes[i].Vars.end()) {
      return llvm::dyn_cast<FieldDecl>(it->second);
    }
  }
  return nullptr;
}

} // namespace phi
