#include "Parser/Parser.hpp"

#include <unordered_map>

std::optional<Type> Parser::parse_type() {
    const std::unordered_map<std::string, Type::Primitive> primitive_map = {
        {"i8", Type::Primitive::i8},
        {"i16", Type::Primitive::i16},
        {"i32", Type::Primitive::i32},
        {"i64", Type::Primitive::i64},
        {"u8", Type::Primitive::u8},
        {"u16", Type::Primitive::u16},
        {"u32", Type::Primitive::u32},
        {"u64", Type::Primitive::u64},
        {"f32", Type::Primitive::f32},
        {"f64", Type::Primitive::f64},
        {"str", Type::Primitive::str},
        {"char", Type::Primitive::character},
        {"null", Type::Primitive::null}};

    std::string id = peek_token().get_lexeme();
    auto it = primitive_map.find(id);
    if (it == primitive_map.end() && peek_token().get_type() != TokenType::tok_identifier) {
        throw_parsing_error(peek_token().get_start().line,
                            peek_token().get_start().col,
                            std::format("Invalid token found: {}", peek_token().get_lexeme()),
                            "Expected a valid type here");
        return std::nullopt;
    }

    advance_token();
    if (it == primitive_map.end()) {
        return Type(id);
    } else {
        return Type(it->second);
    }
}
