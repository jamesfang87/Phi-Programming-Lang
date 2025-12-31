#pragma once

#include <optional>
#include <string>
#include <unordered_map>
#include <unordered_set>

#include "Sema/TypeInference/Substitution.hpp"
#include "Sema/TypeInference/Types/Polytype.hpp"

namespace phi {

class ValueDecl;
class Type;

class TypeEnv {
public:
  void bind(const ValueDecl *D, Polytype P);
  void bind(std::string Name, Polytype P);
  void applySubstitution(const Substitution &S);

  [[nodiscard]] std::optional<Polytype> lookup(const ValueDecl *Decl) const;
  [[nodiscard]] std::optional<Polytype> lookup(const std::string &Name) const;
  [[nodiscard]] std::unordered_set<TypeVar> freeTypeVars() const;

private:
  std::unordered_map<const ValueDecl *, Polytype> DeclMap;
  std::unordered_map<std::string, Polytype> NameMap;
};

} // namespace phi
