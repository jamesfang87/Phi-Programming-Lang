#include "Sema/SymbolTable.hpp"

#include <string>

#include <llvm/Support/Casting.h>

#include "AST/Decl.hpp"
#include "AST/Expr.hpp"

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
  for (auto &Scope : Scopes) {
    if (Scope.Funs.find(Fun->getId()) != Scope.Funs.end())
      return false;
  }
  Scopes.back().Funs[Fun->getId()] = Fun;
  return true;
}

bool SymbolTable::insert(StructDecl *Struct) {
  for (auto &Scope : Scopes) {
    if (Scope.Structs.find(Struct->getId()) != Scope.Structs.end() ||
        Scope.Enums.find(Struct->getId()) != Scope.Enums.end())
      return false;
  }
  Scopes.back().Structs[Struct->getId()] = Struct;
  return true;
}

bool SymbolTable::insert(EnumDecl *Enum) {
  for (auto &Scope : Scopes) {
    if (Scope.Structs.find(Enum->getId()) != Scope.Structs.end() ||
        Scope.Enums.find(Enum->getId()) != Scope.Enums.end())
      return false;
  }
  Scopes.back().Enums[Enum->getId()] = Enum;
  return true;
}

bool SymbolTable::insert(VarDecl *Var) {
  for (auto &Scope : Scopes) {
    if (Scope.Vars.find(Var->getId()) != Scope.Vars.end())
      return false;
  }
  Scopes.back().Vars[Var->getId()] = Var;
  return true;
}

bool SymbolTable::insert(ParamDecl *Param) {
  for (auto &Scope : Scopes) {
    if (Scope.Vars.find(Param->getId()) != Scope.Vars.end())
      return false;
  }
  Scopes.back().Vars[Param->getId()] = Param;
  return true;
}

bool SymbolTable::insert(FieldDecl *Field) {
  for (auto &Scope : Scopes) {
    if (Scope.Vars.find(Field->getId()) != Scope.Vars.end())
      return false;
  }
  Scopes.back().Vars[Field->getId()] = Field;
  return true;
}

ValueDecl *SymbolTable::lookup(DeclRefExpr &Var) {
  for (int I = Scopes.size() - 1; I >= 0; --I) {
    if (auto It = Scopes[I].Vars.find(Var.getId());
        It != Scopes[I].Vars.end()) {
      return It->second;
    }
  }
  return nullptr;
}

FunDecl *SymbolTable::lookup(FunCallExpr &Fun) {
  auto DeclRef = llvm::dyn_cast<DeclRefExpr>(&Fun.getCallee());
  for (int I = Scopes.size() - 1; I >= 0; --I) {
    if (auto It = Scopes[I].Funs.find(DeclRef->getId());
        It != Scopes[I].Funs.end()) {
      return It->second;
    }
  }
  return nullptr;
}

StructDecl *SymbolTable::lookupStruct(const std::string &Id) {
  for (int I = Scopes.size() - 1; I >= 0; --I) {
    if (auto It = Scopes[I].Structs.find(Id); It != Scopes[I].Structs.end()) {
      return It->second;
    }
  }
  return nullptr;
}

EnumDecl *SymbolTable::lookupEnum(const std::string &Id) {
  for (int I = Scopes.size() - 1; I >= 0; --I) {
    if (auto It = Scopes[I].Enums.find(Id); It != Scopes[I].Enums.end()) {
      return It->second;
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

EnumDecl *SymbolTable::lookup(EnumDecl &Enum) {
  for (int i = Scopes.size() - 1; i >= 0; --i) {
    if (auto it = Scopes[i].Enums.find(Enum.getId());
        it != Scopes[i].Enums.end()) {
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
