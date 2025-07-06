#include <cassert>
#include <memory>
#include <string>
#include <utility>

#pragma once

class Type {
public:
    enum class Primitive : char {
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

        // Special types
        null,
        custom
    };

    // Constructor for primitive types
    explicit Type(Primitive primitive_type)
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

private:
    static std::string primitive_to_string(Primitive primitive) {
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
            case Primitive::null: return "null";
            default: return "unknown";
        }
    }

    Primitive primitive_type_;
    std::string custom_type_name_; // Only used for custom types
};

class Block {
public:
    void info_dump(int level = 0) const;
};

class Decl {
public:
    Decl(int line, int col, std::string identifier)
        : line(line),
          col(col),
          identifier(std::move(identifier)) {}

    virtual ~Decl() = default;
    Decl(const Decl&) = delete;
    Decl& operator=(const Decl&) = delete;

    virtual void info_dump(int level = 0) const = 0;

protected:
    int line, col;
    std::string identifier;
};

class FunctionDecl : public Decl {
public:
    FunctionDecl(int line,
                 int col,
                 std::string identifier,
                 Type return_type,
                 std::unique_ptr<Block> block_ptr)
        : Decl(line, col, std::move(identifier)),
          return_type(return_type),
          block(std::move(block_ptr)) {}

    void info_dump(int level = 0) const override;

private:
    Type return_type;
    std::unique_ptr<Block> block;
};

class Stmt {
public:
    Stmt();

private:
};

class Expr {
public:
    Expr();

private:
};
