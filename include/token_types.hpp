#pragma once
enum TokenType {
    tok_eof,
    tok_error,

    // keywords
    tok_break,
    tok_class,
    tok_const,
    tok_continue,
    tok_else,
    tok_elif,
    tok_false,
    tok_for,
    tok_fun,
    tok_if,
    tok_import,
    tok_in,
    tok_let,
    tok_return,
    tok_true,
    tok_while,

    // signed int types
    tok_i8,
    tok_i16,
    tok_i32,
    tok_i64,

    // unsigned int types
    tok_u8,
    tok_u16,
    tok_u32,
    tok_u64,

    // floating point number types
    tok_f32,
    tok_f64,

    // text types
    tok_str,
    tok_char,

    // synatx
    tok_open_paren,
    tok_close_paren,
    tok_open_brace,
    tok_close_brace,
    tok_open_bracket,
    tok_close_bracket,
    tok_fun_return,
    tok_comma,

    // operators
    tok_add,  // +
    tok_sub,  // -
    tok_mul,  // *
    tok_div,  // /
    tok_mod,  // %
    tok_bang, // !

    tok_plus_equals, // +=
    tok_sub_equals,  // -=
    tok_mul_equals,  // *=
    tok_div_equals,  // /=
    tok_mod_equals,  // %=

    tok_member,           // .
    tok_namespace_member, // ::

    tok_increment, // ++
    tok_decrement, // --

    tok_equal,     // ==
    tok_not_equal, // !=

    tok_and, // &&
    tok_or,  // ||

    tok_less,          // <
    tok_less_equal,    // <=
    tok_greater,       // >
    tok_greater_equal, // >=

    tok_assign, // =
    tok_colon,  // :

    tok_int_literal,
    tok_float_literal,
    tok_str_literal,
    tok_char_literal,
    tok_identifier,
};
