#pragma once

#include <unordered_map>

#include "Sema/TypeInference/Types/Monotype.hpp"
#include "Sema/TypeInference/Types/Polytype.hpp"

namespace phi {

// ---------------------------
// Substitution
// ---------------------------
struct Substitution {
  // v ↦ type
  std::unordered_map<TypeVar, Monotype> Map;

  [[nodiscard]] bool empty() const { return Map.empty(); }
  [[nodiscard]] Monotype apply(const Monotype &M) const;
  [[nodiscard]] Polytype apply(const Polytype &P) const;
  void compose(const Substitution &Other);
};

} // namespace phi
