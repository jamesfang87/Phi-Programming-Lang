#pragma once

#include <optional>
#include <string>
#include <unordered_map>
#include <unordered_set>

#include "Sema/HMTI/Substitution.hpp"
#include "Sema/HMTI/Types/Polytype.hpp"

namespace phi {

class ValueDecl; // from your AST
class Type;

//------------------------------------------------------------------------------
// TypeEnv: methods on a class (OOP), not a raw struct
//------------------------------------------------------------------------------

class TypeEnv {
public:
  // Bind by declaration pointer (preferred: your DeclRefExpr should carry a
  // decl*)
  void bind(const ValueDecl *D, Polytype Sc) {
    DeclMap.emplace(D, std::move(Sc));
  }

  // Rare fallback by name (if a DeclRefExpr hasn't been resolved)
  void bind(std::string Name, Polytype Sc) {
    NameMap.emplace(std::move(Name), std::move(Sc));
  }

  std::optional<Polytype> lookup(const ValueDecl *D) const {
    auto It = DeclMap.find(D);
    if (It != DeclMap.end())
      return It->second;
    return std::nullopt;
  }

  std::optional<Polytype> lookup(const std::string &Name) const {
    auto It = NameMap.find(Name);
    if (It != NameMap.end())
      return It->second;
    return std::nullopt;
  }

  // Apply substitution to the whole env (used after unify steps)
  void applySubstitution(const Substitution &S) {
    for (auto &KV : DeclMap)
      KV.second = S.apply(KV.second);
    for (auto &KV : NameMap)
      KV.second = S.apply(KV.second);
  }

  // Free vars in env (for generalization)
  std::unordered_set<TypeVar> freeTypeVars() const {
    std::unordered_set<TypeVar> Acc;
    for (const auto &KV : DeclMap) {
      auto F = KV.second.freeTypeVars();
      Acc.insert(F.begin(), F.end());
    }
    for (const auto &KV : NameMap) {
      auto F = KV.second.freeTypeVars();
      Acc.insert(F.begin(), F.end());
    }
    return Acc;
  }

private:
  std::unordered_map<const ValueDecl *, Polytype> DeclMap;
  std::unordered_map<std::string, Polytype> NameMap;
};

} // namespace phi
