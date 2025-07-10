/**
 * @file token.hpp
 * @brief Token definitions and Token class for the Phi programming language
 * lexer
 *
 * This file contains the TokenType enumeration that defines all possible token
 * types that can be recognized by the scanner, and the Token class that
 * represents a single token with its type, lexeme, and source location
 * information.
 */

#include <cstdint>
#include <string>

#pragma once

/**
 * @brief Enumeration of all possible token types in the Phi programming
 * language
 *
 * This enum defines every type of token that the scanner can recognize,
 * including keywords, operators, literals, identifiers, and syntax elements.
 * The tokens are organized into logical groups for better maintainability.
 */
enum TokenType : uint8_t {
    /// End of file token
    tok_eof,
    /// Error token for invalid characters or malformed tokens
    tok_error,

    // keywords
    tok_break,    ///< 'break' keyword for loop control
    tok_class,    ///< 'class' keyword for class declarations
    tok_const,    ///< 'const' keyword for constant declarations
    tok_continue, ///< 'continue' keyword for loop control
    tok_else,     ///< 'else' keyword for conditional statements
    tok_elif,     ///< 'elif' keyword for conditional statements
    tok_false,    ///< 'false' boolean literal keyword
    tok_for,      ///< 'for' keyword for loop statements
    tok_fun,      ///< 'fun' keyword for function declarations
    tok_if,       ///< 'if' keyword for conditional statements
    tok_import,   ///< 'import' keyword for module imports
    tok_in,       ///< 'in' keyword for iteration and membership
    tok_let,      ///< 'let' keyword for variable declarations
    tok_return,   ///< 'return' keyword for function returns
    tok_true,     ///< 'true' boolean literal keyword
    tok_while,    ///< 'while' keyword for loop statements

    // signed int types
    tok_i8,  ///< 8-bit signed integer type keyword
    tok_i16, ///< 16-bit signed integer type keyword
    tok_i32, ///< 32-bit signed integer type keyword
    tok_i64, ///< 64-bit signed integer type keyword

    // unsigned int types
    tok_u8,  ///< 8-bit unsigned integer type keyword
    tok_u16, ///< 16-bit unsigned integer type keyword
    tok_u32, ///< 32-bit unsigned integer type keyword
    tok_u64, ///< 64-bit unsigned integer type keyword

    // floating point number types
    tok_f32, ///< 32-bit floating point type keyword
    tok_f64, ///< 64-bit floating point type keyword

    // text types
    tok_str,  ///< String type keyword
    tok_char, ///< Character type keyword

    // syntax
    tok_open_paren,    ///< '(' opening parenthesis
    tok_close_paren,   ///< ')' closing parenthesis
    tok_open_brace,    ///< '{' opening brace
    tok_close_brace,   ///< '}' closing brace
    tok_open_bracket,  ///< '[' opening bracket
    tok_close_bracket, ///< ']' closing bracket
    tok_fun_return,    ///< '->' function return type indicator
    tok_comma,         ///< ',' comma separator
    tok_semicolon,     ///< ';' statement terminator

    // operators
    tok_add,  ///< '+' addition operator
    tok_sub,  ///< '-' subtraction operator
    tok_mul,  ///< '*' multiplication operator
    tok_div,  ///< '/' division operator
    tok_mod,  ///< '%' modulo operator
    tok_bang, ///< '!' logical NOT operator

    tok_plus_equals, ///< '+=' addition assignment operator
    tok_sub_equals,  ///< '-=' subtraction assignment operator
    tok_mul_equals,  ///< '*=' multiplication assignment operator
    tok_div_equals,  ///< '/=' division assignment operator
    tok_mod_equals,  ///< '%=' modulo assignment operator

    tok_member,           ///< '.' member access operator
    tok_namespace_member, ///< '::' namespace member access operator

    tok_increment, ///< '++' increment operator
    tok_decrement, ///< '--' decrement operator

    tok_equal,     ///< '==' equality operator
    tok_not_equal, ///< '!=' inequality operator

    tok_and, ///< '&&' logical AND operator
    tok_or,  ///< '||' logical OR operator

    tok_less,          ///< '<' less than operator
    tok_less_equal,    ///< '<=' less than or equal operator
    tok_greater,       ///< '>' greater than operator
    tok_greater_equal, ///< '>=' greater than or equal operator

    tok_assign, ///< '=' assignment operator
    tok_colon,  ///< ':' colon for type annotations

    tok_int_literal,   ///< Integer literal (e.g., 42, 123)
    tok_float_literal, ///< Floating point literal (e.g., 3.14, 2.5)
    tok_str_literal,   ///< String literal (e.g., "hello")
    tok_char_literal,  ///< Character literal (e.g., 'a', '\n')
    tok_identifier, ///< User-defined identifier (e.g., variable names, function
                    ///< names)
};

/**
 * @brief Converts a TokenType enumeration value to its string representation
 * @param type The TokenType to convert
 * @return A string representation of the token type
 */
std::string token_type_to_string(TokenType type);

/**
 * @brief Represents a single token in the Phi programming language
 *
 * A Token contains all the information needed to represent a lexical unit:
 * the token type, the original text (lexeme), and its position in the source
 * code. This class is immutable once constructed and provides accessor methods
 * for all token properties.
 */
class Token {
public:
    /**
     * @brief Constructs a new Token
     * @param line The line number where this token appears (1-indexed)
     * @param col The column number where this token starts (1-indexed)
     * @param type The type of this token
     * @param lexeme The actual text content of this token from the source
     */
    Token(int line, int col, TokenType type, std::string lexeme);

    /**
     * @brief Gets the line number where this token appears
     * @return The 1-indexed line number
     */
    [[nodiscard]] int get_line() const { return line; }

    /**
     * @brief Gets the column number where this token starts
     * @return The 1-indexed column number
     */
    [[nodiscard]] int get_col() const { return col; }

    /**
     * @brief Gets the type of this token
     * @return The TokenType enumeration value
     */
    [[nodiscard]] TokenType get_type() const { return type; }

    /**
     * @brief Gets the lexeme (original text) of this token
     * @return The string content that was scanned to create this token
     */
    [[nodiscard]] std::string get_lexeme() const { return lexeme; }

    /**
     * @brief Converts this token to a human-readable string representation
     * @return A formatted string showing the token type, lexeme, and position
     */
    [[nodiscard]] std::string to_string() const;

private:
    int line, col;      ///< Source location (line and column numbers)
    TokenType type;     ///< The type of this token
    std::string lexeme; ///< The original text content from source
};
