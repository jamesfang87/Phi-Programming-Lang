#pragma once

#include "Sema/HMTI/HMType.hpp"

#include <optional>
#include <string>
#include <unordered_map>

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
  void bind(const ValueDecl *D, Polytype Sc) { ByDecl_[D] = std::move(Sc); }

  // Rare fallback by name (if a DeclRefExpr hasn't been resolved)
  void bind(std::string Name, Polytype Sc) {
    ByName_.emplace(std::move(Name), std::move(Sc));
  }

  std::optional<Polytype> lookup(const ValueDecl *D) const {
    auto It = ByDecl_.find(D);
    if (It != ByDecl_.end())
      return It->second;
    return std::nullopt;
  }

  std::optional<Polytype> lookup(const std::string &Name) const {
    auto It = ByName_.find(Name);
    if (It != ByName_.end())
      return It->second;
    return std::nullopt;
  }

  // Apply substitution to the whole env (used after unify steps)
  void applySubstitution(const Substitution &S) {
    for (auto &KV : ByDecl_)
      KV.second = S.apply(KV.second);
    for (auto &KV : ByName_)
      KV.second = S.apply(KV.second);
  }

  // Free vars in env (for generalization)
  std::unordered_set<TypeVar, TypeVarHash> freeTypeVars() const {
    std::unordered_set<TypeVar, TypeVarHash> Acc;
    for (const auto &KV : ByDecl_) {
      auto F = phi::freeTypeVars(KV.second);
      Acc.insert(F.begin(), F.end());
    }
    for (const auto &KV : ByName_) {
      auto F = phi::freeTypeVars(KV.second);
      Acc.insert(F.begin(), F.end());
    }
    return Acc;
  }

  // ADL shim for generalize(...) call
  friend std::unordered_set<TypeVar, TypeVarHash>
  freeTypeVars(const TypeEnv &E) {
    return E.freeTypeVars();
  }

private:
  std::unordered_map<const ValueDecl *, Polytype> ByDecl_;
  std::unordered_map<std::string, Polytype> ByName_;
};

} // namespace phi
