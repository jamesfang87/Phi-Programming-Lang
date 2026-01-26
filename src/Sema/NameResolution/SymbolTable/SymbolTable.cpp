#include "Sema/NameResolution/SymbolTable.hpp"

#include <llvm/ADT/TypeSwitch.h>
#include <ranges>
#include <string>

#include <llvm/Support/Casting.h>
#include <utility>

#include "AST/Nodes/Decl.hpp"
#include "AST/Nodes/Expr.hpp"
#include "AST/Nodes/Stmt.hpp"

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

bool SymbolTable::insertAsImportable(ModuleDecl *Mod) {
  if (ImportableItems.contains(Mod->getId())) {
    return false;
  }

  ImportableItems[Mod->getId()] = Mod;
  return true;
}

bool SymbolTable::insertAsImportable(ItemDecl *Item, ModuleDecl *ParentMod) {
  auto CanonicalId = ParentMod->getId() + "::" + Item->getId();
  if (ImportableItems.contains(CanonicalId)) {
    return false;
  }

  ImportableItems[CanonicalId] = Item;
  return true;
}

bool SymbolTable::insert(ItemDecl *Item) {
  assert(!llvm::isa<ModuleDecl>(Item) &&
         "Do not use insert for a ModuleDecl; they cannot be referenced other "
         "than being imported. In that case, use insertAsImportable");

  if (auto *Fun = llvm::dyn_cast<FunDecl>(Item)) {
    return insert(Fun);
  }

  if (auto *Adt = llvm::dyn_cast<AdtDecl>(Item)) {
    return insert(Adt);
  }

  std::unreachable();
  return false;
}

bool SymbolTable::insert(FunDecl *Fun) {
  for (auto &Scope : Scopes) {
    if (Scope.Funs.find(Fun->getId()) != Scope.Funs.end())
      return false;
  }
  Scopes.back().Funs[Fun->getId()] = Fun;
  return true;
}

bool SymbolTable::insert(AdtDecl *Struct) {
  for (auto &Scope : Scopes) {
    if (Scope.Adts.contains(Struct->getId()))
      return false;
  }
  Scopes.back().Adts[Struct->getId()] = Struct;
  return true;
}

bool SymbolTable::insert(LocalDecl *Var) {
  for (auto &Scope : Scopes) {
    if (Scope.Vars.find(Var->getId()) != Scope.Vars.end())
      return false;
  }
  Scopes.back().Vars[Var->getId()] = Var;
  return true;
}

bool SymbolTable::insert(MemberDecl *Field) {
  for (auto &Scope : Scopes) {
    if (Scope.Mems.find(Field->getId()) != Scope.Mems.end())
      return false;
  }
  Scopes.back().Mems[Field->getId()] = Field;
  return true;
}

bool SymbolTable::insertWithQual(ItemDecl *Item, const std::string &Qual) {
  assert(!llvm::isa<ModuleDecl>(Item) &&
         "Do not use insertWithQual for a ModuleDecl; they cannot be "
         "referenced other "
         "than being imported. In that case, use insertAsImportable");

  std::string QualifiedName = std::format("{}::{}", Qual, Item->getId());
  if (auto *Fun = llvm::dyn_cast<FunDecl>(Item)) {
    for (auto &Scope : Scopes) {
      if (Scope.Funs.find(QualifiedName) != Scope.Funs.end())
        return false;
    }
    Scopes.back().Funs[QualifiedName] = Fun;
    return true;
  }

  if (auto *Adt = llvm::dyn_cast<AdtDecl>(Item)) {
    for (auto &Scope : Scopes) {
      if (Scope.Adts.find(QualifiedName) != Scope.Adts.end())
        return false;
    }
    Scopes.back().Adts[QualifiedName] = Adt;
    return true;
  }

  std::unreachable();
  return false;
}

LocalDecl *SymbolTable::lookup(DeclRefExpr &Var) {
  for (auto &Scope : std::ranges::reverse_view(Scopes)) {
    if (auto It = Scope.Vars.find(Var.getId()); It != Scope.Vars.end()) {
      return It->second;
    }
  }
  return nullptr;
}

FunDecl *SymbolTable::lookup(FunCallExpr &Fun) {
  auto *DeclRef = llvm::dyn_cast<DeclRefExpr>(&Fun.getCallee());
  for (auto &Scope : std::ranges::reverse_view(Scopes)) {
    if (auto It = Scope.Funs.find(DeclRef->getId()); It != Scope.Funs.end()) {
      return It->second;
    }
  }
  return nullptr;
}

AdtDecl *SymbolTable::lookup(const std::string &Id) {
  for (auto &Scope : std::ranges::reverse_view(Scopes)) {
    if (auto It = Scope.Adts.find(Id); It != Scope.Adts.end()) {
      return It->second;
    }
  }
  return nullptr;
}

// === Lookup overloads by declaration type ===

FunDecl *SymbolTable::lookup(FunDecl &Fun) {
  for (auto &Scope : std::ranges::reverse_view(Scopes)) {
    if (auto It = Scope.Funs.find(Fun.getId()); It != Scope.Funs.end()) {
      return It->second;
    }
  }
  return nullptr;
}

StructDecl *SymbolTable::lookup(StructDecl &Struct) {
  return llvm::dyn_cast<StructDecl>(lookup(Struct.getId()));
}

EnumDecl *SymbolTable::lookup(EnumDecl &Enum) {
  return llvm::dyn_cast<EnumDecl>(lookup(Enum.getId()));
}

VarDecl *SymbolTable::lookup(VarDecl &Var) {
  for (auto &Scope : std::ranges::reverse_view(Scopes)) {
    if (auto It = Scope.Vars.find(Var.getId()); It != Scope.Vars.end()) {
      return llvm::dyn_cast<VarDecl>(It->second);
    }
  }
  return nullptr;
}

ParamDecl *SymbolTable::lookup(ParamDecl &Param) {
  for (auto &Scope : std::ranges::reverse_view(Scopes)) {
    if (auto It = Scope.Vars.find(Param.getId()); It != Scope.Vars.end()) {
      return llvm::dyn_cast<ParamDecl>(It->second);
    }
  }
  return nullptr;
}

FieldDecl *SymbolTable::lookup(FieldDecl &Field) {
  for (auto &Scope : std::ranges::reverse_view(Scopes)) {
    if (auto It = Scope.Vars.find(Field.getId()); It != Scope.Vars.end()) {
      return llvm::dyn_cast<FieldDecl>(It->second);
    }
  }
  return nullptr;
}

ItemDecl *SymbolTable::lookupImport(const std::string &Id) {
  if (auto It = ImportableItems.find(Id); It != ImportableItems.end()) {
    return It->second;
  }
  return nullptr;
}

} // namespace phi
