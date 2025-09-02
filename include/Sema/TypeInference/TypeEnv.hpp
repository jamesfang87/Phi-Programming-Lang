#pragma once

#include <optional>
#include <ranges>
#include <string>
#include <unordered_map>
#include <unordered_set>

#include "Sema/TypeInference/Substitution.hpp"
#include "Sema/TypeInference/Types/Polytype.hpp"

namespace phi {

class ValueDecl;
class Type;

//------------------------------------------------------------------------------
// TypeEnv: methods on a class (OOP), not a raw struct
//------------------------------------------------------------------------------

class TypeEnv {
public:
  // Bind by declaration pointer (preferred: your DeclRefExpr should carry a
  // decl*)
  void bind(const ValueDecl *D, Polytype P) {
    DeclMap.emplace(D, std::move(P));
  }

  // Rare fallback by name (if a DeclRefExpr hasn't been resolved)
  void bind(std::string Name, Polytype P) {
    NameMap.emplace(std::move(Name), std::move(P));
  }

  std::optional<Polytype> lookup(const ValueDecl *Decl) const {
    const auto It = DeclMap.find(Decl);
    if (It != DeclMap.end())
      return It->second;
    return std::nullopt;
  }

  [[nodiscard]] std::optional<Polytype> lookup(const std::string &Name) const {
    if (const auto It = NameMap.find(Name); It != NameMap.end())
      return It->second;
    return std::nullopt;
  }

  // Apply substitution to the whole env (used after unify steps)
  void applySubstitution(const Substitution &S) {
    for (auto &Decl : DeclMap | std::views::values)
      Decl = S.apply(Decl);
    for (auto &Name : NameMap | std::views::values)
      Name = S.apply(Name);
  }

  // Free vars in env (for generalization)
  [[nodiscard]] std::unordered_set<TypeVar> freeTypeVars() const {
    std::unordered_set<TypeVar> Acc;
    for (const auto &Decl : DeclMap | std::views::values) {
      auto F = Decl.freeTypeVars();
      Acc.insert(F.begin(), F.end());
    }
    for (const auto &Name : NameMap | std::views::values) {
      auto F = Name.freeTypeVars();
      Acc.insert(F.begin(), F.end());
    }
    return Acc;
  }

private:
  std::unordered_map<const ValueDecl *, Polytype> DeclMap;
  std::unordered_map<std::string, Polytype> NameMap;
};

} // namespace phi
