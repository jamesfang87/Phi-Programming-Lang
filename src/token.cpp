#include "token.hpp"
#include "token_types.hpp"
#include <format>
#include <string>

Token::Token(int line, int col, TokenType type, std::string lexeme) {
    this->line = line;
    this->col = col;
    this->type = type;
    this->lexeme = std::move(lexeme);
}

std::string token_type_to_string(TokenType type) {
    switch (type) {
        case tok_eof: return "EOF";

        case tok_break: return "BREAK";
        case tok_class: return "CLASS";
        case tok_const: return "CONST";
        case tok_continue: return "CONTINUE";
        case tok_else: return "ELSE";
        case tok_elif: return "ELIF";
        case tok_false: return "FALSE";
        case tok_for: return "FOR";
        case tok_fun: return "FUN";
        case tok_if: return "IF";
        case tok_import: return "IMPORT";
        case tok_in: return "IN";
        case tok_let: return "LET";
        case tok_return: return "RETURN";
        case tok_true: return "TRUE";
        case tok_while: return "WHILE";

        case tok_i8: return "I8";
        case tok_i16: return "I16";
        case tok_i32: return "I32";
        case tok_i64: return "I64";

        case tok_u8: return "U8";
        case tok_u16: return "U16";
        case tok_u32: return "U32";
        case tok_u64: return "U64";

        case tok_f32: return "F32";
        case tok_f64: return "F64";

        case tok_str: return "STR";
        case tok_char: return "CHAR";

        case tok_open_paren: return "OPEN_PAREN";
        case tok_close_paren: return "CLOSE_PAREN";
        case tok_open_brace: return "OPEN_BRACE";
        case tok_close_brace: return "CLOSE_BRACE";
        case tok_open_bracket: return "OPEN_BRACKET";
        case tok_close_bracket: return "CLOSE_BRACKET";
        case tok_fun_return: return "FUN_RETURN";
        case tok_comma: return "COMMA";

        case tok_add: return "ADD";
        case tok_sub: return "SUB";
        case tok_mul: return "MUL";
        case tok_div: return "DIV";
        case tok_mod: return "MOD";
        case tok_bang: return "BANG";

        case tok_plus_equals: return "PLUS_EQUALS";
        case tok_sub_equals: return "SUB_EQUALS";
        case tok_mul_equals: return "MUL_EQUALS";
        case tok_div_equals: return "DIV_EQUALS";
        case tok_mod_equals: return "MOD_EQUALS";

        case tok_member: return "MEMBER";
        case tok_namespace_member: return "NAMESPACE_MEMBER";

        case tok_increment: return "INCREMENT";
        case tok_decrement: return "DECREMENT";

        case tok_equal: return "EQUAL";
        case tok_not_equal: return "NOT_EQUAL";

        case tok_and: return "AND";
        case tok_or: return "OR";

        case tok_less: return "LESS";
        case tok_less_equal: return "LESS_EQUAL";
        case tok_greater: return "GREATER";
        case tok_greater_equal: return "GREATER_EQUAL";

        case tok_assign: return "ASSIGN";
        case tok_colon: return "COLON";

        case tok_int_literal: return "INT_LITERAL";
        case tok_float_literal: return "FLOAT_LITERAL";
        case tok_str_literal: return "STRING_LITERAL";
        case tok_char_literal: return "CHAR_LITERAL";
        case tok_identifier: return "IDENTIFIER";

        default: return "UNKNOWN";
    }
}

std::string Token::as_str() const {
    return std::format("[{}] \"{}\" at line {}, column {}", token_type_to_string(type), lexeme, line, col);
}
