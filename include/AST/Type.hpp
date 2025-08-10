#pragma once

#include <cassert>
#include <string>

namespace phi {

/**
 * @brief Represents a type in the Phi type system
 *
 * Encapsulates both primitive types (integers, floats, bool, etc.) and custom
 * user-defined types. Provides type checking, comparison, and string
 * conversion.
 */
class Type {
public:
  /**
   * @brief Checks if type is primitive
   * @return true if primitive, false if custom
   */
  [[nodiscard]] bool isPrimitive() const noexcept {
    return PrimitiveType != PrimitiveKind::CustomKind;
  }

  /**
   * @brief Enumeration of primitive types
   */
  enum class PrimitiveKind : uint8_t {
    // SIGNED INTEGERS
    I8Kind,
    I16Kind,
    I32Kind,
    I64Kind,

    // UNSIGNED INTEGERS
    U8Kind,
    U16Kind,
    U32Kind,
    U64Kind,

    // FLOATING-POINT
    F32Kind,
    F64Kind,

    // TEXT TYPES
    StringKind,
    CharKind,

    // BOOLEAN
    BoolKind,

    // RANGE
    RangeKind,

    // SPECIAL TYPES
    NullKind,
    CustomKind
  };

  /**
   * @brief Constructs primitive type
   * @param primitiveType
   */
  explicit Type(const PrimitiveKind PrimitiveType)
      : PrimitiveType(PrimitiveType) {
    assert(PrimitiveType != PrimitiveKind::CustomKind &&
           "Use custom_type constructor for custom types");
  }

  /**
   * @brief Constructs custom type
   * @param CustomTypeName Name of custom type
   */
  explicit Type(std::string CustomTypeName)
      : PrimitiveType(PrimitiveKind::CustomKind),
        CustomTypeName(std::move(CustomTypeName)) {}

  /**
   * @brief Converts type to string representation
   * @return Type name string
   */
  [[nodiscard]] std::string toString() const {
    if (PrimitiveType == PrimitiveKind::CustomKind) {
      return CustomTypeName;
    }
    return primitiveToString(PrimitiveType);
  }

  // ACCESSORS
  [[nodiscard]] PrimitiveKind getPrimitiveType() const noexcept {
    return PrimitiveType;
  }

  /**
   * @brief Retrieves custom type name
   * @return Custom type name string
   */
  [[nodiscard]] const std::string &getCustomTypeName() const {
    assert(PrimitiveType == PrimitiveKind::CustomKind);
    return CustomTypeName;
  }

  // OPERATORS
  bool operator==(const Type &Other) const {
    if (PrimitiveType != Other.PrimitiveType) {
      return false;
    }
    if (PrimitiveType == PrimitiveKind::CustomKind) {
      return CustomTypeName == Other.CustomTypeName;
    }
    return true;
  }

  bool operator!=(const Type &Other) const { return !(*this == Other); }

private:
  /**
   * @brief Converts primitive enum to string
   * @param primitive Primitive type value
   * @return Primitive type name
   */
  static std::string primitiveToString(const PrimitiveKind &Prim) {
    switch (Prim) {
    case PrimitiveKind::I8Kind:
      return "i8";
    case PrimitiveKind::I16Kind:
      return "i16";
    case PrimitiveKind::I32Kind:
      return "i32";
    case PrimitiveKind::I64Kind:
      return "i64";
    case PrimitiveKind::U8Kind:
      return "u8";
    case PrimitiveKind::U16Kind:
      return "u16";
    case PrimitiveKind::U32Kind:
      return "u32";
    case PrimitiveKind::U64Kind:
      return "u64";
    case PrimitiveKind::F32Kind:
      return "f32";
    case PrimitiveKind::F64Kind:
      return "f64";
    case PrimitiveKind::StringKind:
      return "string";
    case PrimitiveKind::CharKind:
      return "char";
    case PrimitiveKind::BoolKind:
      return "bool";
    case PrimitiveKind::RangeKind:
      return "range";
    case PrimitiveKind::NullKind:
      return "null";
    default:
      return "unknown";
    }
  }

  PrimitiveKind PrimitiveType; ///< Underlying primitive type
  std::string CustomTypeName;  ///< Name for custom types
};

bool isIntTy(const Type &Ty);

bool isSignedInt(const Type &Ty);
bool isUnsignedInt(const Type &Ty);

/// Checks if type is any float type
bool isFloat(const Type &Ty);

/// Checks if type is numeric (integer or float)
bool isNumTy(const Type &Ty);

} // namespace phi
