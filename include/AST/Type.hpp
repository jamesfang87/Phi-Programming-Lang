#pragma once

#include <cassert>
#include <cstdint>
#include <optional>
#include <string>
#include <utility>
#include <variant>
#include <vector>

#include <llvm/IR/LLVMContext.h>
#include <llvm/IR/Type.h>

#include "SrcManager/SrcLocation.hpp"

namespace phi {

//===----------------------------------------------------------------------===//
// Forward Declarations and Type Aliases
//===----------------------------------------------------------------------===//

class Type;
class Monotype;
using TypePtr = std::shared_ptr<Type>;

//===----------------------------------------------------------------------===//
// Primitive Type Enumeration
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

//===----------------------------------------------------------------------===//
// Composite Type Structures
//===----------------------------------------------------------------------===//

struct StructType {
  std::string Name;
  bool operator==(const StructType &Other) const noexcept {
    return Name == Other.Name;
  }
};

struct EnumType {
  std::string Name;
  bool operator==(const EnumType &Other) const noexcept {
    return Name == Other.Name;
  }
};

struct TupleType {
  std::vector<Type> Types;
  bool operator==(const TupleType &Other) const noexcept;
};

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

//===----------------------------------------------------------------------===//
// Type - Main type representation class
//===----------------------------------------------------------------------===//

class Type {
public:
  using TypeAtoms =
      std::variant<PrimitiveKind, StructType, EnumType, TupleType,
                   ReferenceType, PointerType, GenericType, FunctionType>;

  //===--------------------------------------------------------------------===//
  // Constructors & Destructors
  //===--------------------------------------------------------------------===//

  Type() = default;

  //===--------------------------------------------------------------------===//
  // Static Factory Methods
  //===--------------------------------------------------------------------===//

  static Type makePrimitive(PrimitiveKind K, SrcLocation L) {
    return Type{K, std::move(L)};
  }

  static Type makeStruct(std::string Name, SrcLocation L) {
    return Type(StructType{std::move(Name)}, std::move(L));
  }

  static Type makeEnum(std::string Name, SrcLocation L) {
    return Type(EnumType{std::move(Name)}, std::move(L));
  }

  static Type makeTuple(std::vector<Type> Types, SrcLocation L) {
    return Type(TupleType{.Types = std::move(Types)}, std::move(L));
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

  //===--------------------------------------------------------------------===//
  // Getters
  //===--------------------------------------------------------------------===//

  [[nodiscard]] const TypeAtoms &getData() const noexcept { return Data; }
  [[nodiscard]] SrcLocation getLocation() const { return Location; }

  [[nodiscard]] PrimitiveKind asPrimitive() const {
    assert(isPrimitive() && "asPrimitive() used on non-primitive type");
    return std::get<PrimitiveKind>(Data);
  }

  [[nodiscard]] StructType asStruct() const {
    assert(isStruct() && "asStruct() used on non-custom type");
    return std::get<StructType>(Data);
  }

  [[nodiscard]] EnumType asEnum() const {
    assert(isStruct() && "asEnum() used on non-custom type");
    return std::get<EnumType>(Data);
  }

  [[nodiscard]] TupleType asTuple() const {
    assert(isTuple() && "asTuple() used on non-tuple type");
    return std::get<TupleType>(Data);
  }

  [[nodiscard]] PointerType asPtr() const {
    assert(isPtr() && "asPtr() used on non-ptr type");
    return std::get<PointerType>(Data);
  }

  [[nodiscard]] ReferenceType asRef() const {
    assert(isRef() && "asRef() used on non-ref type");
    return std::get<ReferenceType>(Data);
  }

  [[nodiscard]] GenericType asGeneric() const {
    assert(isGeneric() && "asGeneric() used on non-generic type");
    return std::get<GenericType>(Data);
  }

  [[nodiscard]] FunctionType asFun() const {
    assert(isFun() && "asFun() used on non-function type");
    return std::get<FunctionType>(Data);
  }

  //===--------------------------------------------------------------------===//
  // Type Classification Methods
  //===--------------------------------------------------------------------===//

  [[nodiscard]] bool isPrimitive() const {
    return std::holds_alternative<PrimitiveKind>(Data);
  }

  [[nodiscard]] bool isStruct() const {
    return std::holds_alternative<StructType>(Data);
  }

  [[nodiscard]] bool isEnum() const {
    return std::holds_alternative<EnumType>(Data);
  }

  [[nodiscard]] bool isTuple() const {
    return std::holds_alternative<TupleType>(Data);
  }

  [[nodiscard]] bool isRef() const {
    return std::holds_alternative<ReferenceType>(Data);
  }

  [[nodiscard]] bool isPtr() const {
    return std::holds_alternative<PointerType>(Data);
  }

  [[nodiscard]] bool isGeneric() const {
    return std::holds_alternative<GenericType>(Data);
  }

  [[nodiscard]] bool isFun() const {
    return std::holds_alternative<FunctionType>(Data);
  }

  //===--------------------------------------------------------------------===//
  // Semantic Type Queries
  //===--------------------------------------------------------------------===//

  [[nodiscard]] bool isInteger() const {
    if (!isPrimitive()) {
      return false;
    }

    switch (asPrimitive()) {
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

  [[nodiscard]] bool isSignedInteger() const {
    if (!isPrimitive()) {
      return false;
    }

    switch (asPrimitive()) {
    case PrimitiveKind::I8:
    case PrimitiveKind::I16:
    case PrimitiveKind::I32:
    case PrimitiveKind::I64:
      return true;
    default:
      return false;
    }
  }

  [[nodiscard]] bool isUnsignedInteger() const {
    if (!isPrimitive()) {
      return false;
    }

    switch (asPrimitive()) {
    case PrimitiveKind::U8:
    case PrimitiveKind::U16:
    case PrimitiveKind::U32:
    case PrimitiveKind::U64:
      return true;
    default:
      return false;
    }
  }

  [[nodiscard]] bool isFloat() const {
    if (!isPrimitive()) {
      return false;
    }

    auto K = asPrimitive();
    return K == PrimitiveKind::F32 || K == PrimitiveKind::F64;
  }

  [[nodiscard]] bool isNull() const {
    if (!isPrimitive()) {
      return false;
    }

    return asPrimitive() == PrimitiveKind::Null;
  }

  [[nodiscard]] std::optional<std::string> getCustomName() const {
    if (isStruct()) {
      return asStruct().Name;
    }

    if (isEnum()) {
      return asEnum().Name;
    }

    return std::nullopt;
  }

  [[nodiscard]] const Type getUnderlying() const;

  //===--------------------------------------------------------------------===//
  // Comparison Operators
  //===--------------------------------------------------------------------===//

  bool operator==(const Type &Other) const noexcept {
    return Data == Other.Data;
  }

  bool operator!=(const Type &Other) const noexcept {
    return !(*this == Other);
  }

  //===--------------------------------------------------------------------===//
  // Conversion Methods
  //===--------------------------------------------------------------------===//

  [[nodiscard]] std::string toString() const;
  [[nodiscard]] Monotype toMonotype() const;
  [[nodiscard]] llvm::Type *toLLVM(llvm::LLVMContext &Ctx) const;

private:
  //===--------------------------------------------------------------------===//
  // Private Members
  //===--------------------------------------------------------------------===//

  TypeAtoms Data;
  SrcLocation Location{.Path = "", .Line = -1, .Col = -1};

  //===--------------------------------------------------------------------===//
  // Private Constructors
  //===--------------------------------------------------------------------===//

  explicit Type(TypeAtoms N) : Data(std::move(N)) {}
  Type(TypeAtoms N, SrcLocation Location)
      : Data(std::move(N)), Location(std::move(Location)) {}
};

//===----------------------------------------------------------------------===//
// Inline Implementations for Composite Type Comparisons
//===----------------------------------------------------------------------===//

inline bool TupleType::operator==(const TupleType &Other) const noexcept {
  return (Types == Other.Types);
}

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
