#pragma once

#include <cassert>
#include <string>

class Type {
public:
    [[nodiscard]] bool is_primitive() const noexcept {
        return primitive_type_ != Primitive::custom;
    }

    enum class Primitive : uint8_t {
        // Signed integer types
        i8,
        i16,
        i32,
        i64,

        // Unsigned integer types
        u8,
        u16,
        u32,
        u64,

        // Floating-point types
        f32,
        f64,

        // Text types
        str,
        character,

        // bool
        boolean,

        // range
        range,

        // Special types
        null,
        custom
    };

    // Constructor for primitive types
    explicit Type(const Primitive primitive_type)
        : primitive_type_(primitive_type) {
        assert(primitive_type != Primitive::custom &&
               "Use custom_type constructor for custom types");
    }

    // Constructor for custom types
    explicit Type(std::string custom_type_name)
        : primitive_type_(Primitive::custom),
          custom_type_name_(std::move(custom_type_name)) {}

    // Get string representation
    [[nodiscard]] std::string to_string() const {
        if (primitive_type_ == Primitive::custom) {
            return custom_type_name_;
        }
        return primitive_to_string(primitive_type_);
    }

    // Accessors
    [[nodiscard]] Primitive primitive_type() const noexcept { return primitive_type_; }

    [[nodiscard]] const std::string& custom_type_name() const {
        assert(primitive_type_ == Primitive::custom);
        return custom_type_name_;
    }

    // Equality operators
    bool operator==(const Type& other) const {
        if (primitive_type_ != other.primitive_type_) {
            return false;
        }
        if (primitive_type_ == Primitive::custom) {
            return custom_type_name_ == other.custom_type_name_;
        }
        return true;
    }

    bool operator!=(const Type& other) const { return !(*this == other); }

private:
    static std::string primitive_to_string(const Primitive& primitive) {
        switch (primitive) {
            case Primitive::i8: return "i8";
            case Primitive::i16: return "i16";
            case Primitive::i32: return "i32";
            case Primitive::i64: return "i64";
            case Primitive::u8: return "u8";
            case Primitive::u16: return "u16";
            case Primitive::u32: return "u32";
            case Primitive::u64: return "u64";
            case Primitive::f32: return "f32";
            case Primitive::f64: return "f64";
            case Primitive::str: return "str";
            case Primitive::character: return "char";
            case Primitive::boolean: return "bool";
            case Primitive::range: return "range";
            case Primitive::null: return "null";
            default: return "unknown";
        }
    }

    Primitive primitive_type_;
    std::string custom_type_name_; // Only used for custom types
};
