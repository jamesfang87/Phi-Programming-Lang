#include "Sema/TypeInference/TypeEnv.hpp"

#include <ranges>

namespace phi {

void TypeEnv::bind(const ValueDecl *D, Polytype P) {
  DeclMap.emplace(D, std::move(P));
}

void TypeEnv::bind(std::string Name, Polytype P) {
  NameMap.emplace(std::move(Name), std::move(P));
}

std::optional<Polytype> TypeEnv::lookup(const ValueDecl *Decl) const {
  const auto It = DeclMap.find(Decl);
  if (It != DeclMap.end())
    return It->second;
  return std::nullopt;
}

[[nodiscard]] std::optional<Polytype>
TypeEnv::lookup(const std::string &Name) const {
  if (const auto It = NameMap.find(Name); It != NameMap.end())
    return It->second;
  return std::nullopt;
}

// Apply substitution to the whole env (used after unify steps)
void TypeEnv::applySubstitution(const Substitution &S) {
  for (auto &Decl : DeclMap | std::views::values)
    Decl = S.apply(Decl);
  for (auto &Name : NameMap | std::views::values)
    Name = S.apply(Name);
}

// Free vars in env (for generalization)
[[nodiscard]] std::unordered_set<TypeVar> TypeEnv::freeTypeVars() const {
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

} // namespace phi
