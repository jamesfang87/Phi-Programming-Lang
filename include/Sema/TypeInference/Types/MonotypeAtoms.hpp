#pragma once

#include "Diagnostics/Diagnostic.hpp"
#include <expected>
#include <memory>
#include <optional>
#include <string>
#include <vector>

namespace phi {

class Monotype;

struct TypeVar {
  int Id;
  std::optional<std::vector<std::string>> Constraints;

  bool operator==(const TypeVar &Rhs) const { return Id == Rhs.Id; }

  [[nodiscard]] bool occursIn(const Monotype &M) const;
  [[nodiscard]] std::expected<class Substitution, Diagnostic>
  bindWith(const Monotype &M) const;
};

struct TypeCon {
  std::string Name;
  std::vector<Monotype> Args;

  bool operator==(const TypeCon &Rhs) const;
  bool operator!=(const TypeCon &Rhs) const;
};

struct TypeApp {
  std::string Name;
  std::vector<Monotype> Args;

  bool operator==(const TypeApp &Rhs) const;
  bool operator!=(const TypeApp &Rhs) const;
};

struct TypeFun {
  std::vector<Monotype> Params;
  std::shared_ptr<Monotype> Ret;

  bool operator==(const TypeFun &Rhs) const;
  bool operator!=(const TypeFun &Rhs) const;
};

} // namespace phi

template <> struct std::hash<phi::TypeVar> {
  size_t operator()(const phi::TypeVar &V) const noexcept {
    return std::hash<int>{}(V.Id);
  }
}; // namespace std
