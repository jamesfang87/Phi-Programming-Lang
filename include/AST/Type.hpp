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
    return PrimitiveType != Primitive::Custom;
  }

  /**
   * @brief Enumeration of primitive types
   */
  enum class Primitive : uint8_t {
    // SIGNED INTEGERS
    i8,
    i16,
    i32,
    i64,

    // UNSIGNED INTEGERS
    u8,
    u16,
    u32,
    u64,

    // FLOATING-POINT
    f32,
    f64,

    // TEXT TYPES
    str,
    character,

    // BOOLEAN
    boolean,

    // RANGE
    range,

    // SPECIAL TYPES
    null,
    Custom
  };

  /**
   * @brief Constructs primitive type
   * @param primitiveType
   */
  explicit Type(const Primitive PrimitiveType) : PrimitiveType(PrimitiveType) {
    assert(PrimitiveType != Primitive::Custom &&
           "Use custom_type constructor for custom types");
  }

  /**
   * @brief Constructs custom type
   * @param CustomTypeName Name of custom type
   */
  explicit Type(std::string CustomTypeName)
      : PrimitiveType(Primitive::Custom),
        CustomTypeName(std::move(CustomTypeName)) {}

  /**
   * @brief Converts type to string representation
   * @return Type name string
   */
  [[nodiscard]] std::string toString() const {
    if (PrimitiveType == Primitive::Custom) {
      return CustomTypeName;
    }
    return primitiveToString(PrimitiveType);
  }

  // ACCESSORS
  [[nodiscard]] Primitive getPrimitiveType() const noexcept {
    return PrimitiveType;
  }

  /**
   * @brief Retrieves custom type name
   * @return Custom type name string
   */
  [[nodiscard]] const std::string &getCustomTypeName() const {
    assert(PrimitiveType == Primitive::Custom);
    return CustomTypeName;
  }

  // OPERATORS
  bool operator==(const Type &Other) const {
    if (PrimitiveType != Other.PrimitiveType) {
      return false;
    }
    if (PrimitiveType == Primitive::Custom) {
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
  static std::string primitiveToString(const Primitive &Prim) {
    switch (Prim) {
    case Primitive::i8:
      return "i8";
    case Primitive::i16:
      return "i16";
    case Primitive::i32:
      return "i32";
    case Primitive::i64:
      return "i64";
    case Primitive::u8:
      return "u8";
    case Primitive::u16:
      return "u16";
    case Primitive::u32:
      return "u32";
    case Primitive::u64:
      return "u64";
    case Primitive::f32:
      return "f32";
    case Primitive::f64:
      return "f64";
    case Primitive::str:
      return "string";
    case Primitive::character:
      return "char";
    case Primitive::boolean:
      return "bool";
    case Primitive::range:
      return "range";
    case Primitive::null:
      return "null";
    default:
      return "unknown";
    }
  }

  Primitive PrimitiveType;    ///< Underlying primitive type
  std::string CustomTypeName; ///< Name for custom types
};

bool isIntTy(const Type &type);

bool isSignedInt(const Type &type);
bool isUnsignedInt(const Type &type);

/// Checks if type is any float type
bool isFloat(const Type &type);

/// Checks if type is numeric (integer or float)
bool isNumTy(const Type &type);

} // namespace phi
