#pragma once

#include <optional>
#include <vector>

namespace phi {

class Monotype;

struct TypeVar {
  int Id;
  std::optional<std::vector<std::string>> Constraints;

  bool operator==(const TypeVar &Rhs) const { return Id == Rhs.Id; }
};

struct TypeCon {
  std::string name;
  std::vector<Monotype> args;
};

struct TypeFun {
  std::vector<Monotype> params;
  std::shared_ptr<Monotype> ret;
};

} // namespace phi

namespace std {
template <> struct hash<phi::TypeVar> {
  size_t operator()(const phi::TypeVar &V) const noexcept {
    return std::hash<int>{}(V.Id);
  }
};
} // namespace std
