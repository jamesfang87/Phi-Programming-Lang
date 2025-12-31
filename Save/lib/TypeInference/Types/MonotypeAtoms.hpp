#pragma once

#include "AST/TypeSystem/Type.hpp"
#include "Diagnostics/Diagnostic.hpp"
#include <cstdint>
#include <expected>
#include <memory>
#include <optional>
#include <string>
#include <variant>
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
  struct StructType {
    std::string Id;
    class StructDecl *D;
    bool operator==(const StructType &Rhs) const { return Id == Rhs.Id; };
  };

  struct EnumType {
    std::string Id;
    class EnumDecl *D;
    bool operator==(const EnumType &Rhs) const { return Id == Rhs.Id; };
  };

  std::variant<BuiltinTy::Kind, StructType, EnumType> Data;
  std::string StringRep;

  bool operator==(const TypeCon &Rhs) const;
  bool operator!=(const TypeCon &Rhs) const;
};

struct TypeApp {
  enum BuiltinKind : uint8_t {
    Ref,
    Ptr,
    Tuple,
    Range,
  };

  struct CustomKind {
    std::string Id;
    bool operator==(const CustomKind &Rhs) const { return Id == Rhs.Id; };
  };

  std::variant<BuiltinKind, CustomKind> AppKind;
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
