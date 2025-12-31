#pragma once

#include <unordered_map>

#include "AST/TypeSystem/Type.hpp"

namespace phi {

// ---------------------------
// Substitution
// ---------------------------
class Substitution {
private:
  std::unordered_map<VarTy *, TypeRef> Map;

public:
  Substitution() {}
  Substitution(std::unordered_map<VarTy *, TypeRef> Map)
      : Map(std::move(Map)) {};
  Substitution(VarTy *Var, TypeRef T) : Map({{Var, T}}) {};
  template <typename... Subs> Substitution(const Subs &...subs) {
    (compose(subs), ...); // fold expression in order
  }

  [[nodiscard]] bool empty() const { return Map.empty(); }
  [[nodiscard]] TypeRef apply(const TypeRef &T) const;
  void add(VarTy *Var, TypeRef T);
  void compose(const Substitution &Other);
};

} // namespace phi
