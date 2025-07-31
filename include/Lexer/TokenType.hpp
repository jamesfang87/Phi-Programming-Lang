#pragma once

#include <string>

namespace phi {

/**
 * @brief Enumeration of token types for Phi language
 *
 * Defines all lexical tokens recognized by the scanner, including:
 * - Keywords and type specifiers
 * - Operators and punctuation
 * - Literal value tokens
 * - Structural syntax elements
 *
 * Uses enum class for type safety and namespace protection.
 */
enum class TokenType : uint8_t {
    /// End of file marker
    tok_eof,
    /// Error token for invalid inputs
    tok_error,

    // KEYWORDS
    tok_bool,     ///< `bool` keyword
    tok_break,    ///< `break` keyword
    tok_class,    ///< `class` keyword
    tok_const,    ///< `const` keyword
    tok_continue, ///< `continue` keyword
    tok_else,     ///< `else` keyword
    tok_false,    ///< `false` literal
    tok_for,      ///< `for` keyword
    tok_fun,      ///< `fun` keyword
    tok_if,       ///< `if` keyword
    tok_import,   ///< `import` keyword
    tok_in,       ///< `in` keyword
    tok_let,      ///< `let` keyword
    tok_return,   ///< `return` keyword
    tok_true,     ///< `true` literal
    tok_while,    ///< `while` keyword

    // SIGNED INTEGER TYPES
    tok_i8,  ///< `i8` type (8-bit signed)
    tok_i16, ///< `i16` type (16-bit signed)
    tok_i32, ///< `i32` type (32-bit signed)
    tok_i64, ///< `i64` type (64-bit signed)

    // UNSIGNED INTEGER TYPES
    tok_u8,  ///< `u8` type (8-bit unsigned)
    tok_u16, ///< `u16` type (16-bit unsigned)
    tok_u32, ///< `u32` type (32-bit unsigned)
    tok_u64, ///< `u64` type (64-bit unsigned)

    // FLOATING-POINT TYPES
    tok_f32, ///< `f32` type (32-bit float)
    tok_f64, ///< `f64` type (64-bit float)

    // TEXT TYPES
    tok_str,  ///< `str` type keyword
    tok_char, ///< `char` type keyword

    // SYNTAX DELIMITERS
    tok_open_paren,    ///< `(` parenthesis
    tok_close_paren,   ///< `)` parenthesis
    tok_open_brace,    ///< `{` brace
    tok_close_brace,   ///< `}` brace
    tok_open_bracket,  ///< `[` bracket
    tok_close_bracket, ///< `]` bracket
    tok_fun_return,    ///< `->` function return
    tok_comma,         ///< `,` separator
    tok_semicolon,     ///< `;` statement terminator

    // OPERATORS
    tok_add,  ///< `+` addition
    tok_sub,  ///< `-` subtraction/negation
    tok_mul,  ///< `*` multiplication
    tok_div,  ///< `/` division
    tok_mod,  ///< `%` modulo
    tok_bang, ///< `!` logical NOT

    // COMPOUND ASSIGNMENT OPERATORS
    tok_plus_equals, ///< `+=` add-assign
    tok_sub_equals,  ///< `-=` subtract-assign
    tok_mul_equals,  ///< `*=` multiply-assign
    tok_div_equals,  ///< `/=` divide-assign
    tok_mod_equals,  ///< `%=` modulo-assign

    // MEMBER ACCESS
    tok_member,           ///< `.` member access
    tok_namespace_member, ///< `::` namespace access

    // INCREMENT/DECREMENT
    tok_increment, ///< `++` increment
    tok_decrement, ///< `--` decrement

    // EQUALITY
    tok_equal,     ///< `==` equality
    tok_not_equal, ///< `!=` inequality

    // LOGICAL
    tok_and, ///< `&&` logical AND
    tok_or,  ///< `||` logical OR

    // RELATIONAL
    tok_less,          ///< `<` less than
    tok_less_equal,    ///< `<=` less than or equal
    tok_greater,       ///< `>` greater than
    tok_greater_equal, ///< `>=` greater than or equal

    // ASSIGNMENT AND TYPE
    tok_assign, ///< `=` assignment
    tok_colon,  ///< `:` type annotation

    // RANGE OPERATORS
    tok_exclusive_range, ///< `..` exclusive range
    tok_inclusive_range, ///< `..=` inclusive range

    // LITERALS
    tok_int_literal,   ///< Integer literal (e.g., 42)
    tok_float_literal, ///< Float literal (e.g., 3.14)
    tok_str_literal,   ///< String literal (e.g., "text")
    tok_char_literal,  ///< Char literal (e.g., 'a')
    tok_identifier,    ///< Identifier (user-defined names)
};

/**
 * @brief Converts TokenType to human-readable string
 * @param type Token type to convert
 * @return String representation of token type
 */
std::string type_to_string(TokenType type);

} // namespace phi
