#pragma once

#include <unordered_map>

#include "AST/TypeSystem/Type.hpp"

namespace phi {

// ---------------------------
// Substitution
// ---------------------------
struct Substitution {
  std::unordered_map<VarTy *, TypeRef> Map;

  [[nodiscard]] bool empty() const { return Map.empty(); }
  [[nodiscard]] TypeRef apply(const TypeRef &T) const;
  void compose(const Substitution &Other);
};

} // namespace phi
