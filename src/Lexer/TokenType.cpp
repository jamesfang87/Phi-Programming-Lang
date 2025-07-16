#include "Lexer/TokenType.hpp"

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

std::string type_to_string(TokenType type) {
    switch (type) {
        // Special tokens
        case TokenType::tok_eof: return "EOF";
        case TokenType::tok_error: return "ERROR";

        // Keywords
        case TokenType::tok_break: return "BREAK";
        case TokenType::tok_class: return "CLASS";
        case TokenType::tok_const: return "CONST";
        case TokenType::tok_continue: return "CONTINUE";
        case TokenType::tok_else: return "ELSE";
        case TokenType::tok_false: return "FALSE";
        case TokenType::tok_for: return "FOR";
        case TokenType::tok_fun: return "FUN";
        case TokenType::tok_if: return "IF";
        case TokenType::tok_import: return "IMPORT";
        case TokenType::tok_in: return "IN";
        case TokenType::tok_let: return "LET";
        case TokenType::tok_return: return "RETURN";
        case TokenType::tok_true: return "TRUE";
        case TokenType::tok_while: return "WHILE";

        // Signed integer types
        case TokenType::tok_i8: return "I8";
        case TokenType::tok_i16: return "I16";
        case TokenType::tok_i32: return "I32";
        case TokenType::tok_i64: return "I64";

        // Unsigned integer types
        case TokenType::tok_u8: return "U8";
        case TokenType::tok_u16: return "U16";
        case TokenType::tok_u32: return "U32";
        case TokenType::tok_u64: return "U64";

        // Floating point types
        case TokenType::tok_f32: return "F32";
        case TokenType::tok_f64: return "F64";

        // Text types
        case TokenType::tok_str: return "STR";
        case TokenType::tok_char: return "CHAR";

        // Syntax elements
        case TokenType::tok_open_paren: return "OPEN_PAREN";
        case TokenType::tok_close_paren: return "CLOSE_PAREN";
        case TokenType::tok_open_brace: return "OPEN_BRACE";
        case TokenType::tok_close_brace: return "CLOSE_BRACE";
        case TokenType::tok_open_bracket: return "OPEN_BRACKET";
        case TokenType::tok_close_bracket: return "CLOSE_BRACKET";
        case TokenType::tok_fun_return: return "FUN_RETURN";
        case TokenType::tok_comma: return "COMMA";
        case TokenType::tok_semicolon: return "SEMICOLON";

        // Basic operators
        case TokenType::tok_add: return "ADD";
        case TokenType::tok_sub: return "SUB";
        case TokenType::tok_mul: return "MUL";
        case TokenType::tok_div: return "DIV";
        case TokenType::tok_mod: return "MOD";
        case TokenType::tok_bang: return "BANG";

        // Assignment operators
        case TokenType::tok_plus_equals: return "PLUS_EQUALS";
        case TokenType::tok_sub_equals: return "SUB_EQUALS";
        case TokenType::tok_mul_equals: return "MUL_EQUALS";
        case TokenType::tok_div_equals: return "DIV_EQUALS";
        case TokenType::tok_mod_equals: return "MOD_EQUALS";

        // Member access
        case TokenType::tok_member: return "MEMBER";
        case TokenType::tok_namespace_member: return "NAMESPACE_MEMBER";

        // Increment/decrement
        case TokenType::tok_increment: return "INCREMENT";
        case TokenType::tok_decrement: return "DECREMENT";

        // Comparison
        case TokenType::tok_equal: return "EQUAL";
        case TokenType::tok_not_equal: return "NOT_EQUAL";

        // Logical
        case TokenType::tok_and: return "AND";
        case TokenType::tok_or: return "OR";

        // Relational
        case TokenType::tok_less: return "LESS";
        case TokenType::tok_less_equal: return "LESS_EQUAL";
        case TokenType::tok_greater: return "GREATER";
        case TokenType::tok_greater_equal: return "GREATER_EQUAL";

        // Other operators
        case TokenType::tok_assign: return "ASSIGN";
        case TokenType::tok_colon: return "COLON";

        case TokenType::tok_exclusive_range: return "EXCLUSIVE_RANGE";
        case TokenType::tok_inclusive_range: return "INCLUSIVE_RANGE";

        // Literals
        case TokenType::tok_int_literal: return "INT_LITERAL";
        case TokenType::tok_float_literal: return "FLOAT_LITERAL";
        case TokenType::tok_str_literal: return "STRING_LITERAL";
        case TokenType::tok_char_literal: return "CHAR_LITERAL";

        // Identifier
        case TokenType::tok_identifier: return "IDENTIFIER";

        default: return "UNKNOWN";
    }
}
