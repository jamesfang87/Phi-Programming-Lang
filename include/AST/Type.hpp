#pragma once

#include <cassert>
#include <cstdint>
#include <format>
#include <sstream>
#include <string>
#include <utility>
#include <variant>
#include <vector>

#include "SrcManager/SrcLocation.hpp"

namespace phi {

//===----------------------------------------------------------------------===//
// Type Algebra
//   - PrimitiveKind
//   - TypeVariable (e.g., T, U)
//   - CustomType (nominal)
//   - ReferenceType (&T), PointerType (*T)
//   - GenericType  (Vector<T>, Map<K,V>)
//   - FunctionType (fn(A,B)->C)
//   - Type: variant wrapper + helpers/unification
//===----------------------------------------------------------------------===//

enum class PrimitiveKind : uint8_t {
  I8,
  I16,
  I32,
  I64,
  U8,
  U16,
  U32,
  U64,
  F32,
  F64,
  String,
  Char,
  Bool,
  Range,
  Null
};

inline std::string primitiveKindToString(PrimitiveKind Kind) {
  switch (Kind) {
  case PrimitiveKind::I8:
    return "i8";
  case PrimitiveKind::I16:
    return "i16";
  case PrimitiveKind::I32:
    return "i32";
  case PrimitiveKind::I64:
    return "i64";
  case PrimitiveKind::U8:
    return "u8";
  case PrimitiveKind::U16:
    return "u16";
  case PrimitiveKind::U32:
    return "u32";
  case PrimitiveKind::U64:
    return "u64";
  case PrimitiveKind::F32:
    return "f32";
  case PrimitiveKind::F64:
    return "f64";
  case PrimitiveKind::String:
    return "string";
  case PrimitiveKind::Char:
    return "char";
  case PrimitiveKind::Bool:
    return "bool";
  case PrimitiveKind::Range:
    return "range";
  case PrimitiveKind::Null:
    return "null";
  }
  return "unknown";
}

struct CustomType {
  std::string Name;
  bool operator==(const CustomType &Other) const noexcept {
    return Name == Other.Name;
  }
};

struct ReferenceType; // fwd
struct PointerType;   // fwd
struct GenericType;   // fwd
struct FunctionType;  // fwd

// Wrapper Type forward-declared so inner nodes can use shared_ptr<Type>.
class Type;

using TypePtr = std::shared_ptr<Type>;

struct ReferenceType {
  TypePtr Pointee;
  bool operator==(const ReferenceType &Other) const noexcept;
};

struct PointerType {
  TypePtr Pointee;
  bool operator==(const PointerType &Other) const noexcept;
};

struct GenericType {
  std::string Name;                // e.g., "Vector", "Set", "Map"
  std::vector<Type> TypeArguments; // Vector<T> => ["T"], Map<K,V> => ["K","V"]
  bool operator==(const GenericType &Other) const noexcept;
};

struct FunctionType {
  std::vector<Type> Parameters;
  TypePtr ReturnType;
  bool operator==(const FunctionType &Other) const noexcept;
};

class Type {
public:
  using Node = std::variant<PrimitiveKind, CustomType, ReferenceType,
                            PointerType, GenericType, FunctionType>;

  Type() = default;

  // Constructors (factories to keep usage clear)
  static Type makePrimitive(PrimitiveKind K, SrcLocation L) {
    return Type{K, std::move(L)};
  }

  static Type makeCustom(std::string Name, SrcLocation L) {
    return Type(CustomType{std::move(Name)}, std::move(L));
  }

  static Type makeReference(Type Pointee, SrcLocation L) {
    return Type(ReferenceType{std::make_shared<Type>(std::move(Pointee))},
                std::move(L));
  }

  static Type makePointer(Type Pointee, SrcLocation L) {
    return Type(PointerType{std::make_shared<Type>(std::move(Pointee))},
                std::move(L));
  }

  static Type makeGeneric(std::string Name, std::vector<Type> Args,
                          SrcLocation L) {
    return Type(
        GenericType{.Name = std::move(Name), .TypeArguments = std::move(Args)},
        std::move(L));
  }

  static Type makeFunction(std::vector<Type> Params, Type Result,
                           SrcLocation L) {
    return Type(
        FunctionType{.Parameters = std::move(Params),
                     .ReturnType = std::make_shared<Type>(std::move(Result))},
        std::move(L));
  }

  const Node &node() const noexcept { return Data; }

  // Kind checks
  [[nodiscard]] bool isPrimitive() const {
    return std::holds_alternative<PrimitiveKind>(Data);
  }
  [[nodiscard]] bool isCustom() const {
    return std::holds_alternative<CustomType>(Data);
  }

  [[nodiscard]] bool isReference() const {
    return std::holds_alternative<ReferenceType>(Data);
  }

  [[nodiscard]] bool isPointer() const {
    return std::holds_alternative<PointerType>(Data);
  }

  [[nodiscard]] bool isGeneric() const {
    return std::holds_alternative<GenericType>(Data);
  }

  [[nodiscard]] bool isFun() const {
    return std::holds_alternative<FunctionType>(Data);
  }

  [[nodiscard]] PointerType asPtr() const {
    return std::get<PointerType>(Data);
  }

  [[nodiscard]] ReferenceType asRef() const {
    return std::get<ReferenceType>(Data);
  }

  bool operator==(const Type &Other) const noexcept {
    return Data == Other.Data;
  }
  bool operator!=(const Type &Other) const noexcept {
    return !(*this == Other);
  }

  // Rendering for debugging/diagnostics.
  std::string toString() const {
    SrcLocation L = this->Location;
    struct V {
      SrcLocation L;
      std::string operator()(PrimitiveKind K) const {
        return std::format("{}", primitiveKindToString(K));
      }
      std::string operator()(const CustomType &C) const { return C.Name; }
      std::string operator()(const ReferenceType &R) const {
        return "&" + R.Pointee->toString();
      }
      std::string operator()(const PointerType &P) const {
        return "*" + P.Pointee->toString();
      }
      std::string operator()(const GenericType &G) const {
        std::ostringstream Oss;
        Oss << G.Name << "<";
        for (size_t I = 0; I < G.TypeArguments.size(); ++I) {
          Oss << G.TypeArguments[I].toString();
          if (I + 1 < G.TypeArguments.size())
            Oss << ",";
        }
        Oss << ">";
        return Oss.str();
      }
      std::string operator()(const FunctionType &F) const {
        std::ostringstream Oss;
        Oss << "fn(";
        for (size_t I = 0; I < F.Parameters.size(); ++I) {
          Oss << F.Parameters[I].toString();
          if (I + 1 < F.Parameters.size())
            Oss << ",";
        }
        Oss << ") -> " << F.ReturnType->toString();
        return Oss.str();
      }
    };
    return std::visit(V{L}, Data);
  }

  [[nodiscard]] class Monotype toMonotype() const;

  SrcLocation getLocation() { return Location; };

  // Simple classification helpers
  static bool isInteger(PrimitiveKind K) {
    switch (K) {
    case PrimitiveKind::I8:
    case PrimitiveKind::I16:
    case PrimitiveKind::I32:
    case PrimitiveKind::I64:
    case PrimitiveKind::U8:
    case PrimitiveKind::U16:
    case PrimitiveKind::U32:
    case PrimitiveKind::U64:
      return true;
    default:
      return false;
    }
  }
  static bool isSignedInteger(PrimitiveKind K) {
    switch (K) {
    case PrimitiveKind::I8:
    case PrimitiveKind::I16:
    case PrimitiveKind::I32:
    case PrimitiveKind::I64:
      return true;
    default:
      return false;
    }
  }
  static bool isUnsignedInteger(PrimitiveKind K) {
    switch (K) {
    case PrimitiveKind::U8:
    case PrimitiveKind::U16:
    case PrimitiveKind::U32:
    case PrimitiveKind::U64:
      return true;
    default:
      return false;
    }
  }
  static bool isFloat(PrimitiveKind K) {
    return K == PrimitiveKind::F32 || K == PrimitiveKind::F64;
  }

private:
  Node Data;
  SrcLocation Location{"", -1, -1};

  explicit Type(Node N) : Data(std::move(N)) {}
  Type(Node N, SrcLocation Location)
      : Data(std::move(N)), Location(std::move(Location)) {}
};

inline bool
ReferenceType::operator==(const ReferenceType &Other) const noexcept {
  return *Pointee == *Other.Pointee;
}
inline bool PointerType::operator==(const PointerType &Other) const noexcept {
  return *Pointee == *Other.Pointee;
}
inline bool GenericType::operator==(const GenericType &Other) const noexcept {
  return Name == Other.Name && TypeArguments == Other.TypeArguments;
}
inline bool FunctionType::operator==(const FunctionType &Other) const noexcept {
  if (Parameters != Other.Parameters)
    return false;
  return *ReturnType == *Other.ReturnType;
}

} // namespace phi
