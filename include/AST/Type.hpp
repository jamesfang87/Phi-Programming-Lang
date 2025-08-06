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
    return primitiveType != Primitive::custom;
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
    custom
  };

  /**
   * @brief Constructs primitive type
   * @param primitive_type Primitive type value
   */
  explicit Type(const Primitive primitiveType) : primitiveType(primitiveType) {
    assert(primitive_type != Primitive::custom &&
           "Use custom_type constructor for custom types");
  }

  /**
   * @brief Constructs custom type
   * @param custom_type_name Name of custom type
   */
  explicit Type(std::string custom_type_name)
      : primitiveType(Primitive::custom),
        custom_type_name_(std::move(custom_type_name)) {}

  /**
   * @brief Converts type to string representation
   * @return Type name string
   */
  [[nodiscard]] std::string toString() const {
    if (primitiveType == Primitive::custom) {
      return custom_type_name_;
    }
    return primitive_to_string(primitiveType);
  }

  // ACCESSORS
  [[nodiscard]] Primitive primitive_type() const noexcept {
    return primitiveType;
  }

  /**
   * @brief Retrieves custom type name
   * @return Custom type name string
   */
  [[nodiscard]] const std::string &custom_type_name() const {
    assert(primitive_type_ == Primitive::custom);
    return custom_type_name_;
  }

  // OPERATORS
  bool operator==(const Type &other) const {
    if (primitiveType != other.primitiveType) {
      return false;
    }
    if (primitiveType == Primitive::custom) {
      return custom_type_name_ == other.custom_type_name_;
    }
    return true;
  }

  bool operator!=(const Type &other) const { return !(*this == other); }

private:
  /**
   * @brief Converts primitive enum to string
   * @param primitive Primitive type value
   * @return Primitive type name
   */
  static std::string primitive_to_string(const Primitive &primitive) {
    switch (primitive) {
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

  Primitive primitiveType;       ///< Underlying primitive type
  std::string custom_type_name_; ///< Name for custom types
};

bool isIntTy(const Type &type);

bool isSignedInt(const Type &type);
bool isUnsignedInt(const Type &type);

/// Checks if type is any float type
bool isFloat(const Type &type);

/// Checks if type is numeric (integer or float)
bool isNumTy(const Type &type);

} // namespace phi
