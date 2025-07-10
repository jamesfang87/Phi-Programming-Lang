/**
 * @file token.cpp
 * @brief Implementation of the Token class and utility functions
 *
 * This file contains the implementation of the Token class constructor,
 * the token_type_to_string utility function for converting TokenType values
 * to human-readable strings, and the Token::to_string() method for debugging
 * output.
 */

#include "token.hpp"
#include <format>
#include <string>

Token::Token(int line, int col, TokenType type, std::string lexeme) {
    this->line = line;
    this->col = col;
    this->type = type;
    this->lexeme = std::move(lexeme);
}

/**
 * @brief Converts a TokenType enumeration value to its string representation
 *
 * This function provides a mapping from TokenType enum values to their
 * corresponding human-readable string names. This is primarily used for
 * debugging output and error messages. The returned strings are in uppercase
 * and match the token type names without the "tok_" prefix.
 *
 * @param type The TokenType to convert
 * @return A string representation of the token type (e.g., "IDENTIFIER", "ADD")
 */
std::string token_type_to_string(TokenType type) {
    // Static lookup table for fast token type to string conversion
    static const std::string token_strings[] = {
        // Special tokens
        "EOF",   // tok_eof
        "ERROR", // tok_error

        // Keywords (control flow)
        "BREAK",    // tok_break
        "CLASS",    // tok_class
        "CONST",    // tok_const
        "CONTINUE", // tok_continue
        "ELSE",     // tok_else
        "ELIF",     // tok_elif
        "FALSE",    // tok_false
        "FOR",      // tok_for
        "FUN",      // tok_fun
        "IF",       // tok_if
        "IMPORT",   // tok_import
        "IN",       // tok_in
        "LET",      // tok_let
        "RETURN",   // tok_return
        "TRUE",     // tok_true
        "WHILE",    // tok_while

        // Signed integer types
        "I8",  // tok_i8
        "I16", // tok_i16
        "I32", // tok_i32
        "I64", // tok_i64

        // Unsigned integer types
        "U8",  // tok_u8
        "U16", // tok_u16
        "U32", // tok_u32
        "U64", // tok_u64

        // Floating point types
        "F32", // tok_f32
        "F64", // tok_f64

        // Text types
        "STR",  // tok_str
        "CHAR", // tok_char

        // Syntax elements
        "OPEN_PAREN",    // tok_open_paren
        "CLOSE_PAREN",   // tok_close_paren
        "OPEN_BRACE",    // tok_open_brace
        "CLOSE_BRACE",   // tok_close_brace
        "OPEN_BRACKET",  // tok_open_bracket
        "CLOSE_BRACKET", // tok_close_bracket
        "FUN_RETURN",    // tok_fun_return
        "COMMA",         // tok_comma
        "SEMICOLON",     // tok_semicolon

        // Basic operators
        "ADD",  // tok_add
        "SUB",  // tok_sub
        "MUL",  // tok_mul
        "DIV",  // tok_div
        "MOD",  // tok_mod
        "BANG", // tok_bang

        // Assignment operators
        "PLUS_EQUALS", // tok_plus_equals
        "SUB_EQUALS",  // tok_sub_equals
        "MUL_EQUALS",  // tok_mul_equals
        "DIV_EQUALS",  // tok_div_equals
        "MOD_EQUALS",  // tok_mod_equals

        // Member access
        "MEMBER",           // tok_member
        "NAMESPACE_MEMBER", // tok_namespace_member

        // Increment/decrement
        "INCREMENT", // tok_increment
        "DECREMENT", // tok_decrement

        // Comparison operators
        "EQUAL",     // tok_equal
        "NOT_EQUAL", // tok_not_equal

        // Logical operators
        "AND", // tok_and
        "OR",  // tok_or

        // Relational operators
        "LESS",          // tok_less
        "LESS_EQUAL",    // tok_less_equal
        "GREATER",       // tok_greater
        "GREATER_EQUAL", // tok_greater_equal

        // Other operators
        "ASSIGN", // tok_assign
        "COLON",  // tok_colon

        // Literals
        "INT_LITERAL",    // tok_int_literal
        "FLOAT_LITERAL",  // tok_float_literal
        "STRING_LITERAL", // tok_str_literal
        "CHAR_LITERAL",   // tok_char_literal
        "IDENTIFIER"      // tok_identifier
    };

    // Array size check to ensure all tokens are covered
    static_assert(
        sizeof(token_strings) / sizeof(token_strings[0]) == tok_identifier + 1,
        "Token string array size must match the number of token types");

    // Bounds check and return appropriate string
    if (type <= tok_identifier) {
        return token_strings[type];
    }
    return "UNKNOWN";
}

/**
 * @brief Formats the token as a human-readable string for debugging
 *
 * Creates a formatted string representation of the token that includes
 * the token type name, the lexeme (original text), and the source location.
 * This method is primarily used for debugging output and compiler diagnostics.
 *
 * The format is: [TOKEN_TYPE] "lexeme" at line N, column M
 *
 * @return A formatted string representation of this token
 */
std::string Token::to_string() const {
    return std::format("[{}] \"{}\" at line {}, column {}",
                       token_type_to_string(type),
                       lexeme,
                       line,
                       col);
}
